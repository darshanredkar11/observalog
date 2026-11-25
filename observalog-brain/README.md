# ObservaLog Lite Brain

Rust runtime for raw-log ingestion and streaming analysis. It accepts heterogeneous raw logs over HTTP, persists them before acknowledging, then moves each event through independent Timescale-backed gates.

## Environment

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `DATABASE_URL` | yes | — | TimescaleDB connection string |
| `LITE_INGEST_TOKEN` | yes | — | Bearer token required by `POST /ingest/raw` (minimum 24 bytes) |
| `LISTEN_ADDR` | no | `0.0.0.0:4000` | HTTP bind address |
| `DB_MAX_CONNECTIONS` | no | `20` | SQL connection pool size |
| `LITE_SLOW_THRESHOLD_MS` | no | `1000` | Slow-operation signal threshold |
| `LITE_MAX_INFLIGHT_GATE_WORK` | no | `50000` | Maximum pending/leased durable work across all gates; full queue returns HTTP 503 |
| `RUST_LOG` | no | `info` | Tracing filter |

No Anthropic key, Kafka broker or Valkey server is required.

## API

- `GET /health`
- `POST /ingest/raw` — accepts one JSON record, a JSON array, JSON Lines or raw text.

The endpoint requires `Authorization: Bearer <LITE_INGEST_TOKEN>` and returns HTTP 202 after the raw journal row and normalization work item commit in one database transaction. If persistence is unavailable or the bounded durable gate queue is full it returns 503, allowing Fluent Bit to retry from its filesystem buffer. The HTTP request-body limit is 16 MB.

## Delivery and recovery

Each gate claims work from `lite_gate_work` using `FOR UPDATE SKIP LOCKED`, a lease and a durable cursor. Gate outputs and the next gate's work item are committed transactionally where both are database operations. Failed work is retried with bounded backoff and moved to `dead` after ten attempts, with the error retained for inspection.

## Run

```bash
DATABASE_URL=postgres://observalog:password@localhost:5432/observalog cargo run --release
```

Run tests with `cargo test`.
