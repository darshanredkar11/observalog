# ObservaLog Lite Deployment Guide

The current deployment has exactly three runtime components:

1. **Fluent Bit** collects container and file logs, preserves source metadata/offsets, buffers on filesystem and retries HTTP delivery.
2. **Brain** accepts raw records at `POST /ingest/raw`, commits them with a durable normalization work item, and runs independent gate workers.
3. **TimescaleDB** stores raw records, canonical JSONB, typed time-series projection, gate work, relationships, signals and findings.

Kafka, Valkey/Redis, an ObservaLog SDK and an LLM key are not runtime requirements.

## Local deployment

```bash
cp .env.example .env
docker compose up -d --build
curl -fsS http://localhost:4000/health
```

Place application log files under `./var/logs/*.log`. On Linux Docker hosts, Fluent Bit also tails `/var/lib/docker/containers/*/*.log`.

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `POSTGRES_PASSWORD` | `observalog` | Local TimescaleDB password; change outside local development |
| `DATABASE_URL` | Compose-generated | Brain database connection |
| `LISTEN_ADDR` | `0.0.0.0:4000` | Brain HTTP listener |
| `DB_MAX_CONNECTIONS` | `20` | SQL connection pool |
| `LITE_SLOW_THRESHOLD_MS` | `1000` | Slow-operation signal threshold |
| `LITE_MAX_INFLIGHT_GATE_WORK` | `50000` | Maximum pending/leased durable work across all gates |

When the durable work limit is reached, raw ingest returns HTTP 503 and rolls back the batch. Fluent Bit retains/retries it using filesystem buffering. The HTTP body limit is 16 MB.

## Ingestion API

`GET /health`

`POST /ingest/raw` accepts a JSON object, JSON array, JSON Lines or plain text. Include an optional `X-ObservaLog-Source` header. A HTTP 202 response means both the raw event and initial work item have committed to TimescaleDB.

## Recovery

See [the replay and recovery runbook](lite/replay-and-recovery.md) for queue monitoring, safe downstream gate replay and poison-record diagnosis.

## Verification

GitHub Actions validates the Rust build/tests/Clippy, incident generator and actual Fluent Bit → Brain → TimescaleDB flow. The release gate is the three-permutation integration run: each arrival permutation uses a fresh database and checks raw durability, normalized rows, findings, parent-child links and zero dead/pending work.

For the full architecture and current implementation constraints, see [ObservaLog Lite](lite/README.md).
