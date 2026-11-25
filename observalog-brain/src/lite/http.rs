use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::model::IngestResponse;

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ingest/raw", post(ingest_raw))
        .with_state(pool)
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

/// Accepts JSON Lines from Fluent Bit and raw text lines from direct sources.
/// A 202 is returned only after raw input and its first durable gate-work item
/// have committed to PostgreSQL/TimescaleDB.
async fn ingest_raw(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty request body".into()));
    }
    let source_header = headers
        .get("x-observalog-source")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("fluent-bit");
    let records = decode_body(&body, source_header);
    if records.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no records found".into()));
    }

    let mut accepted = 0usize;
    let mut duplicates = 0usize;
    let mut tx = pool.begin().await.map_err(internal_error)?;

    for (raw_text, source_metadata, source_key) in records {
        let raw_id = Uuid::new_v4().to_string();
        let inserted: Option<String> = sqlx::query_scalar(
            r#"
            INSERT INTO lite_raw_records (id,raw_text,source_metadata,source_key)
            VALUES ($1,$2,$3,$4)
            ON CONFLICT (source_key) WHERE source_key IS NOT NULL DO NOTHING
            RETURNING id
            "#
        )
        .bind(&raw_id)
        .bind(raw_text)
        .bind(source_metadata)
        .bind(source_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal_error)?;

        if inserted.is_some() {
            sqlx::query(
                "INSERT INTO lite_gate_work (raw_id,gate_name) VALUES ($1,'normalize') ON CONFLICT (raw_id,gate_name) DO NOTHING"
            )
            .bind(raw_id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
            accepted += 1;
        } else {
            duplicates += 1;
        }
    }

    tx.commit().await.map_err(internal_error)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse { accepted, duplicates, rejected: 0 }),
    ))
}

fn decode_body(body: &[u8], source_header: &str) -> Vec<(String, Value, Option<String>)> {
    let text = String::from_utf8_lossy(body);
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        return values_to_records(value, source_header);
    }

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| record_from_line(line, source_header))
        .collect()
}

fn values_to_records(value: Value, source_header: &str) -> Vec<(String, Value, Option<String>)> {
    match value {
        Value::Array(items) => items.into_iter()
            .map(|item| record_from_value(item, source_header))
            .collect(),
        other => vec![record_from_value(other, source_header)],
    }
}

fn record_from_line(line: &str, source_header: &str) -> (String, Value, Option<String>) {
    match serde_json::from_str::<Value>(line) {
        Ok(value) => record_from_value(value, source_header),
        Err(_) => (line.to_string(), json!({"source_name":source_header,"format":"text"}), None),
    }
}

fn record_from_value(value: Value, source_header: &str) -> (String, Value, Option<String>) {
    let mut metadata = match &value {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({"record":value}),
    };
    if !metadata.is_object() {
        metadata = json!({"record":metadata});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.entry("source_name".to_string()).or_insert_with(|| json!(source_header));
    }

    let is_fluent_bit_envelope = value.get("log").and_then(Value::as_str).is_some()
        && (value.get("source_path").is_some() || value.get("tag").is_some());
    let raw_text = if is_fluent_bit_envelope {
        value.get("log").and_then(Value::as_str).unwrap_or_default().trim_end_matches('\n').to_string()
    } else {
        match &value {
            Value::String(s) => s.clone(),
            _ => serde_json::to_string(&value).unwrap_or_else(|_| value.to_string()),
        }
    };

    let source_path = value.get("source_path").and_then(Value::as_str);
    let source_offset = value.get("source_offset").and_then(value_to_string);
    let source_key = source_path.zip(source_offset.as_deref())
        .map(|(path, offset)| format!("{source_header}:{path}:{offset}"));

    (raw_text, metadata, source_key)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn internal_error(error: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, format!("durable ingest unavailable: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_fluent_bit_envelope_and_uses_path_offset_identity() {
        let line = r#"{"log":"{\"message\":\"hello\"}\n","source_path":"/var/log/app.log","source_offset":88,"tag":"app.raw"}"#;
        let (raw, metadata, key) = record_from_line(line, "fluent-bit");
        assert_eq!(raw, r#"{"message":"hello"}"#);
        assert_eq!(metadata["source_path"], "/var/log/app.log");
        assert_eq!(key.as_deref(), Some("fluent-bit:/var/log/app.log:88"));
    }

    #[test]
    fn accepts_plain_text_without_fabricating_identifiers() {
        let (raw, metadata, key) = record_from_line("2026-11-25 ERROR payment timed out", "manual");
        assert_eq!(raw, "2026-11-25 ERROR payment timed out");
        assert_eq!(metadata["source_name"], "manual");
        assert_eq!(key, None);
    }

    #[test]
    fn decodes_json_lines_and_arrays() {
        let values = decode_body(br#"{"message":"one"}
{"message":"two"}"#, "test");
        assert_eq!(values.len(), 2);
        let array = decode_body(br#"[{"message":"one"},{"message":"two"}]"#, "test");
        assert_eq!(array.len(), 2);
    }
}
