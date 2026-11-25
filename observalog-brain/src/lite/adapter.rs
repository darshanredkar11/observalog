use super::model::CanonicalEvent;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

/// Adapt one raw log into the internal event model. No identifier, outcome or
/// duration is invented. The untouched source record is stored separately.
pub fn adapt(raw: &str, source: &Value, observed_at: DateTime<Utc>) -> CanonicalEvent {
    let parsed = serde_json::from_str::<Value>(raw).ok();
    let text_record = parsed.is_none().then(|| parse_text_record(raw));
    let mut provenance = Map::new();
    let mut quality_flags: Vec<String> = Vec::new();

    let source_name = string_field(source, &["source_name", "source", "tag"]);
    let root = parsed.as_ref().or(text_record.as_ref()).unwrap_or(source);
    let format = if parsed.is_some() { "json" } else { "text" };

    let timestamp = find(root, &["timestamp", "@timestamp", "time", "ts", "datetime", "date", "observed_timestamp"])
        .and_then(|(v, path)| {
            provenance.insert("event_ts".into(), json!({"source_path":path,"method":"alias"}));
            parse_timestamp(v)
        });
    let event_ts_inferred = timestamp.is_none();
    let event_ts = timestamp.unwrap_or(observed_at);
    if event_ts_inferred {
        provenance.insert("event_ts".into(), json!({"method":"collector_observed_at_fallback"}));
        quality_flags.push("source_timestamp_missing_or_invalid".into());
    }

    let service = text_field(root, &["service", "service.name", "serviceName", "app_name", "app", "logger", "application"]);
    record_source_path(root, &["service", "service.name", "serviceName", "app_name", "app", "logger", "application"], "service", &mut provenance);
    let instance = text_field(root, &["instance", "host", "host.name", "hostname", "pod", "container_name", "node"]);
    record_source_path(root, &["instance", "host", "host.name", "hostname", "pod", "container_name", "node"], "instance", &mut provenance);
    let version = text_field(root, &["service.version", "serviceVersion", "version", "build", "release"]);
    record_source_path(root, &["service.version", "serviceVersion", "version", "build", "release"], "version", &mut provenance);
    let environment = text_field(root, &["environment", "env", "deployment.environment", "deploymentEnvironment"]);
    let region = text_field(root, &["region", "cloud.region", "availability_zone", "zone"]);
    let tenant_id = text_field(root, &["tenant_id", "tenantId", "tenant.id", "tenant"]);

    let severity = find(root, &["severity_text", "severity", "level", "log.level", "levelname"])
        .and_then(|(v, path)| {
            provenance.insert("severity".into(), json!({"source_path":path,"method":"normalized"}));
            normalize_severity(v)
        });
    if severity.is_none() {
        quality_flags.push("severity_unknown".into());
    }

    let trace_id = text_field(root, &["trace_id", "traceId", "trace.id", "trace", "x_trace_id"]);
    record_source_path(root, &["trace_id", "traceId", "trace.id", "trace", "x_trace_id"], "trace_id", &mut provenance);
    let span_id = text_field(root, &["span_id", "spanId", "span.id", "span"]);
    record_source_path(root, &["span_id", "spanId", "span.id", "span"], "span_id", &mut provenance);
    let parent_span_id = text_field(root, &["parent_span_id", "parentSpanId", "parent_span", "parentSpan"]);
    record_source_path(root, &["parent_span_id", "parentSpanId", "parent_span", "parentSpan"], "parent_span_id", &mut provenance);
    let request_id = text_field(root, &["request_id", "requestId", "correlation_id", "correlationId", "correlationID", "x_request_id"]);
    record_source_path(root, &["request_id", "requestId", "correlation_id", "correlationId", "correlationID", "x_request_id"], "request_id", &mut provenance);
    let business_entity_id = text_field(root, &[
        "business_entity_id", "entity_id", "order_id", "orderId", "payment_id",
        "document_id", "doc_id", "workflow_id", "transaction_id", "export_id"
    ]);
    record_source_path(root, &["business_entity_id", "entity_id", "order_id", "orderId", "payment_id", "document_id", "doc_id", "workflow_id", "transaction_id", "export_id"], "business_entity_id", &mut provenance);
    let source_event_id = text_field(root, &["event_id", "eventId", "log_id", "record_id", "id"]);
    record_source_path(root, &["event_id", "eventId", "log_id", "record_id", "id"], "source_event_id", &mut provenance);
    let event_name = text_field(root, &["event_name", "eventName", "event.name", "event", "operation"]);
    record_source_path(root, &["event_name", "eventName", "event.name", "event", "operation"], "event_name", &mut provenance);
    let message = text_field(root, &["message", "msg", "body", "log", "log_message"])
        .or_else(|| {
            let t = raw.trim();
            (!t.is_empty()).then(|| t.to_string())
        });
    record_source_path(root, &["message", "msg", "body", "log", "log_message"], "message", &mut provenance);
    let outcome = text_field(root, &["outcome", "result", "operation_status"])
        .map(|s| s.to_ascii_lowercase());
    record_source_path(root, &["outcome", "result", "operation_status"], "outcome", &mut provenance);
    let duration_ms = find(root, &["duration_ms", "durationMs", "elapsed_ms", "latency_ms", "duration"])
        .and_then(|(v, path)| {
            provenance.insert("duration_ms".into(), json!({"source_path":path,"method":"numeric"}));
            parse_number(v)
        });

    let error_object = find(root, &["error", "err", "exception"]).map(|(v, _)| v);
    let error_kind = error_object.and_then(|v| text_field(v, &["kind", "type", "name", "class"]))
        .or_else(|| text_field(root, &["error_kind", "errorKind", "exception_type", "exceptionType"]));
    let error_code = error_object.and_then(|v| text_field(v, &["code", "error_code", "errorCode"]))
        .or_else(|| text_field(root, &["error_code", "errorCode", "err_code", "exception_code", "exceptionCode"]));
    let error_message = error_object.and_then(|v| text_field(v, &["message", "msg", "reason"]))
        .or_else(|| text_field(root, &["error_message", "errorMessage"]));

    if trace_id.is_none() { quality_flags.push("trace_id_missing".into()); }
    if service.is_none() { quality_flags.push("service_identity_missing".into()); }
    if message.is_none() { quality_flags.push("message_missing".into()); }
    if raw.len() > 256 * 1024 { quality_flags.push("large_record".into()); }

    let mut attributes = match root {
        Value::Object(map) => Value::Object(map.clone()),
        other => json!({"record":other}),
    };
    if !attributes.is_object() { attributes = json!({"record":attributes}); }

    CanonicalEvent {
        source_name,
        source_event_id,
        event_ts,
        observed_at,
        event_ts_inferred,
        service,
        instance,
        version,
        environment,
        region,
        tenant_id,
        severity,
        trace_id,
        span_id,
        parent_span_id,
        request_id,
        business_entity_id,
        event_name,
        message,
        outcome,
        duration_ms,
        error_kind,
        error_code,
        error_message,
        attributes,
        provenance: Value::Object(provenance),
        quality: json!({"format":format,"flags":quality_flags,"mapping_confidence":0.0}),
    }
}


