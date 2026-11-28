use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub source_name: Option<String>,
    pub source_event_id: Option<String>,
    pub event_ts: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub event_ts_inferred: bool,
    pub service: Option<String>,
    pub instance: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub region: Option<String>,
    pub tenant_id: Option<String>,
    pub severity: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub request_id: Option<String>,
    pub business_entity_id: Option<String>,
    pub message_id: Option<String>,
    pub dependency_name: Option<String>,
    pub event_name: Option<String>,
    pub message: Option<String>,
    pub outcome: Option<String>,
    pub duration_ms: Option<f64>,
    pub error_kind: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub attributes: Value,
    pub provenance: Value,
    pub quality: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestResponse {
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
}

/// Replaces NUL bytes (`\0`) in text so PostgreSQL TEXT/JSONB inserts never fail.
pub fn sanitize_text_nulls(input: &str) -> String {
    if input.contains('\0') {
        input.replace('\0', "\u{FFFD}")
    } else {
        input.to_string()
    }
}

/// Recursively replaces NUL characters (`\0`) inside JSON strings and object keys
/// because PostgreSQL JSONB rejects `\u0000`. Returns `true` if any NUL was found.
pub fn sanitize_json_nulls(value: &mut Value) -> bool {
    match value {
        Value::String(text) => {
            if text.contains('\0') {
                *text = text.replace('\0', "\u{FFFD}");
                true
            } else {
                false
            }
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= sanitize_json_nulls(item);
            }
            changed
        }
        Value::Object(map) => {
            let mut changed = false;
            let keys_with_null: Vec<String> = map
                .keys()
                .filter(|k| k.contains('\0'))
                .cloned()
                .collect();
            for old_key in keys_with_null {
                if let Some(val) = map.remove(&old_key) {
                    map.insert(old_key.replace('\0', "\u{FFFD}"), val);
                    changed = true;
                }
            }
            for val in map.values_mut() {
                changed |= sanitize_json_nulls(val);
            }
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitize_json_nulls_removes_nul_from_keys_and_values() {
        let mut payload = json!({
            "msg": "\u{0000}binary\u{0001}",
            "nested": [{"k\u{0000}ey": "a\u{0000}b"}]
        });
        assert!(sanitize_json_nulls(&mut payload));
        assert!(!payload.to_string().contains("\\u0000"));
        assert!(!sanitize_json_nulls(&mut payload));
    }
}
