# ObservaLog Deployment Guide

---

## Architecture overview

```
Go services (observalog-go)
        │ stdout (NDJSON)
        ▼
  Fluent Bit (tail + GZIP)
        │ GZIP-compressed NDJSON
        ▼
    Kafka topic: observalog-logs
        │
        ▼
  observalog-brain (Rust)
        ├── TimescaleDB (log_index + log_payload)
        ├── Valkey/Redis (gap detection grace window)
        └── WebSocket API (port 4000)
```

---

## Required services

| Service         | Minimum version | Purpose |
|----------------|----------------|---------|
| Kafka          | 3.0            | Log transport and durability |
| TimescaleDB    | 2.11           | Hypertable log storage |
| Valkey / Redis | 7.0            | Out-of-order gap detection buffer |
| Anthropic API  | —              | LLM triage (claude-sonnet-4-6) |

---

## 1. TimescaleDB

### Setup

```sql
-- Create extension (TimescaleDB must already be installed)
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS vector;  -- pgvector for embeddings
```

The brain applies its own DDL at startup via `schema::run()`. You do not need to run migrations manually. The schema creates:

- `log_index` — hypertable on `ts`, 1-day chunks, typed columns only
- `log_payload` — full JSONB payload + VECTOR(384) embedding column
- `known_issues` — fingerprint dedup cache with repair recommendations
- All required indexes

### Retention policy

The brain applies a 7-day retention policy on `log_index`. ERROR and WARN traces are sampled before this policy applies — they are retained beyond the default window based on their outcome value.

### Critical query constraint

Every query that filters by `trace_id` must include a `ts` range bound:

```sql
WHERE trace_id = $1
  AND ts BETWEEN $2 AND $3
```

Without the `ts` bound, TimescaleDB scans all 30 chunks instead of 1. This constraint is enforced in `db/queries.rs`. Do not add ad-hoc queries that omit the time bound.

### Connection string

```
DATABASE_URL=postgres://observalog:password@timescaledb:5432/observalog
```

Recommended: 20 max connections (`PgPoolOptions::new().max_connections(20)`).

---

## 2. Kafka

### Topic setup

Create the log topic before starting the brain:

```bash
kafka-topics.sh --create \
  --topic observalog-logs \
  --partitions 6 \
  --replication-factor 2 \
  --bootstrap-server localhost:9092
```

Recommended partition count: 1 per brain consumer thread. Start with 6 for most workloads.

### Retention

Set log segment retention to at least 24 hours to allow the brain to recover from crashes without log loss:

```bash
kafka-configs.sh --alter \
  --entity-type topics \
  --entity-name observalog-logs \
  --add-config retention.ms=86400000 \
  --bootstrap-server localhost:9092
```

### Brain config

| Environment variable | Default | Description |
|---------------------|---------|-------------|
| `KAFKA_BROKERS`     | required | Comma-separated broker list |
| `KAFKA_TOPIC`       | `observalog-logs` | Topic name |
| `KAFKA_GROUP_ID`    | `observalog-brain` | Consumer group ID |

The brain commits Kafka offsets only after a successful TimescaleDB write. A crash recovers from the last committed offset with no log loss (Gap 9).

---

## 3. Fluent Bit

Fluent Bit tails each service's container stdout and forwards to Kafka with GZIP compression.

### Sample configuration

```ini
[SERVICE]
    flush        1
    log_level    info

[INPUT]
    name         tail
    path         /var/log/containers/*.log
    tag          observalog.*
    parser       docker
    read_from_head false

[FILTER]
    name         grep
    match        observalog.*
    regex        log ^[AB]:

[OUTPUT]
    name         kafka
    match        observalog.*
    brokers      kafka:9092
    topics       observalog-logs
    compression  gzip
    rdkafka.request.required.acks -1
```

The `grep` filter ensures only Part A and Part B lines are forwarded — other stdout lines (Go runtime, framework logs) are dropped.

### Docker / Kubernetes

For Kubernetes, use the DaemonSet pattern: one Fluent Bit pod per node, mounting `/var/log/containers`.

---

## 4. Valkey / Redis

Valkey provides the 30-second grace window for gap detection (Gap 5 / Gap 7).

### Setup

Any Valkey or Redis 7.0+ instance works. No special configuration required.

```bash
# Default connection
REDIS_URL=redis://localhost:6379
```

### Key pattern

The brain sets keys with the pattern:

```
gap:<trace_id>:<service>:<seq_before>
```

Each key has a 30-second TTL. The keyspace is self-cleaning — no manual cleanup needed.

### Memory estimate

Each key is ~60 bytes. At 10,000 traces/second with 2 services each, the keyspace is approximately 1.2M keys, ~72 MB. Well within Valkey's defaults.

---

## 5. observalog-brain

### Environment variables