fn parse_text_record(raw: &str) -> Value {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let mut fields = Map::new();
    if let Some(first) = tokens.first() {
        if DateTime::parse_from_rfc3339(first).is_ok() {
            fields.insert("timestamp".into(), json!(*first));
        } else if tokens.len() > 1 {
            let combined = format!("{}T{}Z", first, tokens[1]);
            if DateTime::parse_from_rfc3339(&combined).is_ok() {
                fields.insert("timestamp".into(), json!(combined));
            }
        }
    }

    for token in &tokens {
        let candidate = token.trim_matches(|ch: char| matches!(ch, '[' | ']' | ':' | ',' ));
        let value = Value::String(candidate.to_string());
        if let Some(level) = normalize_severity(&value) {
            fields.insert("severity".into(), json!(level));
            break;
        }
    }

    if let Some(start) = raw.find('[') {
        if let Some(end) = raw[start + 1..].find(']') {
            let scope = &raw[start + 1..start + 1 + end];
            let service = scope.split(',').next().unwrap_or("").trim();
            if !service.is_empty() && !service.chars().all(|c| c.is_ascii_digit()) {
                fields.insert("service".into(), json!(service));
            }
        }
    }
    if !fields.contains_key("service") && tokens.len() >= 3 {
        let candidate = tokens.get(2).copied().unwrap_or_default().trim_matches(|ch: char| ch == '[' || ch == ']');
        if !candidate.is_empty() && !candidate.contains('=') && !candidate.starts_with('[') {
            fields.insert("service".into(), json!(candidate));
        }
    }

    let message_start = tokens.iter().position(|token| {
        let candidate = token.trim_matches(|ch: char| matches!(ch, '[' | ']' | ':' | ','));
        normalize_severity(&Value::String(candidate.to_string())).is_some()
    });
    let message = message_start
        .map(|index| tokens[index + 1..].join(" "))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| raw.to_string());
    fields.insert("message".into(), json!(message));

    for token in &tokens {
        if let Some((key, value)) = token.split_once('=') {
            let key = key.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.');
            let value = value.trim_matches(|ch: char| matches!(ch, '"' | '\'' | ',' | ']'));
            if !key.is_empty() && !value.is_empty() {
                fields.insert(key.into(), json!(value));
            }
        }
    }
    Value::Object(fields)
}

fn find<'a>(value: &'a Value, aliases: &[&str]) -> Option<(&'a Value, String)> {
    for alias in aliases {
        if let Some(found) = find_key(value, alias, "$") {
            return Some(found);
        }
    }
    None
}

fn find_key<'a>(value: &'a Value, key: &str, path: &str) -> Option<(&'a Value, String)> {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(key) {
                return Some((v, format!("{path}.{key}")));
            }
            for (child_key, child) in map {
                if let Some(found) = find_key(child, key, &format!("{path}.{child_key}")) {
                    return Some(found);
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                if let Some(found) = find_key(child, key, &format!("{path}[{index}]")) {
                    return Some(found);
                }
            }
        }
        _ => {}
    }
    None
}

