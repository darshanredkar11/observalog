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
    if parsed.is_none() && (raw.trim_start().starts_with('{') || raw.trim_start().starts_with('[')) {
        quality_flags.push("json_parse_failed_preserved_as_text".into());
    }

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
    record_source_path(root, &["environment", "env", "deployment.environment", "deploymentEnvironment"], "environment", &mut provenance);
    let region = text_field(root, &["region", "cloud.region", "availability_zone", "zone"]);
    record_source_path(root, &["region", "cloud.region", "availability_zone", "zone"], "region", &mut provenance);
    let tenant_id = text_field(root, &["tenant_id", "tenantId", "tenant.id", "tenant"]);
    record_source_path(root, &["tenant_id", "tenantId", "tenant.id", "tenant"], "tenant_id", &mut provenance);

    let severity = find(root, &["severity_text", "severity", "severity_number", "level", "log.level", "levelname", "syslog_severity"])
        .and_then(|(v, path)| {
            provenance.insert("severity".into(), json!({"source_path":path,"method":"source-specific-normalization"}));
            normalize_severity_at(v, &path)
        });
    if severity.is_none() {
        quality_flags.push("severity_unknown".into());
    }

    let trace_id = text_field(root, &["trace_id", "traceId", "trace.id", "trace", "x_trace_id"]);
    record_source_path(root, &["trace_id", "traceId", "trace.id", "trace", "x_trace_id"], "trace_id", &mut provenance);
    if trace_id.as_deref().is_some_and(|value| value.trim().len() < 8) {
        quality_flags.push("trace_id_short_unverified".into());
    }
    let span_id = text_field(root, &["span_id", "spanId", "span.id", "span"]);
    record_source_path(root, &["span_id", "spanId", "span.id", "span"], "span_id", &mut provenance);
    let parent_span_id = text_field(root, &["parent_span_id", "parentSpanId", "parent_span", "parentSpan"]);
    record_source_path(root, &["parent_span_id", "parentSpanId", "parent_span", "parentSpan"], "parent_span_id", &mut provenance);
    let request_id = text_field(root, &["request_id", "requestId", "request.id", "correlation_id", "correlationId", "correlationID", "x_request_id"]);
    record_source_path(root, &["request_id", "requestId", "request.id", "correlation_id", "correlationId", "correlationID", "x_request_id"], "request_id", &mut provenance);
    let business_entity_id = text_field(root, &[
        "business_entity_id", "entity_id", "order_id", "orderId", "order.id", "payment_id", "payment.id",
        "document_id", "document.id", "doc_id", "workflow_id", "workflow.id", "transaction_id", "transaction.id", "export_id", "export.id"
    ]);
    record_source_path(root, &["business_entity_id", "entity_id", "order_id", "orderId", "order.id", "payment_id", "payment.id", "document_id", "document.id", "doc_id", "workflow_id", "workflow.id", "transaction_id", "transaction.id", "export_id", "export.id"], "business_entity_id", &mut provenance);
    let message_id = text_field(root, &["message_id", "messageId", "messaging.message.id", "kafka_message_id", "kafkaMessageId", "delivery_id"]);
    record_source_path(root, &["message_id", "messageId", "messaging.message.id", "kafka_message_id", "kafkaMessageId", "delivery_id"], "message_id", &mut provenance);
    let dependency_name = text_field(root, &["dependency_name", "dependency_host", "upstream_host", "provider", "server.address", "net.peer.name", "db.system"]);
    record_source_path(root, &["dependency_name", "dependency_host", "upstream_host", "provider", "server.address", "net.peer.name", "db.system"], "dependency_name", &mut provenance);
    let source_event_id = text_field(root, &["event_id", "eventId", "event.id", "log_id", "logId", "log.id", "record_id", "recordId"]);
    record_source_path(root, &["event_id", "eventId", "event.id", "log_id", "logId", "log.id", "record_id", "recordId"], "source_event_id", &mut provenance);
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
    let duration_raw = find(root, &["duration_ms", "durationMs", "elapsed_ms", "latency_ms", "duration"]);
    let duration_ms = duration_raw.as_ref().and_then(|(v, path)| {
        provenance.insert("duration_ms".into(), json!({"source_path":path,"method":"numeric"}));
        parse_number(v)
    });
    if duration_raw.is_some() && duration_ms.is_none() {
        quality_flags.push("duration_invalid".into());
    }

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

    for (field, aliases) in [
        ("service", &["service", "service.name", "serviceName", "app_name", "app", "logger", "application"][..]),
        ("trace_id", &["trace_id", "traceId", "trace.id", "trace", "x_trace_id"][..]),
        ("request_id", &["request_id", "requestId", "correlation_id", "correlationId", "correlationID", "x_request_id"][..]),
        ("business_entity_id", &["business_entity_id", "entity_id", "order_id", "orderId", "payment_id", "document_id", "doc_id", "workflow_id", "transaction_id", "export_id"][..]),
        ("severity", &["severity_text", "severity", "level", "log.level", "levelname"][..]),
        ("duration", &["duration_ms", "durationMs", "elapsed_ms", "latency_ms", "duration"][..]),
    ] {
        if aliases_conflict(root, aliases) {
            quality_flags.push(format!("conflicting_{field}_aliases"));
        }
    }

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
        message_id,
        dependency_name,
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


