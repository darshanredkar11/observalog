use super::parser::ParsedEntry;
use super::writer_contract::MAX_PAYLOAD_FETCH_ROWS;
use crate::db::queries;
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::PgPool;
use tracing::{debug, warn};

/// Write one parsed entry to TimescaleDB.
///
/// Order is strict per writer_contract.rs INVARIANT 1:
///   1. Build payload JSON
///   2. INSERT log_payload → get payload_id
///   3. INSERT log_index using payload_id
///
/// If a crash happens between steps 2 and 3, an orphaned payload row exists
/// but causes no data corruption — the brain simply won't find it via log_index.
pub async fn write_entry(pool: &PgPool, entry: &ParsedEntry) -> Result<(i64, i64)> {
    // Build the full payload document.
    let payload = build_payload(entry);

    // INVARIANT 1: write payload first.
    let payload_id = queries::insert_log_payload(pool, &payload).await?;

    // Then write the index row.
    let index_id = queries::insert_log_index(pool, entry, payload_id).await?;

    debug!(
        trace_id = %entry.trace_id,
        event = %entry.event,
        payload_id,
        index_id,
        "entry written"
    );

    Ok((payload_id, index_id))
}

/// Build the JSONB payload stored in log_payload.
/// Expands abbreviated Part B keys back to their full names for readability
/// in the cold table.
fn build_payload(entry: &ParsedEntry) -> Value {
    let mut doc = json!({
        // Ambient fields (from Part A)
        "trace_id":      entry.trace_id,
        "span_id":       entry.span_id,
        "seq":           entry.seq,
        "service":       entry.service,
        "level":         entry.level,
        "ts":            entry.ts.to_rfc3339(),

        // Part B structural
        "event":         entry.event,
        "message":       entry.message,
    });

    if let Some(ref ps) = entry.parent_span {
        doc["parent_span"] = json!(ps);
    }
    if let Some(ms) = entry.duration_ms {
        doc["duration_ms"] = json!(ms);
    }
    if let Some(o) = entry.outcome {
        doc["outcome"] = json!(o);
    }
    if let Some(ref uid) = entry.user_id {
        doc["user_id"] = json!(uid);
    }
    if let Some(ref js) = entry.journey_stage {
        doc["journey_stage"] = json!(js);
    }
    if let Some(ref e) = entry.error {
        doc["error"] = json!({
            "kind":      e.kind,
            "code":      e.code,
            "message":   e.message,
            "retryable": e.retryable,
        });
    }
    if let Some(ref ctx) = entry.ctx {
        doc["ctx"] = ctx.clone();
    }
    if let Some(fp) = entry.fingerprint {
        doc["fingerprint"] = json!(fp);
    }

    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::parser::ParsedError;
    use chrono::Utc;

    fn make_entry() -> ParsedEntry {
        ParsedEntry {
            schema_version: 1,
            trace_id: "trc_test000001".to_string(),
            span_id: "spn_001".to_string(),
            parent_span: None,
            seq: 1,
            service: 1,
            level: 3,
            outcome: Some(2),
            ts: Utc::now(),
            event: "auth.jwt.failed".to_string(),
            message: "Token validation failed".to_string(),
            duration_ms: Some(42),
            user_id: Some("user123".to_string()),
            journey_stage: Some("auth.login".to_string()),
            error: Some(ParsedError {
                kind: "JWTInvalid".to_string(),
                code: "JWT_INVALID".to_string(),
                message: "signature mismatch".to_string(),
                retryable: false,
            }),
            ctx: Some(serde_json::json!({"doc_id": "doc123"})),
            fingerprint: Some(-1234567890),
        }
    }

    #[test]
    fn test_build_payload_has_required_fields() {
        let entry = make_entry();
        let payload = build_payload(&entry);
        assert!(payload["trace_id"].is_string());
        assert!(payload["event"].is_string());
        assert!(payload["error"]["code"].is_string());
        assert!(payload["fingerprint"].is_number());
    }
}