fn text_field(value: &Value, aliases: &[&str]) -> Option<String> {
    find(value, aliases).and_then(|(v, _)| match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn string_field(value: &Value, aliases: &[&str]) -> Option<String> {
    text_field(value, aliases)
}

fn record_source_path(root: &Value, aliases: &[&str], field: &str, provenance: &mut Map<String, Value>) {
    if let Some((_, path)) = find(root, aliases) {
        provenance.insert(field.into(), json!({"source_path":path,"method":"alias"}));
    }
}

fn parse_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn parse_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(s) => DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc)),
        Value::Number(n) => {
            let number = n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?;
            if number > 100_000_000_000 {
                DateTime::from_timestamp_millis(number)
            } else {
                DateTime::from_timestamp(number, 0)
            }
        }
        _ => None,
    }
}

fn normalize_severity(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => {
            let n = n.as_i64()?;
            Some(match n {
                0..=10 => "DEBUG",
                11..=20 => "INFO",
                21..=30 => "WARN",
                31..=40 => "ERROR",
                _ => "FATAL",
            }.into())
        }
        Value::String(s) => {
            let upper = s.trim().to_ascii_uppercase();
            Some(match upper.as_str() {
                "TRACE" | "VERBOSE" => "TRACE",
                "DEBUG" | "DBG" => "DEBUG",
                "INFO" | "INFORMATION" => "INFO",
                "WARN" | "WARNING" => "WARN",
                "ERROR" | "ERR" => "ERROR",
                "FATAL" | "CRITICAL" | "PANIC" => "FATAL",
                _ => return None,
            }.into())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now() -> DateTime<Utc> { DateTime::parse_from_rfc3339("2026-11-25T12:00:00Z").unwrap().with_timezone(&Utc) }

    #[test]
    fn adapts_pino_aliases_and_pino_numeric_level() {
        let event = adapt(r#"{"time":1795608000000,"level":40,"msg":"database timeout","service.name":"orders","trace_id":"t-1","err_code":"DB_TIMEOUT"}"#, &json!({"tag":"app"}), now());
        assert_eq!(event.service.as_deref(), Some("orders"));
        assert_eq!(event.severity.as_deref(), Some("ERROR"));
        assert_eq!(event.trace_id.as_deref(), Some("t-1"));
        assert_eq!(event.error_code.as_deref(), Some("DB_TIMEOUT"));
        assert!(!event.event_ts_inferred);
    }

    #[test]
    fn adapts_nested_otel_fields_without_fabricating_trace() {
        let event = adapt(r#"{"timestamp":"2026-11-25T12:00:00Z","severity_text":"WARN","body":"slow query","resource":{"service":{"name":"billing"},"attributes":{"host.name":"billing-2"}}}"#, &json!({}), now());
        assert_eq!(event.service.as_deref(), Some("billing"));
        assert_eq!(event.instance.as_deref(), Some("billing-2"));
        assert_eq!(event.trace_id, None);
        assert_eq!(event.severity.as_deref(), Some("WARN"));
    }

    #[test]
    fn preserves_missing_timestamp_as_observed_fallback() {
        let observed = now();
        let event = adapt("2026-11-25 ERROR payment timeout", &json!({"source_name":"legacy"}), observed);
        assert!(event.event_ts_inferred);
        assert_eq!(event.event_ts, observed);
        assert_eq!(event.source_name.as_deref(), Some("legacy"));
    }

    #[test]
    fn keeps_explicit_outcome_unknown_instead_of_defaulting_success() {
        let event = adapt(r#"{"message":"payment complete","status":200}"#, &json!({}), now());
        assert_eq!(event.outcome, None);
    }

    #[test]
    fn reads_string_duration_and_error_object() {
        let event = adapt(r#"{"message":"gateway failed","duration":"125.5","error":{"type":"Timeout","code":"UPSTREAM_TIMEOUT","message":"deadline exceeded"}}"#, &json!({}), now());
        assert_eq!(event.duration_ms, Some(125.5));
        assert_eq!(event.error_kind.as_deref(), Some("Timeout"));
        assert_eq!(event.error_code.as_deref(), Some("UPSTREAM_TIMEOUT"));
    }

    #[test]
    fn parses_spring_text_and_key_value_context() {
        let event = adapt("2026-11-25T12:00:00Z ERROR [payments,payment-2] GatewayHandler : timeout trace_id=trace-1 order_id=order-2 error_code=GATEWAY_TIMEOUT", &json!({}), now());
        assert_eq!(event.service.as_deref(), Some("payments"));
        assert_eq!(event.severity.as_deref(), Some("ERROR"));
        assert_eq!(event.trace_id.as_deref(), Some("trace-1"));
        assert_eq!(event.business_entity_id.as_deref(), Some("order-2"));
    }
}