fn aliases_conflict(root: &Value, aliases: &[&str]) -> bool {
    let mut values = Vec::new();
    for alias in aliases {
        if let Some((value, _)) = find(root, &[*alias]) {
            if !value.is_null() && (value.is_string() || value.is_number() || value.is_boolean()) {
                values.push(value.to_string());
            }
        }
    }
    values.sort();
    values.dedup();
    values.len() > 1
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
    if !fields.contains_key("service") {
        let severity_index = tokens.iter().position(|token| {
            let candidate = token.trim_matches(|ch: char| matches!(ch, '[' | ']' | ':' | ','));
            normalize_severity(&Value::String(candidate.to_string())).is_some()
        });
        if let Some(index) = severity_index {
            if let Some(candidate) = tokens.get(index + 1) {
                let candidate = candidate.trim_matches(|ch: char| ch == '[' || ch == ']');
                if !candidate.is_empty() && !candidate.contains('=') && !candidate.starts_with('[') {
                    fields.insert("service".into(), json!(candidate));
                }
            }
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

    // Do not promote incidental key=value text in ordinary prose into fields.
    // Only parse KV tokens when the record has recognizable log structure
    // (timestamp, early severity, bracketed scope) or is mostly KV tokens.
    let kv_count = tokens.iter().filter(|token| token.contains('=')).count();
    let severity_at_start = message_start.is_some_and(|index| index <= 3);
    let structured_kv = fields.contains_key("timestamp")
        || severity_at_start
        || fields.contains_key("service")
        || (!tokens.is_empty() && kv_count >= 2 && kv_count * 2 >= tokens.len());
    if structured_kv {
        for token in &tokens {
            if let Some((key, value)) = token.split_once('=') {
                let key = key.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.');
                let value = value.trim_matches(|ch: char| matches!(ch, '"' | '\'' | ',' | ']'));
                if !key.is_empty() && !value.is_empty() {
                    fields.insert(key.into(), json!(value));
                }
            }
        }
    }
    Value::Object(fields)
}

fn find<'a>(value: &'a Value, aliases: &[&str]) -> Option<(&'a Value, String)> {
    // Prefer root fields across all aliases before recursive fallback so
    // nested noise (e.g. error.message) cannot override the record's message.
    if let Value::Object(map) = value {
        for alias in aliases {
            if let Some(v) = map.get(*alias) {
                return Some((v, format!("$.{alias}")));
            }
        }
    }
    // Resolve explicit dotted paths before recursive envelope fallback.
    for alias in aliases.iter().filter(|alias| alias.contains('.')) {
        if let Some(found) = find_key(value, alias, "$") {
            return Some(found);
        }
    }
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
            if key.contains('.') {
                let mut cursor = value;
                let mut source_path = path.to_string();
                let mut matched = true;
                for part in key.split('.') {
                    if let Some(next) = cursor.get(part) {
                        cursor = next;
                        source_path.push('.');
                        source_path.push_str(part);
                    } else {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    return Some((cursor, source_path));
                }
            }
            // Recurse only through known telemetry envelope containers. Arbitrary
            // nested objects (especially error/exception payloads) are not fields
            // of the log record, and arrays are not treated as event envelopes.
            for child_key in ["resource", "attributes", "context", "metadata", "fields", "log", "data", "payload", "body"] {
                if let Some(child) = map.get(child_key) {
                    if let Some(found) = find_key(child, key, &format!("{path}.{child_key}")) {
                        return Some(found);
                    }
                }
            }
        }
        _ => {}
    }
    None
}

