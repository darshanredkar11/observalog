use observalog_brain::lite::graph_diagnosis::{rank, DiagnosticEvent, EvidenceEdge};
use serde_json::{json, Value};
use std::io::{self, Read};

fn field<'a>(event: &'a Value, key: &str) -> Option<&'a Value> {
    event.get(key)
        .or_else(|| event.get("attrs").and_then(|attrs| attrs.get(key)))
        .or_else(|| event.get("attributes").and_then(|attrs| attrs.get(key)))
}

fn text(event: &Value, key: &str) -> Option<String> {
    field(event, key)?.as_str().map(str::to_owned)
}

fn same_flow(left: &Value, right: &Value) -> bool {
    let trace_match = text(left, "trace_id").zip(text(right, "trace_id"))
        .is_some_and(|(a, b)| !a.is_empty() && a == b);
    let request_match = text(left, "request_id").zip(text(right, "request_id"))
        .is_some_and(|(a, b)| !a.is_empty() && a == b);
    trace_match || request_match
}

fn is_failure(event: &Value) -> bool {
    let level = text(event, "level").or_else(|| text(event, "severity"))
        .unwrap_or_default().to_ascii_uppercase();
    let outcome = text(event, "outcome").unwrap_or_default().to_ascii_lowercase();
    matches!(level.as_str(), "ERROR" | "FATAL" | "CRITICAL")
        || matches!(outcome.as_str(), "failure" | "failed" | "error")
}

fn evaluate(events: &[Value]) -> Value {
    let diagnostic_events: Vec<DiagnosticEvent> = events.iter().enumerate().map(|(index, event)| {
        DiagnosticEvent {
            id: index as u64 + 1,
            service: text(event, "service").unwrap_or_else(|| "unknown-service".to_owned()),
            instance: text(event, "instance"),
            timestamp_ms: event.get("timestamp_ms").or_else(|| event.get("offset_ms"))
                .and_then(Value::as_i64).unwrap_or_default(),
            is_failure: is_failure(event),
            error_code: text(event, "error_code"),
            dependency: text(event, "dependency").or_else(|| text(event, "dependency_name")),
            version: text(event, "version"),
        }
    }).collect();

    // Build explicit directed span edges from the same identities used by the
    // test fixture. A shared trace/request alone does not create a causal edge.
    let mut edges = Vec::new();
    for (child_index, child) in events.iter().enumerate() {
        let Some(parent_span) = text(child, "parent_span_id") else { continue; };
        for (parent_index, parent) in events.iter().enumerate() {
            if parent_index == child_index || text(parent, "span_id").as_deref() != Some(parent_span.as_str()) {
                continue;
            }
            if !same_flow(parent, child) || text(parent, "tenant") != text(child, "tenant") {
                continue;
            }
            edges.push(EvidenceEdge::ParentChild {
                parent: parent_index as u64 + 1,
                child: child_index as u64 + 1,
            });
        }
    }

    let diagnosis = rank(&diagnostic_events, &edges);
    let candidates: Vec<Value> = diagnosis.candidates.iter().map(|candidate| json!({
        "service": candidate.service,
        "isolated_instance": candidate.isolated_instance.as_deref(),
        "score": candidate.score,
        "direct_failures": candidate.direct_failures,
        "upstream_failure_links": candidate.upstream_failure_links,
        "downstream_failure_links": candidate.downstream_failure_links,
        "contrast_evidence": candidate.contrast_evidence,
        "healthy_peer_events": candidate.healthy_peer_events,
        "reasons": candidate.reasons,
    })).collect();

    json!({
        "abstained": diagnosis.abstained,
        "likely_initiator": diagnosis.likely_initiator,
        "likely_instance": diagnosis.likely_instance.as_deref(),
        "candidates": candidates,
        "note": diagnosis.note,
    })
}

fn main() {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read input: {error}");
        std::process::exit(2);
    }
    let events: Vec<Value> = match serde_json::from_str(&input) {
        Ok(events) => events,
        Err(error) => {
            eprintln!("invalid event JSON: {error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = serde_json::to_writer(io::stdout(), &evaluate(&events)) {
        eprintln!("failed to write diagnosis: {error}");
        std::process::exit(2);
    }
}