| Variable             | Required | Default                     | Description |
|---------------------|----------|-----------------------------|-------------|
| `DATABASE_URL`      | yes      | —                           | TimescaleDB connection string |
| `KAFKA_BROKERS`     | yes      | —                           | Kafka broker addresses |
| `KAFKA_TOPIC`       | no       | `observalog-logs`           | Log topic |
| `KAFKA_GROUP_ID`    | no       | `observalog-brain`          | Consumer group |
| `REDIS_URL`         | no       | `redis://localhost:6379`    | Valkey/Redis |
| `ANTHROPIC_API_KEY` | yes      | —                           | LLM triage |
| `LISTEN_ADDR`       | no       | `0.0.0.0:4000`              | WebSocket API bind address |
| `RUST_LOG`          | no       | `info`                      | Log level filter |

### Running

```bash
DATABASE_URL=postgres://observalog:password@timescaledb:5432/observalog \
KAFKA_BROKERS=kafka:9092 \
REDIS_URL=redis://valkey:6379 \
ANTHROPIC_API_KEY=sk-ant-... \
./observalog-brain
```

### Docker

```dockerfile
FROM debian:bookworm-slim
COPY observalog-brain /usr/local/bin/observalog-brain
EXPOSE 4000
CMD ["observalog-brain"]
```

### WebSocket API

The brain exposes a WebSocket API at `ws://<host>:4000/ws/triage`.

Send a JSON message:

```json
{ "trace_id": "trc_7f2a1b9e4d" }
```

Receive a triage result:

```json
{
  "trace_id": "trc_7f2a1b9e4d",
  "failure_class": "ExternalDependency",
  "repair_id": "RATE_LIMIT_BACKOFF",
  "confidence": 0.91,
  "summary": "Provider quota exceeded on doc-service outbound send. Retryable.",
  "call_graph": [...],
  "gaps": [],
  "cached": false
}
```

---

## 6. Go services (observalog-go)

### Environment variables

| Variable         | Required | Default | Description |
|-----------------|----------|---------|-------------|
| `SERVICE_NAME`  | yes      | —       | Service identifier (must be registered service code) |
| `ENV`           | yes      | —       | `production`, `staging`, `development` |
| `LOG_LEVEL`     | no       | `info`  | Minimum log level: `debug`, `info`, `warn`, `error` |
| `LOG_BUFFER_SIZE` | no     | `10000` | Channel buffer for async drain goroutine |

### Initialization

```go
func main() {
    log.Init(log.ConfigFromEnv("v1.2.3-abc123def"))
    defer log.Shutdown()
    // ...
}
```

`ConfigFromEnv` reads `SERVICE_NAME`, `ENV`, `LOG_LEVEL`, `LOG_BUFFER_SIZE` from the environment. The `version` argument should be the git SHA or semver tag baked in at build time.

### HTTP middleware

```go
http.ListenAndServe(":8080", middleware.Middleware(yourRouter))
```

The middleware:
- Generates a `trace_id` for each request (or propagates `X-Trace-ID` from upstream)
- Sets `span_id`, `journey_stage` (from the HTTP path), `user_id` (from the request context)
- Initializes the per-request `seq` counter at 1

### Kafka consumer middleware

```go
for msg := range consumer.Messages() {
    ctx := middleware.KafkaMiddleware(context.Background(), msg)
    // handle msg using ctx
}
```

### Buffer overflow

When the async channel buffer is full, log entries are dropped (non-blocking). The count of dropped entries is available via `log.DroppedLogCount()`. Expose this as a metric and alert if it grows.

---

## 7. logscanner (CI)

### Installation

Build from source:

```bash
cd logscanner && cargo build --release
cp target/release/logscanner /usr/local/bin/
```

### CI usage

```yaml
- name: Scan logs
  run: logscanner --files $(git diff --name-only origin/main...HEAD | grep '\.go$')
```

Scan a directory:

```bash
logscanner --dir ./services/auth-service
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0    | No findings |
| 1    | ERROR findings present — block merge |
| 2    | WARN findings only — allow merge, notify |

---

## 8. Health checks

| Component       | Check |
|----------------|-------|
| observalog-brain | `GET /health` → 200 OK |
| TimescaleDB    | `SELECT 1` via `DATABASE_URL` |
| Kafka          | Consumer group lag (alert if > 10,000 messages) |
| Valkey         | `PING` → PONG |
| Brain WebSocket | Connection to `ws://<host>:4000/ws/triage` |

---

## 9. Scaling

| Bottleneck          | Resolution |
|--------------------|------------|
| Brain ingest lag   | Add Kafka partitions; run multiple brain instances in the same consumer group |
| TimescaleDB writes | Batch inserts; increase `max_connections` |
| LLM call volume    | Fingerprint dedup already eliminates ~95%. For remaining 5%, add request queuing in `triage/dedup.rs` |
| Valkey memory      | Increase TTL from 30s → 15s if keyspace grows beyond 1 GB |

---

## 10. Monitoring recommendations

| Metric                         | Alert threshold |
|-------------------------------|----------------|
| `log.DroppedLogCount()`       | > 0 per minute |
| Kafka consumer lag             | > 10,000 messages |
| Brain triage latency (p99)    | > 2 seconds |
| LLM calls per minute          | Sudden spike (fingerprint dedup degraded) |
| TimescaleDB chunk count       | Retention policy not pruning |
| `log_payload` rows without matching `log_index` row | > 0 (orphan payloads — write failure) |
