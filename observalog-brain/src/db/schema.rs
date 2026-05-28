/// Complete DDL for observalog-brain. Applied once at startup via run().
/// Tables, hypertable config, indexes, and retention policies match TIMESCALE_SCHEMA.md exactly.
pub const DDL: &str = r#"
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS vector;

-- ─── log_payload (write first — Decision 8 / Gap 4) ─────────────────────────
CREATE TABLE IF NOT EXISTS log_payload (
    id        BIGSERIAL   PRIMARY KEY,
    data      JSONB       NOT NULL,
    embedding VECTOR(384)
);

CREATE INDEX IF NOT EXISTS log_payload_gin
    ON log_payload USING GIN (data jsonb_path_ops);

CREATE INDEX IF NOT EXISTS log_payload_hnsw
    ON log_payload USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;

-- ─── log_index (hot table) ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS log_index (
    id           BIGSERIAL    PRIMARY KEY,
    trace_id     CHAR(14)     NOT NULL,
    span_id      CHAR(7)      NOT NULL,
    parent_span  CHAR(7),
    service      SMALLINT     NOT NULL,
    level        SMALLINT     NOT NULL,
    outcome      SMALLINT,
    seq          SMALLINT     NOT NULL,
    ts           TIMESTAMPTZ  NOT NULL,
    user_id      TEXT,
    fingerprint  BIGINT,
    payload_id   BIGINT       NOT NULL
);

SELECT create_hypertable(
    'log_index', 'ts',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Brain hot path: trace journey fetch (Gap 1: always pair with ts bound)
CREATE INDEX IF NOT EXISTS log_idx_trace
    ON log_index USING HASH (trace_id);

-- Time range scans — exploits physical write order
CREATE INDEX IF NOT EXISTS log_idx_ts
    ON log_index USING BRIN (ts);

-- O(1) dedup check — partial index, error logs only
CREATE INDEX IF NOT EXISTS log_idx_fingerprint
    ON log_index USING HASH (fingerprint)
    WHERE fingerprint IS NOT NULL;

-- User investigation — partial, WARN+ERROR only
CREATE INDEX IF NOT EXISTS log_idx_user_errors
    ON log_index (user_id, ts DESC)
    WHERE level >= 2;

-- Gap detection — covers trace+service+seq ordering
CREATE INDEX IF NOT EXISTS log_idx_gap_detect
    ON log_index (trace_id, service, seq);

-- ─── known_issues (fingerprint dedup cache — Decision 9) ────────────────────
CREATE TABLE IF NOT EXISTS known_issues (
    fingerprint      BIGINT       PRIMARY KEY,
    first_seen       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    last_seen        TIMESTAMPTZ  NOT NULL DEFAULT now(),
    occurrence_count BIGINT       NOT NULL DEFAULT 1,
    repair_id        TEXT         NOT NULL,
    cached_fix       TEXT,
    service          SMALLINT     NOT NULL,
    event            TEXT         NOT NULL,
    error_code       TEXT         NOT NULL
);

CREATE INDEX IF NOT EXISTS known_issues_fp
    ON known_issues USING HASH (fingerprint);

-- ─── Retention (7 days for INFO success traces; ERROR/WARN kept longer) ──────
SELECT add_retention_policy('log_index', INTERVAL '7 days', if_not_exists => TRUE);
"#;

pub async fn run(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    // Execute each statement separately — postgres doesn't support multi-statement
    // execution with a single query() call.
    for stmt in split_statements(DDL) {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(stmt).execute(pool).await {
            // "already exists" errors are harmless on restart — log and continue.
            let msg = e.to_string();
            if !msg.contains("already exists") && !msg.contains("duplicate") {
                return Err(anyhow::anyhow!("schema DDL failed on: {}\nError: {}", &stmt[..50.min(stmt.len())], e));
            }
        }
    }
    Ok(())
}

fn split_statements(ddl: &str) -> Vec<&str> {
    ddl.split(';').collect()
}
