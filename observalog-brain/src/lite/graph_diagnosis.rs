//! Gate 5.2-B deterministic graph localization prototype.
//!
//! Shadow-only pure evaluator. It ranks service candidates from typed evidence,
//! never treats timestamp proximity or shared trace/request membership as causality.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub id: u64,
    pub service: String,
    pub instance: Option<String>,
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
    pub contrast_evidence: usize,
    pub healthy_peer_events: usize,
    /// Exact replica when a single failed instance contrasts with healthy peers.
    pub isolated_instance: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    pub abstained: bool,
    pub likely_initiator: Option<String>,
    /// More specific replica target when a unique failed instance is isolated.
    pub likely_instance: Option<String>,
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
            contrast_evidence: 0, healthy_peer_events: 0, isolated_instance: None, reasons: Vec::new(),
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
            if parent_event.service == child_event.service || !child_event.is_failure || parent_event.timestamp_ms > child_event.timestamp_ms { continue; }
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

    // Typed cohort contrast: failures isolated to a version or replica while
    // comparable healthy peers exist. This is stronger than raw failure volume.
    let mut failed_versions: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut healthy_versions: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut failed_instances: BTreeMap<(String, String), std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut healthy_instances: BTreeMap<(String, String), std::collections::BTreeSet<String>> = BTreeMap::new();
    for event in events {
        if let Some(version) = event.version.as_ref() {
            if event.is_failure {
                failed_versions.entry(event.service.clone()).or_default().insert(version.clone());
                if let Some(instance) = event.instance.as_ref() {
                    failed_instances.entry((event.service.clone(), version.clone())).or_default().insert(instance.clone());
                }
            } else {
                healthy_versions.entry(event.service.clone()).or_default().insert(version.clone());
                if let Some(instance) = event.instance.as_ref() {
                    healthy_instances.entry((event.service.clone(), version.clone())).or_default().insert(instance.clone());
                }
            }
        }
    }
    for candidate in scores.values_mut() {
        let version_contrast = failed_versions.get(&candidate.service).is_some_and(|failed|
            healthy_versions.get(&candidate.service).is_some_and(|healthy|
                failed.iter().any(|fv| healthy.iter().any(|hv| fv != hv))));
        let mut isolated_instances = std::collections::BTreeSet::new();
        let instance_contrast = failed_instances.iter().any(|((service, version), failed)| {
            if service != &candidate.service {
                return false;
            }
            let Some(healthy) = healthy_instances.get(&(service.clone(), version.clone())) else {
                return false;
            };
            let has_contrast = failed.iter().any(|fi| healthy.iter().any(|hi| fi != hi));
            if has_contrast && failed.len() == 1 {
                if let Some(instance) = failed.iter().next() {
                    isolated_instances.insert(instance.clone());
                }
            }
            has_contrast
        });
        candidate.isolated_instance = if isolated_instances.len() == 1 {
            isolated_instances.into_iter().next()
        } else {
            None
        };
        if version_contrast || instance_contrast {
            candidate.contrast_evidence += 1;
            candidate.score += 1.5;
            candidate.reasons.push(if version_contrast {
                "failure isolated to version with healthy alternate-version peers".into()
            } else {
                "failure isolated to replica with healthy same-version peers".into()
            });
            if let Some(instance) = candidate.isolated_instance.as_ref() {
                candidate.reasons.push(format!(
                    "single failing instance {instance} contrasts with healthy same-version peers"
                ));
            }
        }
    }

    // Healthy peer counts are retained as context, not a penalty that obscures a replica/version contrast.
    // metadata is present. It never removes candidates or proves root cause.
    let snapshot: Vec<(String, Option<String>, bool)> = events.iter()
        .map(|e| (e.service.clone(), e.version.clone(), e.is_failure)).collect();
    for candidate in scores.values_mut() {
        if let Some(version) = events.iter().find(|e| e.service == candidate.service).and_then(|e| e.version.as_ref()) {
            let successes = snapshot.iter().filter(|(service, v, failure)| service == &candidate.service && v.as_ref() == Some(version) && !failure).count();
            candidate.healthy_peer_events = successes;
            if successes > 0 {
                candidate.reasons.push(format!("{} healthy same-service/version control events", successes));
            }
        }
    }

    let mut candidates: Vec<Candidate> = scores.into_values().collect();
    candidates.sort_by(|a,b| b.score.total_cmp(&a.score).then_with(|| a.service.cmp(&b.service)));
    let top_is_eligible = candidates.first().is_some_and(|c| c.score >= ABSTAIN_SCORE
        && (c.upstream_failure_links > 0 || c.contrast_evidence > 0));
    let ambiguous_top = candidates.get(1).is_some_and(|second|
        candidates.first().is_some_and(|first| (first.score - second.score).abs() < 1e-9));
    let winner = if top_is_eligible && !ambiguous_top {
        candidates.first().map(|c| c.service.clone())
    } else {
        None
    };
    let abstained = winner.is_none();
    let likely_instance = winner.as_ref().and_then(|service| {
        candidates.iter().find(|candidate| &candidate.service == service)
            .and_then(|candidate| candidate.isolated_instance.clone())
    });
    Diagnosis {
        abstained,
        likely_initiator: winner,
        likely_instance,
        candidates,
        note: if ambiguous_top {
            "Top candidates are tied; abstain instead of choosing by lexical order."
        } else if abstained {
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
        DiagnosticEvent { id, service: service.into(), instance: None, timestamp_ms: ts, is_failure: failure,
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
    fn tied_multiple_failure_services_abstain_instead_of_lexical_winner() {
        let events = vec![
            e(1,"identity",10,true,Some("v1")), e(2,"identity",11,true,Some("v1")),
            e(3,"payments",12,true,Some("v1")), e(4,"payments",13,true,Some("v1")),
        ];
        let d = rank(&events, &[]);
        assert!(d.abstained);
        assert_eq!(d.likely_initiator, None);
        assert_eq!(d.candidates[0].score, d.candidates[1].score);
    }

    #[test]
    fn replica_contrast_resolves_a_unique_failed_instance() {
        let events = vec![
            DiagnosticEvent { id: 1, service: "identity".into(), instance: Some("identity-01".into()), timestamp_ms: 1,
                is_failure: false, error_code: None, dependency: None, version: Some("2.2.1".into()) },
            DiagnosticEvent { id: 2, service: "identity".into(), instance: Some("identity-02".into()), timestamp_ms: 2,
                is_failure: false, error_code: None, dependency: None, version: Some("2.2.1".into()) },
            DiagnosticEvent { id: 3, service: "identity".into(), instance: Some("identity-03".into()), timestamp_ms: 3,
                is_failure: true, error_code: Some("JWT_ISSUER_MISMATCH".into()), dependency: None, version: Some("2.2.1".into()) },
        ];
        let d = rank(&events, &[]);
        assert_eq!(d.likely_initiator.as_deref(), Some("identity"));
        assert_eq!(d.likely_instance.as_deref(), Some("identity-03"));
        assert!(!d.abstained);
    }

    #[test]
    fn multiple_failed_replicas_do_not_get_arbitrarily_reduced_to_one_instance() {
        let events = vec![
            DiagnosticEvent { id: 1, service: "identity".into(), instance: Some("identity-01".into()), timestamp_ms: 1,
                is_failure: false, error_code: None, dependency: None, version: Some("2.2.1".into()) },
            DiagnosticEvent { id: 2, service: "identity".into(), instance: Some("identity-02".into()), timestamp_ms: 2,
                is_failure: false, error_code: None, dependency: None, version: Some("2.2.1".into()) },
            DiagnosticEvent { id: 3, service: "identity".into(), instance: Some("identity-03".into()), timestamp_ms: 3,
                is_failure: true, error_code: Some("JWT_ISSUER_MISMATCH".into()), dependency: None, version: Some("2.2.1".into()) },
            DiagnosticEvent { id: 4, service: "identity".into(), instance: Some("identity-04".into()), timestamp_ms: 4,
                is_failure: true, error_code: Some("JWT_ISSUER_MISMATCH".into()), dependency: None, version: Some("2.2.1".into()) },
        ];
        let d = rank(&events, &[]);
        assert_eq!(d.likely_initiator.as_deref(), Some("identity"));
        assert_eq!(d.likely_instance, None);
    }

    #[test]
    fn failure_volume_without_typed_causal_or_cohort_evidence_abstains() {
        let events = vec![
            e(1,"identity",10,true,Some("v1")), e(2,"identity",11,true,Some("v1")),
            e(3,"media",12,true,Some("v1")),
        ];
        let d = rank(&events, &[]);
        assert!(d.abstained);
    }

    #[test]
    fn version_regression_contrast_can_localize_service() {
        let mut old_ok=e(1,"orders",10,false,Some("4.7.2"));
        old_ok.instance=Some("old".into());
        let mut new_bad=e(2,"orders",20,true,Some("4.8.0"));
        new_bad.instance=Some("new".into());
        let d=rank(&[old_ok,new_bad],&[]);
        assert_eq!(d.likely_initiator.as_deref(),Some("orders"));
        assert_eq!(d.candidates[0].contrast_evidence,1);
    }

    #[test]
    fn failed_parent_after_child_does_not_create_upstream_causality() {
        let events=vec![e(1,"orders",30,true,Some("v1")),e(2,"payments",10,true,Some("v1"))];
        let d=rank(&events,&[EvidenceEdge::ParentChild{parent:1,child:2}]);
        assert!(d.abstained);
        assert_eq!(d.candidates.iter().find(|c|c.service=="orders").unwrap().upstream_failure_links,0);
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