fn text_field(value: &Value, aliases: &[&str]) -> Option<String> {
    for alias in aliases {
        let Some((found, _)) = find(value, &[*alias]) else {
            continue;
        };
        let mapped = match found {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        };
        if mapped.is_some() {
            return mapped;
        }
    }
    None
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
    let parsed = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }?;
    // Invalid measurements remain in raw source but are not canonicalized.
    (parsed.is_finite() && parsed >= 0.0 && parsed <= 1.0e15).then_some(parsed)
}

fn parse_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(s) => DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc)),
        Value::Number(n) => {
            let number = n.as_i64().or_else(|| {
                let float = n.as_f64()?;
                if !float.is_finite() || float < i64::MIN as f64 || float > i64::MAX as f64 {
                    return None;
                }
                Some(float.trunc() as i64)
            })?;
            if number > 100_000_000_000 {
                DateTime::from_timestamp_millis(number)
            } else {
                DateTime::from_timestamp(number, 0)
            }
        }
        _ => None,
    }
}

fn normalize_severity_at(value: &Value, source_path: &str) -> Option<String> {
    if let Value::Number(number) = value {
        let number = number.as_i64()?;
        let path = source_path.to_ascii_lowercase();
        if path.ends_with("severity_number") {
            return Some(match number {
                1..=4 => "TRACE",
                5..=8 => "DEBUG",
                9..=12 => "INFO",
                13..=16 => "WARN",
                17..=20 => "ERROR",
                21..=24 => "FATAL",
                _ => return None,
            }.into());
        }
        if path.ends_with(".level") || path == "$.level" || path.ends_with(".log.level") {
            return Some(match number {
                10 => "TRACE",
                20 => "DEBUG",
                30 => "INFO",
                40 => "WARN",
                50 => "ERROR",
                60 => "FATAL",
                _ => return None,
            }.into());
        }
        if path.ends_with("syslog_severity") {
            return Some(match number {
                0..=2 => "FATAL",
                3 => "ERROR",
                4 => "WARN",
                5..=6 => "INFO",
                7 => "DEBUG",
                _ => return None,
            }.into());
        }
        return None;
    }
    normalize_severity(value)
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
                41..=50 => "FATAL",
                _ => return None,
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
        assert_eq!(event.severity.as_deref(), Some("WARN"));
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
    fn maps_otel_dotted_event_request_and_business_ids() {
        let event = adapt(
            r#"{"timestamp":"2026-11-25T12:00:00Z","severity_text":"INFO","body":"order accepted","resource":{"service":{"name":"orders"}},"attributes":{"event.id":"evt-1","request.id":"req-1","order.id":"ord-1"}}"#,
            &json!({}),
            now(),
        );
        assert_eq!(event.source_event_id.as_deref(), Some("evt-1"));
        assert_eq!(event.request_id.as_deref(), Some("req-1"));
        assert_eq!(event.business_entity_id.as_deref(), Some("ord-1"));
    }

    #[test]
    fn maps_otel_numeric_severity_without_confusing_it_with_pino() {
        let event = adapt(r#"{"severity_number":18,"body":"upstream failed"}"#, &json!({}), now());
        assert_eq!(event.severity.as_deref(), Some("ERROR"));
        let pino = adapt(r#"{"level":50,"msg":"upstream failed"}"#, &json!({}), now());
        assert_eq!(pino.severity.as_deref(), Some("ERROR"));
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

    #[test]
    fn nested_otel_service_resource_is_mapped() {
        let event = adapt(r#"{"timestamp":"2026-11-25T12:00:00Z","severity_text":"WARN","body":"slow query","resource":{"service":{"name":"billing"},"attributes":{"host.name":"billing-2"}}}"#, &json!({}), now());
        assert_eq!(event.service.as_deref(), Some("billing"));
        assert_eq!(event.instance.as_deref(), Some("billing-2"));
    }

    #[test]
    fn conflicting_aliases_are_reported_not_silently_hidden() {
        let event = adapt(r#"{"timestamp":"2026-11-25T12:00:00Z","service":"orders-a","serviceName":"orders-b","message":"failed","level":"ERROR"}"#, &json!({}), now());
        assert_eq!(event.service.as_deref(), Some("orders-a"));
        assert!(event.quality["flags"].as_array().unwrap().iter().any(|flag| flag == "conflicting_service_aliases"));
    }

    #[test]
    fn malformed_json_is_preserved_as_text_and_flagged() {
        let event = adapt(r#"{"timestamp": "broken", "level":ERROR}"#, &json!({}), now());
        assert!(event.quality["flags"].as_array().unwrap().iter().any(|flag| flag == "json_parse_failed_preserved_as_text"));
        assert!(event.message.unwrap().contains("broken"));
    }
}


#[cfg(test)]
mod gate1_adversarial_tests {
    use super::*;
    use serde_json::json;
    use chrono::{DateTime, Utc};

    fn observed() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-25T12:00:00Z").unwrap().with_timezone(&Utc)
    }

    // Gate 1 contract: normalization is tolerant and deterministic, but it must
    // not manufacture semantics from invalid values. Raw text is preserved by
    // the caller; this gate only produces bounded, defensible canonical fields.
    #[test]
    fn null_empty_scalar_and_array_inputs_never_panic_or_invent_fields() {
        for raw in ["", "   ", "null", "true", "false", "0", "42", "\"text\"", "[]", "[1,2]", "{}"] {
            let event = adapt(raw, &json!({}), observed());
            assert_eq!(event.event_ts, observed(), "raw={raw:?}");
            assert!(event.event_ts_inferred, "raw={raw:?}");
            assert_eq!(event.service, None, "raw={raw:?}");
            assert_eq!(event.trace_id, None, "raw={raw:?}");
            assert_eq!(event.outcome, None, "raw={raw:?}");
            assert_eq!(event.duration_ms, None, "raw={raw:?}");
        }
    }

    #[test]
    fn missing_and_null_aliases_remain_missing() {
        for raw in [
            r#"{"service":null,"trace_id":null,"request_id":null,"duration_ms":null,"outcome":null}"#,
            r#"{"service":"","trace_id":"","request_id":"","duration_ms":"","outcome":""}"#,
            r#"{"service":[],"trace_id":{},"request_id":false,"duration_ms":[],"outcome":{}}"#,
        ] {
            let event=adapt(raw,&json!({}),observed());
            assert_eq!(event.service,None, "raw={raw}");
            assert_eq!(event.trace_id,None, "raw={raw}");
            assert_eq!(event.request_id,None, "raw={raw}");
            assert_eq!(event.duration_ms,None, "raw={raw}");
            assert_eq!(event.outcome,None, "raw={raw}");
        }
    }

    #[test]
    fn array_wrapped_records_are_not_promoted_to_single_event_fields() {
        let raw=r#"[{"message":"fake event","service":"fake-service","trace_id":"fake-trace-12345678","duration_ms":200}]"#;
        let event=adapt(raw,&json!({}),observed());
        assert_eq!(event.service,None);
        assert_eq!(event.trace_id,None);
        assert_eq!(event.duration_ms,None);
        assert_eq!(event.message.as_deref(),Some(raw));
    }

    #[test]
    fn nested_exception_message_is_not_promoted_to_log_message() {
        let raw=r#"{"error":{"type":"Timeout","code":"TIMEOUT","message":"exception detail only"}}"#;
        let event=adapt(raw,&json!({}),observed());
        assert_eq!(event.message.as_deref(),Some(raw));
        assert_eq!(event.error_message.as_deref(),Some("exception detail only"));
        assert_eq!(event.error_kind.as_deref(),Some("Timeout"));
        assert_eq!(event.error_code.as_deref(),Some("TIMEOUT"));
    }

    #[test]
    fn nested_noise_does_not_override_top_level_canonical_fields() {
        let raw=r#"{"message":"top message","service":"payments","trace_id":"trace-top-12345678","duration_ms":12,"error":{"message":"nested error message","service":"fake-service","trace_id":"trace-fake-12345678","duration_ms":999}}"#;
        let event=adapt(raw,&json!({}),observed());
        assert_eq!(event.message.as_deref(),Some("top message"));
        assert_eq!(event.service.as_deref(),Some("payments"));
        assert_eq!(event.trace_id.as_deref(),Some("trace-top-12345678"));
        assert_eq!(event.duration_ms,Some(12.0));
    }

    #[test]
    fn invalid_negative_nan_and_infinite_durations_are_not_canonicalized() {
        for value in ["-1", "\"-1\"", "\"NaN\"", "\"Infinity\"", "\"-Infinity\"", "\"1e9999\""] {
            let raw=format!(r#"{{"duration_ms":{value},"message":"duration probe"}}"#);
            let event=adapt(&raw,&json!({}),observed());
            assert_eq!(event.duration_ms,None, "value={value}");
        }
    }

    #[test]
    fn duration_alias_conflicts_are_flagged_and_not_arbitrarily_silenced() {
        let event=adapt(r#"{"duration_ms":100,"latency_ms":900,"message":"alias conflict"}"#,&json!({}),observed());
        assert!(event.quality["flags"].as_array().unwrap().iter().any(|f|f=="conflicting_duration_aliases"));
    }

    #[test]
    fn timestamp_extremes_and_invalid_formats_fall_back_to_observed_time() {
        for raw in [
            r#"{"timestamp":"2026-99-99T88:77:66Z"}"#,
            r#"{"timestamp":"not a timestamp"}"#,
            r#"{"timestamp":1e300}"#,
            r#"{"timestamp":-1e300}"#,
            r#"{"timestamp":"NaN"}"#,
        ] {
            let event=adapt(raw,&json!({}),observed());
            assert_eq!(event.event_ts,observed(), "raw={raw}");
            assert!(event.event_ts_inferred, "raw={raw}");
        }
    }

    #[test]
    fn unknown_severity_and_outcome_are_not_forced_into_known_buckets() {
        let event=adapt(r#"{"severity_text":"ALIEN","outcome":"maybe","message":"uncertain"}"#,&json!({}),observed());
        assert_eq!(event.severity,None);
        assert_eq!(event.outcome.as_deref(),Some("maybe"));
    }

    #[test]
    fn prose_mentions_of_fields_are_not_structural_fields() {
        let raw="The user said service=payments trace_id=short duration_ms=-99 but these are quoted words, not structured JSON.";
        let event=adapt(raw,&json!({}),observed());
        // Unstructured text may be retained as a message, but identifiers and
        // measurements should not be promoted from incidental prose.
        assert_eq!(event.trace_id,None);
        assert_eq!(event.duration_ms,None);
    }

    #[test]
    fn malformed_and_truncated_json_is_retained_as_text() {
        for raw in ["{", "{not-json}", r#"{"message":"unterminated}"#, r#"{"timestamp":NaN}"#, "[1,2,3"] {
            let event=adapt(raw,&json!({}),observed());
            assert_eq!(event.message.as_deref(), Some(raw.trim()), "raw={raw:?}");
            assert!(event.quality["flags"].as_array().unwrap().iter().any(|f|f=="json_parse_failed_preserved_as_text") || !raw.trim_start().starts_with('{'));
        }
    }

    #[test]
    fn long_and_multilingual_records_are_preserved_without_panics() {
        let raw=format!("ERROR service=payments message={} हिन्दी मराठी 日本語 العربية 🚨", "x".repeat(270_000));
        let event=adapt(&raw,&json!({"source_name":"adversarial"}),observed());
        assert!(event.quality["flags"].as_array().unwrap().iter().any(|f|f=="large_record"));
        assert!(event.message.as_deref().unwrap_or_default().contains("मराठी"));
        assert_eq!(event.source_name.as_deref(),Some("adversarial"));
    }
}
