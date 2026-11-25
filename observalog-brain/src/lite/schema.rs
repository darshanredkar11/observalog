use anyhow::Result;
use sqlx::PgPool;

pub const DDL: &str = r#"
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE IF NOT EXISTS lite_raw_records (
    id              TEXT PRIMARY KEY,
    raw_text        TEXT NOT NULL,
    source_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    source_key      TEXT,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS lite_raw_source_key_uidx
    ON lite_raw_records (source_key) WHERE source_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS lite_raw_received_idx ON lite_raw_records (received_at DESC);

CREATE TABLE IF NOT EXISTS lite_gate_work (
    id           BIGSERIAL PRIMARY KEY,
    raw_id       TEXT NOT NULL REFERENCES lite_raw_records(id) ON DELETE CASCADE,
    gate_name    TEXT NOT NULL,
    payload_id   BIGINT,
    event_ts     TIMESTAMPTZ,
    state        TEXT NOT NULL DEFAULT 'pending',
    attempts     INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_until  TIMESTAMPTZ,
    last_error   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (raw_id, gate_name),
    CHECK (state IN ('pending','leased','done','dead'))
);
CREATE INDEX IF NOT EXISTS lite_gate_claim_idx
    ON lite_gate_work (gate_name, state, available_at, id);

CREATE TABLE IF NOT EXISTS lite_source_identity (
    source_key  TEXT PRIMARY KEY,
    payload_id  BIGINT NOT NULL,
    first_seen  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lite_event_payload (
    id              BIGSERIAL PRIMARY KEY,
    raw_id          TEXT NOT NULL UNIQUE,
    raw_text        TEXT NOT NULL,
    source_metadata JSONB NOT NULL,
    canonical       JSONB NOT NULL,
    provenance      JSONB NOT NULL,
    quality         JSONB NOT NULL DEFAULT '{}'::jsonb,
    stored_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS lite_event_payload_gin
    ON lite_event_payload USING GIN (canonical jsonb_path_ops);

CREATE TABLE IF NOT EXISTS lite_event_index (
    id                 BIGINT NOT NULL,
    event_ts           TIMESTAMPTZ NOT NULL,
    observed_at        TIMESTAMPTZ NOT NULL,
    service            TEXT,
    instance           TEXT,
    version            TEXT,
    environment        TEXT,
    region             TEXT,
    tenant_id          TEXT,
    severity           TEXT,
    trace_id           TEXT,
    span_id            TEXT,
    parent_span_id     TEXT,
    request_id         TEXT,
    business_entity_id TEXT,
    event_name         TEXT,
    outcome            TEXT,
    duration_ms        DOUBLE PRECISION,
    error_code         TEXT,
    source_name        TEXT,
    quality_score      DOUBLE PRECISION NOT NULL DEFAULT 0,
    quality_flags      JSONB NOT NULL DEFAULT '[]'::jsonb,
    PRIMARY KEY (event_ts, id)
);

SELECT create_hypertable(
    'lite_event_index', 'event_ts',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

CREATE INDEX IF NOT EXISTS lite_event_trace_time_idx
    ON lite_event_index (trace_id, event_ts DESC);
CREATE INDEX IF NOT EXISTS lite_event_request_time_idx
    ON lite_event_index (request_id, event_ts DESC);
CREATE INDEX IF NOT EXISTS lite_event_entity_time_idx
    ON lite_event_index (business_entity_id, event_ts DESC);
CREATE INDEX IF NOT EXISTS lite_event_service_time_idx
    ON lite_event_index (service, event_ts DESC);
CREATE INDEX IF NOT EXISTS lite_event_error_time_idx
    ON lite_event_index (severity, event_ts DESC)
    WHERE severity IN ('ERROR','FATAL');

CREATE TABLE IF NOT EXISTS lite_event_links (
    event_a       BIGINT NOT NULL,
    event_b       BIGINT NOT NULL,
    relation_type TEXT NOT NULL,
    confidence    DOUBLE PRECISION NOT NULL,
    evidence      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (event_a, event_b, relation_type),
    CHECK (event_a <> event_b)
);
CREATE INDEX IF NOT EXISTS lite_event_links_b_idx ON lite_event_links (event_b);

CREATE TABLE IF NOT EXISTS lite_signals (
    id           BIGSERIAL PRIMARY KEY,
    event_id     BIGINT NOT NULL,
    event_ts     TIMESTAMPTZ NOT NULL,
    signal_type  TEXT NOT NULL,
    severity     TEXT NOT NULL,
    evidence     JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id, signal_type)
);
CREATE INDEX IF NOT EXISTS lite_signals_time_idx ON lite_signals (event_ts DESC, severity);

CREATE TABLE IF NOT EXISTS lite_findings (
    id                BIGSERIAL PRIMARY KEY,
    incident_key      TEXT NOT NULL UNIQUE,
    category          TEXT NOT NULL,
    title             TEXT NOT NULL,
    likely_cause      TEXT NOT NULL,
    confidence        DOUBLE PRECISION NOT NULL,
    primary_event_id  BIGINT NOT NULL,
    evidence_event_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    affected_services JSONB NOT NULL DEFAULT '[]'::jsonb,
    alternatives      JSONB NOT NULL DEFAULT '[]'::jsonb,
    evidence          JSONB NOT NULL DEFAULT '{}'::jsonb,
    first_seen        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen         TIMESTAMPTZ NOT NULL DEFAULT now(),
    occurrence_count  BIGINT NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS lite_findings_last_seen_idx ON lite_findings (last_seen DESC);
"#;

pub async fn run(pool: &PgPool) -> Result<()> {
    // This DDL is a trusted, compile-time constant and intentionally contains
    // multiple statements (including TimescaleDB extension/hypertable setup).
    sqlx::raw_sql(DDL).execute(pool).await?;
    Ok(())
}
