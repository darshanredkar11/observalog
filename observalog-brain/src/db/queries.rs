use crate::ingest::parser::ParsedEntry;
use crate::triage::repair::RepairId;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

/// A row from log_index.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct IndexRow {
    pub id: i64,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span: Option<String>,
    pub service: i16,
    pub level: i16,
    pub outcome: Option<i16>,
    pub seq: i16,
    pub ts: DateTime<Utc>,
    pub user_id: Option<String>,
    pub fingerprint: Option<i64>,
    pub payload_id: i64,
}

/// A row from known_issues.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnownIssue {
    pub fingerprint: i64,
    pub repair_id: String,
    pub cached_fix: Option<String>,
    pub occurrence_count: i64,
    pub service: i16,
    pub event: String,
    pub error_code: String,
}

// ─── Writes ──────────────────────────────────────────────────────────────────

/// Insert log_payload and return its id.
/// Decision 8 / Gap 4: MUST be called before insert_log_index.
pub async fn insert_log_payload(pool: &PgPool, data: &serde_json::Value) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("INSERT INTO log_payload (data) VALUES ($1) RETURNING id")
        .bind(data)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Insert log_index row. payload_id must already exist (INVARIANT 1).
pub async fn insert_log_index(pool: &PgPool, entry: &ParsedEntry, payload_id: i64) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO log_index
            (trace_id, span_id, parent_span, service, level, outcome,
             seq, ts, user_id, fingerprint, payload_id)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        RETURNING id
        "#,
    )
    .bind(&entry.trace_id)
    .bind(&entry.span_id)
    .bind(&entry.parent_span)
    .bind(entry.service as i16)
    .bind(entry.level as i16)
    .bind(entry.outcome.map(|o| o as i16))
    .bind(entry.seq as i16)
    .bind(entry.ts)
    .bind(&entry.user_id)
    .bind(entry.fingerprint)
    .bind(payload_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// ─── Reads ────────────────────────────────────────────────────────────────────

/// Full journey fetch. CRITICAL: always supply ts bounds (Gap 1 / Decision 10).
pub async fn fetch_journey(
    pool: &PgPool,
    trace_id: &str,
    ts_from: DateTime<Utc>,
    ts_to: DateTime<Utc>,
) -> Result<Vec<IndexRow>> {
    let rows = sqlx::query_as::<_, IndexRow>(
        r#"
        SELECT id, trace_id, span_id, parent_span, service, level,
               outcome, seq, ts, user_id, fingerprint, payload_id
        FROM   log_index
        WHERE  trace_id = $1
          AND  ts BETWEEN $2 AND $3
        ORDER  BY ts ASC
        "#,
    )
    .bind(trace_id)
    .bind(ts_from)
    .bind(ts_to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fetch service+seq pairs for gap detection.
pub async fn fetch_seq_chain(
    pool: &PgPool,
    trace_id: &str,
    ts_from: DateTime<Utc>,
    ts_to: DateTime<Utc>,
) -> Result<Vec<(i16, i16)>> {
    let rows: Vec<(i16, i16)> = sqlx::query_as(
        r#"
        SELECT service, seq
        FROM   log_index
        WHERE  trace_id = $1
          AND  ts BETWEEN $2 AND $3
        ORDER  BY service, seq ASC
        "#,
    )
    .bind(trace_id)
    .bind(ts_from)
    .bind(ts_to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// O(1) fingerprint dedup check.
pub async fn find_known_issue(pool: &PgPool, fingerprint: i64) -> Result<Option<KnownIssue>> {
    let row = sqlx::query_as::<_, KnownIssue>(
        r#"
        SELECT fingerprint, repair_id, cached_fix, occurrence_count, service, event, error_code
        FROM   known_issues
        WHERE  fingerprint = $1
        "#,
    )
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Upsert a known issue after LLM triage.
pub async fn upsert_known_issue(
    pool: &PgPool,
    fingerprint: i64,
    service: i16,
    event: &str,
    error_code: &str,
    repair_id: &RepairId,
    cached_fix: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO known_issues
            (fingerprint, service, event, error_code, repair_id, cached_fix)
        VALUES ($1,$2,$3,$4,$5,$6)
        ON CONFLICT (fingerprint) DO UPDATE SET
            last_seen        = now(),
            occurrence_count = known_issues.occurrence_count + 1,
            cached_fix       = COALESCE(EXCLUDED.cached_fix, known_issues.cached_fix)
        "#,
    )
    .bind(fingerprint)
    .bind(service)
    .bind(event)
    .bind(error_code)
    .bind(repair_id.as_str())
    .bind(cached_fix)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch payloads for a set of payload_ids.
/// Gap 10: only call when result set <= MAX_PAYLOAD_FETCH_ROWS.
pub async fn fetch_payloads(pool: &PgPool, ids: &[i64]) -> Result<Vec<serde_json::Value>> {
    let rows: Vec<(serde_json::Value,)> =
        sqlx::query_as("SELECT data FROM log_payload WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}

/// Recent error traces for a user.
pub async fn user_error_traces(pool: &PgPool, user_id: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT trace_id
        FROM   log_index
        WHERE  user_id = $1
          AND  ts > now() - INTERVAL '1 hour'
          AND  level >= 2
        ORDER  BY trace_id
        LIMIT  20
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}
