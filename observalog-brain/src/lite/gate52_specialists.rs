//! Gate 5.2 research-only specialist baselines.
//!
//! These specialists consume preserved event fields/attributes and emit bounded
//! hypotheses with evidence IDs. They are NOT wired into the production pipeline.
//! Each is intentionally narrower than a general RCA model and is tested against
//! positive and negative incident fixtures before any integration is considered.

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hypothesis {
    pub specialist: &'static str,
    pub target_type: &'static str,
    pub target: String,
    pub evidence_ids: Vec<String>,
    pub explanation: &'static str,
}

fn object_field<'a>(event: &'a Value, key: &str) -> Option<&'a Value> {
    event.get(key).or_else(|| event.get("attrs").and_then(|v| v.get(key)))
        .or_else(|| event.get("attributes").and_then(|v| v.get(key)))
}

fn string_field(event: &Value, key: &str) -> Option<String> {
    object_field(event, key)?.as_str().map(str::to_owned)
}

fn number_field(event: &Value, key: &str) -> Option<f64> {
    object_field(event, key)?.as_f64().filter(|v| v.is_finite())
}

fn event_id(event: &Value) -> String {
    event.get("event_id").or_else(|| event.get("id"))
        .and_then(|v| v.as_str().map(str::to_owned).or_else(|| v.as_u64().map(|n| n.to_string())))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn service(event: &Value) -> Option<String> {
    string_field(event, "service")
}

fn instance(event: &Value) -> Option<String> {
    string_field(event, "instance")
}

fn timestamp(event: &Value) -> Option<i64> {
    event.get("timestamp_ms").or_else(|| event.get("offset_ms"))
        .and_then(Value::as_i64)
}

fn is_failure(event: &Value) -> bool {
    let level = string_field(event, "level")
        .or_else(|| string_field(event, "severity"))
        .unwrap_or_default().to_ascii_uppercase();
    let outcome = string_field(event, "outcome").unwrap_or_default().to_ascii_lowercase();
    matches!(level.as_str(), "ERROR" | "FATAL" | "CRITICAL")
        || matches!(outcome.as_str(), "failure" | "failed" | "error")
}

fn same_flow(left: &Value, right: &Value) -> bool {
    let left_tenant = string_field(left, "tenant_id").or_else(|| string_field(left, "tenant"));
    let right_tenant = string_field(right, "tenant_id").or_else(|| string_field(right, "tenant"));
    if left_tenant != right_tenant {
        return false;
    }

    let left_trace = string_field(left, "trace_id").filter(|value| !value.is_empty());
    let right_trace = string_field(right, "trace_id").filter(|value| !value.is_empty());
    let left_request = string_field(left, "request_id").filter(|value| !value.is_empty());
    let right_request = string_field(right, "request_id").filter(|value| !value.is_empty());

    if left_trace.is_some() && right_trace.is_some() && left_trace != right_trace {
        return false;
    }
    if left_request.is_some() && right_request.is_some() && left_request != right_request {
        return false;
    }

    left_trace.is_some() && left_trace == right_trace
        || left_request.is_some() && left_request == right_request
}

/// Conventional signature detector for explicit PostgreSQL deadlock evidence.
/// The DB error is the cause signal; application rollback/retry records are
/// retained as context rather than allowed to tie the database event.
pub fn database_deadlock(events: &[Value]) -> Option<Hypothesis> {
    let event = events.iter().find(|event| {
        let code = string_field(event, "error_code").unwrap_or_default();
        let message = string_field(event, "message").unwrap_or_default().to_ascii_lowercase();
        code == "40P01" || message.contains("deadlock detected")
    })?;
    let target = service(event)?;
    Some(Hypothesis {
        specialist: "database-deadlock-signature",
        target_type: "service",
        target,
        evidence_ids: vec![event_id(event)],
        explanation: "Explicit PostgreSQL deadlock signature (SQLSTATE 40P01 / deadlock-detected message).",
    })
}

/// Request-scoped authentication sequence rule. A specific identity validation
/// failure must precede an auth-context-missing symptom in the same flow.
pub fn auth_cascade(events: &[Value]) -> Option<Hypothesis> {
    for root in events.iter().filter(|e| is_failure(e)) {
        let code = string_field(root, "error_code").unwrap_or_default().to_ascii_uppercase();
        let identity_service = service(root).unwrap_or_default();
        let is_identity_failure = (identity_service.contains("identity") || identity_service.contains("auth"))
            && (code.starts_with("JWT_") || code.starts_with("TOKEN_")
                || code.contains("ISSUER_MISMATCH") || code.contains("AUDIENCE_MISMATCH"));
        if !is_identity_failure { continue; }
        let root_ts = timestamp(root);
        for symptom in events.iter().filter(|e| is_failure(e)) {
            if event_id(root) == event_id(symptom) { continue; }
            let symptom_code = string_field(symptom, "error_code").unwrap_or_default().to_ascii_uppercase();
            if symptom_code != "AUTH_CONTEXT_MISSING" { continue; }
            let ordered = root_ts.zip(timestamp(symptom)).is_some_and(|(a, b)| a <= b);
            if ordered && same_flow(root, symptom) {
                return Some(Hypothesis {
                    specialist: "auth-cascade-signature",
                    target_type: "service",
                    target: identity_service,
                    evidence_ids: vec![event_id(root), event_id(symptom)],
                    explanation: "Identity token validation failure precedes missing-auth-context symptom in the same trace/request.",
                });
            }
        }
    }
    None
}

/// Detect repeated delivery of one message followed by multiple distinct side
/// effects without an idempotency key. Target is the consumer, not the downstream
/// service that happens to emit the duplicate-side-effect error.
pub fn duplicate_delivery(events: &[Value]) -> Option<Hypothesis> {
    let mut by_message: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for event in events {
        if let Some(message_id) = string_field(event, "message_id") {
            if !message_id.is_empty() {
                by_message.entry(message_id).or_default().push(event);
            }
        }
    }

    for (message_id, records) in by_message {
        let mut consumers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut effects = BTreeSet::new();
        let mut has_missing_idempotency = false;
        let mut evidence = Vec::new();

        for event in &records {
            let delivery_attempt = number_field(event, "delivery_attempt").unwrap_or(0.0);
            let event_name = string_field(event, "event_name").unwrap_or_default().to_ascii_lowercase();
            let message = string_field(event, "message").unwrap_or_default().to_ascii_lowercase();
            let is_delivery = delivery_attempt > 0.0 || event_name.contains("receive")
                || message.contains("message received");
            if is_delivery {
                if let Some(consumer) = service(event) {
                    consumers.entry(consumer).or_default().insert(event_id(event));
                }
                evidence.push(event_id(event));
            }

            if let Some(side_effect) = string_field(event, "reservation_id")
                .or_else(|| string_field(event, "side_effect_id"))
            {
                effects.insert(side_effect);
                evidence.push(event_id(event));
                let no_key = object_field(event, "idempotency_key").is_none_or(Value::is_null)
                    || string_field(event, "idempotency_key").is_some_and(|v| v.trim().is_empty());
                has_missing_idempotency |= no_key;
            }
        }

        if consumers.values().map(BTreeSet::len).sum::<usize>() >= 2
            && effects.len() >= 2 && has_missing_idempotency
        {
            if let Some((consumer, _)) = consumers.into_iter()
                .max_by_key(|(_, delivery_ids)| delivery_ids.len())
            {
                evidence.sort();
                evidence.dedup();
                return Some(Hypothesis {
                    specialist: "message-idempotency-invariant",
                    target_type: "component",
                    target: consumer,
                    evidence_ids: evidence,
                    explanation: "One message was delivered repeatedly and produced distinct side effects without an idempotency key.",
                });
            }
        }
        let _ = message_id;
    }
    None
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() { return None; }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

/// Robust trend specialist using Theil-Sen median pairwise slopes over heap and
/// GC-pause samples. It requires an explicit OOM endpoint and meaningful positive
/// movement in both signals; a single high point is not enough.
pub fn resource_exhaustion(events: &[Value]) -> Option<Hypothesis> {
    let mut groups: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    for event in events {
        let Some(heap) = number_field(event, "heap_used_pct") else { continue; };
        let Some(gc) = number_field(event, "gc_pause_ms") else { continue; };
        let (Some(service), Some(instance), Some(ts)) = (service(event), instance(event), timestamp(event)) else {
            continue;
        };
        if !(0.0..=100.0).contains(&heap) || gc < 0.0 { continue; }
        let _ = (heap, gc);
        groups.entry((service, instance)).or_default().push(event);
        let _ = ts;
    }

    for ((service_name, instance_name), mut samples) in groups {
        samples.sort_by_key(|event| timestamp(event).unwrap_or(i64::MIN));
        if samples.len() < 3 { continue; }
        let mut heap_slopes = Vec::new();
        let mut gc_slopes = Vec::new();
        for i in 0..samples.len() {
            for j in (i + 1)..samples.len() {
                let dt = timestamp(samples[j])?.saturating_sub(timestamp(samples[i])?);
                if dt <= 0 { continue; }
                let dt = dt as f64;
                heap_slopes.push((number_field(samples[j], "heap_used_pct")? - number_field(samples[i], "heap_used_pct")?) / dt);
                gc_slopes.push((number_field(samples[j], "gc_pause_ms")? - number_field(samples[i], "gc_pause_ms")?) / dt);
            }
        }
        let heap_slope = median(heap_slopes)?;
        let gc_slope = median(gc_slopes)?;
        let heap_delta = number_field(samples.last()?, "heap_used_pct")?
            - number_field(samples.first()?, "heap_used_pct")?;
        let gc_delta = number_field(samples.last()?, "gc_pause_ms")?
            - number_field(samples.first()?, "gc_pause_ms")?;
        let oom = events.iter().find(|event| {
            service(event).as_deref() == Some(service_name.as_str())
                && instance(event).as_deref() == Some(instance_name.as_str())
                && (string_field(event, "error_code").unwrap_or_default().to_ascii_uppercase().contains("OOM")
                    || string_field(event, "message").unwrap_or_default().to_ascii_lowercase().contains("outofmemoryerror"))
        });
        let Some(oom) = oom else { continue; };
        if heap_slope > 0.0 && gc_slope > 0.0 && heap_delta >= 15.0 && gc_delta >= 100.0 {
            let mut evidence: Vec<String> = samples.iter().map(|event| event_id(event)).collect();
            evidence.push(event_id(oom));
            evidence.sort();
            evidence.dedup();
            return Some(Hypothesis {
                specialist: "resource-theil-sen-trend",
                target_type: "instance",
                target: instance_name,
                evidence_ids: evidence,
                explanation: "Positive robust heap and GC-pause trends precede an explicit OOM endpoint.",
            });
        }
        let _ = service_name;
    }
    None
}

/// Run each specialist independently. No combined score or learned probability
/// is produced; a later evidence verifier must reconcile competing hypotheses.
pub fn analyze(events: &[Value]) -> Vec<Hypothesis> {
    [
        database_deadlock(events),
        auth_cascade(events),
        duplicate_delivery(events),
        resource_exhaustion(events),
    ].into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn database_deadlock_signature_selects_database_not_application_symptom() {
        let events = vec![
            json!({"event_id":"db","service":"postgres","offset_ms":30,"level":"ERROR","error_code":"40P01"}),
            json!({"event_id":"orders","service":"orders","offset_ms":34,"level":"ERROR","error_code":"DB_DEADLOCK"}),
            json!({"event_id":"billing","service":"billing","offset_ms":38,"level":"WARN","retry_count":1}),
        ];
        let h = database_deadlock(&events).unwrap();
        assert_eq!(h.target, "postgres");
        assert_eq!(h.evidence_ids, vec!["db"]);
    }

    #[test]
    fn auth_sequence_selects_identity_only_with_shared_flow_and_order() {
        let events = vec![
            json!({"event_id":"jwt","service":"identity","offset_ms":8,"trace_id":"t1","request_id":"r1","level":"ERROR","error_code":"JWT_AUDIENCE_MISMATCH"}),
            json!({"event_id":"symptom","service":"orders","offset_ms":14,"trace_id":"t1","request_id":"r1","level":"WARN","outcome":"failure","error_code":"AUTH_CONTEXT_MISSING"}),
        ];
        assert_eq!(auth_cascade(&events).unwrap().target, "identity");
        let unrelated = vec![
            json!({"event_id":"jwt","service":"identity","offset_ms":8,"trace_id":"t1","request_id":"r1","level":"ERROR","error_code":"JWT_AUDIENCE_MISMATCH"}),
            json!({"event_id":"symptom","service":"orders","offset_ms":14,"trace_id":"t2","request_id":"r2","level":"WARN","outcome":"failure","error_code":"AUTH_CONTEXT_MISSING"}),
        ];
        assert!(auth_cascade(&unrelated).is_none());

        let conflicting_request = vec![
            json!({"event_id":"jwt","service":"identity","offset_ms":8,"trace_id":"t1","request_id":"r1","tenant":"tenant-a","level":"ERROR","error_code":"JWT_AUDIENCE_MISMATCH"}),
            json!({"event_id":"symptom","service":"orders","offset_ms":14,"trace_id":"t1","request_id":"r2","tenant":"tenant-a","level":"WARN","outcome":"failure","error_code":"AUTH_CONTEXT_MISSING"}),
        ];
        assert!(auth_cascade(&conflicting_request).is_none());

        let cross_tenant = vec![
            json!({"event_id":"jwt","service":"identity","offset_ms":8,"trace_id":"t1","request_id":"r1","tenant":"tenant-a","level":"ERROR","error_code":"JWT_AUDIENCE_MISMATCH"}),
            json!({"event_id":"symptom","service":"orders","offset_ms":14,"trace_id":"t1","request_id":"r1","tenant":"tenant-b","level":"WARN","outcome":"failure","error_code":"AUTH_CONTEXT_MISSING"}),
        ];
        assert!(auth_cascade(&cross_tenant).is_none());
    }

    #[test]
    fn duplicate_delivery_finds_consumer_component_from_message_and_side_effects() {
        let events = vec![
            json!({"event_id":"d1","service":"stock-consumer","message_id":"m1","delivery_attempt":1,"message":"Message received"}),
            json!({"event_id":"s1","service":"stock","message_id":"m1","reservation_id":"res1","idempotency_key":null}),
            json!({"event_id":"d2","service":"stock-consumer","message_id":"m1","delivery_attempt":2,"message":"Message received after redelivery"}),
            json!({"event_id":"s2","service":"stock","message_id":"m1","reservation_id":"res2","idempotency_key":null,"outcome":"failure"}),
        ];
        let h = duplicate_delivery(&events).unwrap();
        assert_eq!(h.target_type, "component");
        assert_eq!(h.target, "stock-consumer");
        assert!(h.evidence_ids.contains(&"d1".to_owned()));
        assert!(h.evidence_ids.contains(&"s2".to_owned()));
    }

    #[test]
    fn idempotent_or_single_side_effect_delivery_is_not_flagged() {
        let events = vec![
            json!({"event_id":"d1","service":"stock-consumer","message_id":"m1","delivery_attempt":1}),
            json!({"event_id":"d2","service":"stock-consumer","message_id":"m1","delivery_attempt":2}),
            json!({"event_id":"s1","service":"stock","message_id":"m1","reservation_id":"res1","idempotency_key":"idem-1"}),
            json!({"event_id":"s2","service":"stock","message_id":"m1","reservation_id":"res1","idempotency_key":"idem-1"}),
        ];
        assert!(duplicate_delivery(&events).is_none());
    }

    #[test]
    fn resource_trend_requires_multiple_rising_samples_and_oom_endpoint() {
        let events = vec![
            json!({"event_id":"r1","service":"catalog","instance":"catalog-03","offset_ms":0,"attrs":{"heap_used_pct":62,"gc_pause_ms":4}}),
            json!({"event_id":"r2","service":"catalog","instance":"catalog-03","offset_ms":10000,"attrs":{"heap_used_pct":74,"gc_pause_ms":15}}),
            json!({"event_id":"r3","service":"catalog","instance":"catalog-03","offset_ms":20000,"attrs":{"heap_used_pct":88,"gc_pause_ms":120}}),
            json!({"event_id":"r4","service":"catalog","instance":"catalog-03","offset_ms":30000,"attrs":{"heap_used_pct":96,"gc_pause_ms":900}}),
            json!({"event_id":"r5","service":"catalog","instance":"catalog-03","offset_ms":35000,"attrs":{"heap_used_pct":99,"gc_pause_ms":2400}}),
            json!({"event_id":"oom","service":"catalog","instance":"catalog-03","offset_ms":36000,"level":"FATAL","error_code":"JAVA_HEAP_OOM"}),
        ];
        let h = resource_exhaustion(&events).unwrap();
        assert_eq!(h.target_type, "instance");
        assert_eq!(h.target, "catalog-03");
        assert!(h.evidence_ids.contains(&"oom".to_owned()));
        let without_oom = events[..5].to_vec();
        assert!(resource_exhaustion(&without_oom).is_none());
    }

    #[test]
    fn oom_from_another_service_cannot_close_resource_hypothesis() {
        let events = vec![
            json!({"event_id":"r1","service":"catalog","instance":"shared-03","offset_ms":0,"attrs":{"heap_used_pct":62,"gc_pause_ms":4}}),
            json!({"event_id":"r2","service":"catalog","instance":"shared-03","offset_ms":10000,"attrs":{"heap_used_pct":74,"gc_pause_ms":15}}),
            json!({"event_id":"r3","service":"catalog","instance":"shared-03","offset_ms":20000,"attrs":{"heap_used_pct":88,"gc_pause_ms":120}}),
            json!({"event_id":"oom","service":"billing","instance":"shared-03","offset_ms":21000,"level":"FATAL","error_code":"JAVA_HEAP_OOM"}),
        ];
        assert!(resource_exhaustion(&events).is_none());
    }

    #[test]
    fn resource_detector_does_not_flag_flat_healthy_series() {
        let events = vec![
            json!({"event_id":"r1","service":"catalog","instance":"catalog-03","offset_ms":0,"attrs":{"heap_used_pct":61,"gc_pause_ms":9}}),
            json!({"event_id":"r2","service":"catalog","instance":"catalog-03","offset_ms":10000,"attrs":{"heap_used_pct":62,"gc_pause_ms":10}}),
            json!({"event_id":"r3","service":"catalog","instance":"catalog-03","offset_ms":20000,"attrs":{"heap_used_pct":61,"gc_pause_ms":8}}),
        ];
        assert!(resource_exhaustion(&events).is_none());
    }
}
