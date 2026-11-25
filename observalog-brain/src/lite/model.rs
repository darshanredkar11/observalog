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
