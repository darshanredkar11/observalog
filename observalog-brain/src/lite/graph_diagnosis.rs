//! Gate 5.2-B deterministic graph localization prototype.
//!
//! Shadow-only pure evaluator. It ranks service candidates from typed evidence,
//! never treats timestamp proximity or shared trace/request membership as causality.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub id: u64,
    pub service: String,
    pub timestamp_ms: i64,
    pub is_failure: bool,
    pub error_code: Option<String>,
    pub dependency: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceEdge {
    /// Direction is explicitly parent -> child from parent_span_id/span_id.
    ParentChild { parent: u64, child: u64 },
    /// Correlation-only links; intentionally do not establish cause.
    SameTrace { a: u64, b: u64 },
    SameRequest { a: u64, b: u64 },
    SameBusinessEntity { a: u64, b: u64 },
    SameMessage { a: u64, b: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub service: String,
    pub score: f64,
    pub direct_failures: usize,
    pub upstream_failure_links: usize,
    pub downstream_failure_links: usize,
    pub healthy_peer_events: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    pub abstained: bool,
    pub likely_initiator: Option<String>,
    pub candidates: Vec<Candidate>,
    pub note: &'static str,
}

const ABSTAIN_SCORE: f64 = 2.0;

/// Conservative deterministic ranking. Same-trace/request/entity/message edges
/// only establish context. Only explicit directed parent-child structure can
/// add upstream localization evidence; timestamps are corroborative.
pub fn rank(events: &[DiagnosticEvent], edges: &[EvidenceEdge]) -> Diagnosis {
    let by_id: BTreeMap<u64, &DiagnosticEvent> = events.iter().map(|e| (e.id, e)).collect();
    let mut scores: BTreeMap<String, Candidate> = BTreeMap::new();

    for event in events {
        let candidate = scores.entry(event.service.clone()).or_insert(Candidate {
            service: event.service.clone(), score: 0.0, direct_failures: 0,
            upstream_failure_links: 0, downstream_failure_links: 0,
            healthy_peer_events: 0, reasons: Vec::new(),
        });
        if event.is_failure {
            candidate.direct_failures += 1;
            candidate.score += 1.0;
            candidate.reasons.push(format!("direct failure event {}", event.id));
            if event.error_code.is_some() || event.dependency.is_some() {
                candidate.score += 0.5;
                candidate.reasons.push(format!("failure {} has error/dependency detail", event.id));
            }
        } else {
            candidate.healthy_peer_events += 1;
        }
    }

    for edge in edges {
        if let EvidenceEdge::ParentChild { parent, child } = edge {
            let (Some(parent_event), Some(child_event)) = (by_id.get(parent), by_id.get(child)) else { continue; };
            if parent_event.service == child_event.service || !child_event.is_failure { continue; }
            if parent_event.is_failure {
                let parent_candidate = scores.get_mut(&parent_event.service).expect("candidate exists");
                parent_candidate.upstream_failure_links += 1;
                parent_candidate.score += 1.5;
                parent_candidate.reasons.push(format!(
                    "failed parent event {} structurally precedes failed child event {}", parent, child
                ));
                let child_candidate = scores.get_mut(&child_event.service).expect("candidate exists");
                child_candidate.downstream_failure_links += 1;
                child_candidate.score -= 0.75;
                child_candidate.reasons.push(format!("failure {} has an explicit failed parent {}", child, parent));
            }
            // Time only corroborates the direction supplied by the span relation.
            if parent_event.timestamp_ms <= child_event.timestamp_ms && parent_event.is_failure {
                scores.get_mut(&parent_event.service).expect("candidate exists").score += 0.25;
            }
        }
    }

    // Healthy peer evidence is only a bounded contrast penalty when same-version
    // metadata is present. It never removes candidates or proves root cause.
    let snapshot: Vec<(String, Option<String>, bool)> = events.iter()
        .map(|e| (e.service.clone(), e.version.clone(), e.is_failure)).collect();
    for candidate in scores.values_mut() {
        if let Some(version) = events.iter().find(|e| e.service == candidate.service).and_then(|e| e.version.as_ref()) {
            let successes = snapshot.iter().filter(|(service, v, failure)| service == &candidate.service && v.as_ref() == Some(version) && !failure).count();
            candidate.healthy_peer_events = successes;
            let penalty = (successes as f64 * 0.25).min(1.0);
            if penalty > 0.0 {
                candidate.score -= penalty;
                candidate.reasons.push(format!("{} healthy same-service/version control events (bounded contrast)", successes));
            }
        }
    }

    let mut candidates: Vec<Candidate> = scores.into_values().collect();
    candidates.sort_by(|a,b| b.score.total_cmp(&a.score).then_with(|| a.service.cmp(&b.service)));
    let winner = candidates.first().filter(|c| c.score >= ABSTAIN_SCORE
        && (c.direct_failures > 0 || c.upstream_failure_links > 0)).map(|c| c.service.clone());
    let abstained = winner.is_none();
    Diagnosis {
        abstained,
        likely_initiator: winner,
        candidates,
        note: if abstained {
            "Insufficient graph-supported evidence; abstain rather than infer cause from temporal or correlation-only links."
        } else {
            "Ranked hypothesis only; graph support does not prove causality."
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: u64, service: &str, ts: i64, failure: bool, version: Option<&str>) -> DiagnosticEvent {
        DiagnosticEvent { id, service: service.into(), timestamp_ms: ts, is_failure: failure,
            error_code: failure.then(|| "E_FAIL".into()), dependency: None, version: version.map(str::to_string) }
    }

    #[test]
    fn explicit_failed_parent_ranks_upstream_over_downstream_symptom() {
        let events = vec![e(1,"tax",10,true,Some("v1")), e(2,"checkout",20,true,Some("v1"))];
        let d = rank(&events, &[EvidenceEdge::ParentChild { parent:1, child:2 }]);
        assert_eq!(d.likely_initiator.as_deref(), Some("tax"));
        assert!(!d.abstained);
        assert!(d.candidates[0].upstream_failure_links > 0);
        assert!(d.candidates.iter().find(|c| c.service=="checkout").unwrap().score < d.candidates[0].score);
    }

    #[test]
    fn same_trace_only_cannot_create_root_cause() {
        let events = vec![e(1,"tax",10,true,Some("v1")), e(2,"checkout",11,true,Some("v1"))];
        let d = rank(&events, &[EvidenceEdge::SameTrace { a:1, b:2 }]);
        assert!(d.abstained);
        assert_eq!(d.likely_initiator, None);
    }

    #[test]
    fn time_order_without_typed_edge_is_not_causal() {
        let events = vec![e(1,"tax",1,true,Some("v1")), e(2,"checkout",100,true,Some("v1"))];
        assert!(rank(&events, &[]).abstained);
    }

    #[test]
    fn healthy_controls_reduce_score_but_do_not_override_typed_evidence() {
        let events = vec![e(1,"tax",10,true,Some("v1")), e(2,"checkout",20,true,Some("v1")),
            e(3,"tax",30,false,Some("v1")), e(4,"tax",40,false,Some("v1"))];
        let d = rank(&events, &[EvidenceEdge::ParentChild { parent:1, child:2 }]);
        assert_eq!(d.likely_initiator.as_deref(), Some("tax"));
        assert_eq!(d.candidates[0].healthy_peer_events, 2);
    }

    #[test]
    fn unrelated_concurrent_failures_abstain() {
        let events = vec![e(1,"tax",10,true,Some("v1")), e(2,"billing",10,true,Some("v1"))];
        assert!(rank(&events, &[EvidenceEdge::SameRequest { a:1, b:2 }]).abstained);
    }

    #[test]
    fn unknown_or_missing_event_ids_are_ignored_in_edges() {
        let events = vec![e(1,"tax",10,true,Some("v1")), e(2,"checkout",20,true,Some("v1"))];
        let d = rank(&events, &[EvidenceEdge::ParentChild { parent:99, child:2 }]);
        assert!(d.abstained);
    }
}
