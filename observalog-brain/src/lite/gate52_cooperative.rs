//! Gate 5.2 cooperative specialist + graph research evaluator.
//!
//! A specialist can propose a narrow, evidence-linked hypothesis. The verifier
//! checks that evidence IDs exist, evidence is tenant-consistent, and the target
//! is grounded in those evidence records. If no specialist survives verification,
//! the existing deterministic graph ranker is used as a conservative fallback.
//! This module is research-only and is not wired into production findings.

use crate::lite::gate52_specialists::{self, Hypothesis};
use super::graph_diagnosis::{rank, Candidate, DiagnosticEvent, EvidenceEdge};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CooperativeDiagnosis {
    pub accepted_specialist_hypotheses: Vec<Hypothesis>,
    pub rejected_specialist_hypotheses: usize,
    pub graph_fallback_target: Option<String>,
    pub primary_target: Option<String>,
    pub primary_target_type: Option<String>,
    pub decision_source: &'static str,
    pub abstained: bool,
}

fn text_field(event: &Value, key: &str) -> Option<String> {
    event.get(key)
        .or_else(|| event.get("attrs").and_then(|attrs| attrs.get(key)))
        .or_else(|| event.get("attributes").and_then(|attrs| attrs.get(key)))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn event_id(event: &Value) -> Option<String> {
    event.get("event_id").or_else(|| event.get("id"))
        .and_then(|value| value.as_str().map(str::to_owned)
            .or_else(|| value.as_u64().map(|number| number.to_string())))
}

fn tenant_id(event: &Value) -> Option<String> {
    text_field(event, "tenant_id").or_else(|| text_field(event, "tenant"))
}

fn verify_hypothesis(events: &[Value], hypothesis: &Hypothesis) -> bool {
    if hypothesis.evidence_ids.is_empty() {
        return false;
    }

    let by_id: BTreeMap<String, &Value> = events.iter()
        .filter_map(|event| event_id(event).map(|id| (id, event)))
        .collect();

    let mut evidence = Vec::with_capacity(hypothesis.evidence_ids.len());
    for id in &hypothesis.evidence_ids {
        let Some(event) = by_id.get(id) else { return false; };
        evidence.push(*event);
    }

    let first_tenant = tenant_id(evidence[0]);
    if evidence.iter().skip(1).any(|event| tenant_id(event) != first_tenant) {
        return false;
    }

    evidence.iter().any(|event| match hypothesis.target_type {
        "service" => text_field(event, "service").as_deref() == Some(hypothesis.target.as_str()),
        "component" => {
            text_field(event, "service").as_deref() == Some(hypothesis.target.as_str())
                || text_field(event, "component").as_deref() == Some(hypothesis.target.as_str())
                || text_field(event, "component_name").as_deref() == Some(hypothesis.target.as_str())
        }
        "instance" => text_field(event, "instance").as_deref() == Some(hypothesis.target.as_str()),
        _ => false,
    })
}

fn same_flow(parent: &Value, child: &Value) -> bool {
    if tenant_id(parent) != tenant_id(child) {
        return false;
    }

    let parent_trace = text_field(parent, "trace_id").filter(|value| !value.is_empty());
    let child_trace = text_field(child, "trace_id").filter(|value| !value.is_empty());
    let parent_request = text_field(parent, "request_id").filter(|value| !value.is_empty());
    let child_request = text_field(child, "request_id").filter(|value| !value.is_empty());

    if parent_trace.is_some() && child_trace.is_some() && parent_trace != child_trace {
        return false;
    }
    if parent_request.is_some() && child_request.is_some() && parent_request != child_request {
        return false;
    }

    (parent_trace.is_some() && parent_trace == child_trace)
        || (parent_request.is_some() && parent_request == child_request)
}

fn graph_fallback(events: &[Value]) -> crate::lite::graph_diagnosis::Diagnosis {
    let diagnostic_events: Vec<DiagnosticEvent> = events.iter().enumerate().map(|(index, event)| {
        let level = text_field(event, "level")
            .or_else(|| text_field(event, "severity"))
            .unwrap_or_default().to_ascii_uppercase();
        let outcome = text_field(event, "outcome").unwrap_or_default().to_ascii_lowercase();
        DiagnosticEvent {
            id: index as u64 + 1,
            service: text_field(event, "service").unwrap_or_else(|| "unknown-service".to_owned()),
            instance: text_field(event, "instance"),
            timestamp_ms: event.get("timestamp_ms").or_else(|| event.get("offset_ms"))
                .and_then(Value::as_i64).unwrap_or_default(),
            is_failure: matches!(level.as_str(), "ERROR" | "FATAL" | "CRITICAL")
                || matches!(outcome.as_str(), "failure" | "failed" | "error"),
            error_code: text_field(event, "error_code"),
            dependency: text_field(event, "dependency").or_else(|| text_field(event, "dependency_name")),
            version: text_field(event, "version"),
        }
    }).collect();

    let mut edges = Vec::new();
    for (child_index, child) in events.iter().enumerate() {
        let Some(parent_span) = text_field(child, "parent_span_id") else { continue; };
        for (parent_index, parent) in events.iter().enumerate() {
            if child_index == parent_index
                || text_field(parent, "span_id").as_deref() != Some(parent_span.as_str())
                || !same_flow(parent, child)
            {
                continue;
            }
            edges.push(EvidenceEdge::ParentChild {
                parent: parent_index as u64 + 1,
                child: child_index as u64 + 1,
            });
        }
    }

    rank(&diagnostic_events, &edges)
}

pub fn analyze(events: &[Value]) -> CooperativeDiagnosis {
    let raw_hypotheses = gate52_specialists::analyze(events);
    let mut accepted = Vec::new();
    let mut rejected = 0;

    for hypothesis in raw_hypotheses {
        if verify_hypothesis(events, &hypothesis) {
            accepted.push(hypothesis);
        } else {
            rejected += 1;
        }
    }

    // Do not silently choose between multiple distinct specialist targets.
    if accepted.len() == 1 {
        let hypothesis = &accepted[0];
        return CooperativeDiagnosis {
            accepted_specialist_hypotheses: accepted,
            rejected_specialist_hypotheses: rejected,
            graph_fallback_target: None,
            primary_target: Some(hypothesis.target.clone()),
            primary_target_type: Some(hypothesis.target_type.to_owned()),
            decision_source: "specialist",
            abstained: false,
        };
    }

    if !accepted.is_empty() {
        return CooperativeDiagnosis {
            accepted_specialist_hypotheses: accepted,
            rejected_specialist_hypotheses: rejected,
            graph_fallback_target: None,
            primary_target: None,
            primary_target_type: None,
            decision_source: "ambiguous-specialists",
            abstained: true,
        };
    }

    let graph = graph_fallback(events);
    let fallback = if graph.abstained { None } else { graph.likely_initiator };
    CooperativeDiagnosis {
        accepted_specialist_hypotheses: accepted,
        rejected_specialist_hypotheses: rejected,
        graph_fallback_target: fallback.clone(),
        primary_target: fallback,
        primary_target_type: graph.likely_initiator.map(|_| "service".to_owned()),
        decision_source: if graph.abstained { "abstain" } else { "graph" },
        abstained: graph.abstained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hypothesis(target_type: &'static str, target: &str, evidence_ids: Vec<&str>) -> Hypothesis {
        Hypothesis {
            specialist: "test-specialist",
            target_type,
            target: target.to_owned(),
            evidence_ids: evidence_ids.into_iter().map(str::to_owned).collect(),
            explanation: "test hypothesis",
        }
    }

    #[test]
    fn verifier_requires_present_evidence_and_grounded_target() {
        let events = vec![
            json!({"event_id":"e1","service":"identity","tenant":"tenant-a"}),
            json!({"event_id":"e2","service":"orders","tenant":"tenant-a"}),
        ];
        assert!(verify_hypothesis(&events, &hypothesis("service", "identity", vec!["e1", "e2"])));
        assert!(!verify_hypothesis(&events, &hypothesis("service", "payments", vec!["e1", "e2"])));
        assert!(!verify_hypothesis(&events, &hypothesis("service", "identity", vec!["missing"])));
        assert!(!verify_hypothesis(&events, &hypothesis("service", "identity", vec![])));
    }

    #[test]
    fn verifier_rejects_evidence_crossing_tenant_boundaries() {
        let events = vec![
            json!({"event_id":"e1","service":"identity","tenant":"tenant-a"}),
            json!({"event_id":"e2","service":"orders","tenant":"tenant-b"}),
        ];
        assert!(!verify_hypothesis(&events, &hypothesis("service", "identity", vec!["e1", "e2"])));
    }

    #[test]
    fn graph_fallback_remains_available_when_no_specialist_matches() {
        let events = vec![
            json!({"event_id":"e1","service":"orders","instance":"orders-old","offset_ms":1,"version":"1.0","level":"INFO"}),
            json!({"event_id":"e2","service":"orders","instance":"orders-new","offset_ms":2,"version":"2.0","level":"ERROR","error_code":"REGRESSION"}),
        ];
        let result = analyze(&events);
        assert_eq!(result.decision_source, "graph");
        assert_eq!(result.primary_target.as_deref(), Some("orders"));
        assert!(!result.abstained);
    }
}
