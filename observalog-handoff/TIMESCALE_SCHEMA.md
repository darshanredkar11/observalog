# ObservaLog — TimescaleDB Schema

Complete schema for observalog-brain. Run once at startup via db/schema.rs.

---

## Extensions

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS vector;
```

---

## log_index (hot table — brain queries this for 95% of its work)

```sql
CREATE TABLE IF NOT EXISTS log_index (
    id           BIGSERIAL    PRIMARY KEY,
    trace_id     CHAR(14)     NOT NULL,
    span_id      CHAR(7)      NOT NULL,
    parent_span  CHAR(7),
    service      SMALLINT     NOT NULL,   -- 0=system 1=auth 2=doc 3=provider
    level        SMALLINT     NOT NULL,   -- 0=debug 1=info 2=warn 3=error
    outcome      SMALLINT,                -- 0=none 1=success 2=failure 3=partial
    seq          SMALLINT     NOT NULL,
    ts           TIMESTAMPTZ  NOT NULL,
    user_id      TEXT,
    fingerprint  BIGINT,                  -- xxHash64, null when no error
    payload_id   BIGINT       NOT NULL
);

SELECT create_hypertable(
    'log_index', 'ts',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);
```

## log_index indexes

```sql
-- Primary brain hot path: full journey fetch
-- CRITICAL: always pair with ts range bound (Gap 1 fix)
CREATE INDEX IF NOT EXISTS log_idx_trace
    ON log_index USING HASH (trace_id);

-- Time range scans — exploits physical write order
CREATE INDEX IF NOT EXISTS log_idx_ts
    ON log_index USING BRIN (ts);

-- Dedup check O(1) — partial, error logs only
CREATE INDEX IF NOT EXISTS log_idx_fingerprint
    ON log_index USING HASH (fingerprint)
    WHERE fingerprint IS NOT NULL;

-- User investigation — partial, WARN+ERROR only
CREATE INDEX IF NOT EXISTS log_idx_user_errors
    ON log_index (user_id, ts DESC)
    WHERE level >= 2;

-- Gap detection — composite covers trace+service+seq ordering
CREATE INDEX IF NOT EXISTS log_idx_gap_detect
    ON log_index (trace_id, service, seq);
```

---

## log_payload (cold table — fetched only for interesting rows)

```sql
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
```

---

## known_issues (fingerprint dedup cache)

```sql
CREATE TABLE IF NOT EXISTS known_issues (
    fingerprint      BIGINT       PRIMARY KEY,
    first_seen       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    last_seen        TIMESTAMPTZ  NOT NULL DEFAULT now(),
    occurrence_count BIGINT       NOT NULL DEFAULT 1,
    repair_id        TEXT         NOT NULL,  -- RepairId enum value
    cached_fix       TEXT,                   -- LLM-generated fix text
    service          SMALLINT     NOT NULL,
    event            TEXT         NOT NULL,
    error_code       TEXT         NOT NULL
);

CREATE INDEX IF NOT EXISTS known_issues_fp
    ON known_issues USING HASH (fingerprint);
```

---

## Retention policies (TTL — automatic chunk drops)

```sql
-- Apply after hypertable creation
SELECT add_retention_policy('log_index', INTERVAL '7 days', if_not_exists => TRUE);
-- Note: brain applies outcome-based sampling before insert
-- ERROR/WARN traces are always stored regardless of age
-- This policy covers INFO success traces only
```

---

## Brain query patterns

### Full journey fetch (hot path — always include ts bound)
```sql
SELECT id, trace_id, span_id, parent_span, service, level, outcome, seq, ts, fingerprint, payload_id
FROM log_index
WHERE trace_id = $1
  AND ts BETWEEN $2 AND $3
ORDER BY ts ASC;
```

### Gap detection
```sql
SELECT service, seq
FROM log_index
WHERE trace_id = $1
  AND ts BETWEEN $2 AND $3
ORDER BY service, seq ASC;
```

### Fingerprint dedup check
```sql
SELECT repair_id, cached_fix, occurrence_count
FROM known_issues
WHERE fingerprint = $1;
```

### Payload fetch (only for interesting rows)
```sql
SELECT data
FROM log_payload
WHERE id = ANY($1);
```

### User investigation
```sql
SELECT DISTINCT trace_id
FROM log_index
WHERE user_id = $1
  AND ts > now() - INTERVAL '1 hour'
  AND level >= 2
ORDER BY ts DESC
LIMIT 20;
```
