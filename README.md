# ObservaLog Lite

**Accept raw logs from any application. Normalize what can be understood, correlate evidence across services, and identify likely failures without requiring an ObservaLog SDK.**

The lite runtime is intentionally reduced to three components:

- **Fluent Bit** — tails container and application logs, buffers locally, and forwards raw records.
- **Brain** — HTTP ingest plus independent, durable processing gates.
- **TimescaleDB** — raw journal, canonical payload store, hot typed index, gate work queue, relationships, signals and findings.

There is no mandatory application SDK, fixed producer wire format, Kafka, Valkey or LLM dependency in this branch.

## Architecture

```
Any application / container / file
  JSON · Spring · Python · Pino · OTel · Nginx · plain text
                  |
              Fluent Bit
       durable filesystem buffering
                  |
       POST /ingest/raw (HTTP)
                  |
        durable raw journal
                  |
       +----------+-----------+
       |          |           |
   Normalize   Quality    Correlate
       |          |           |
       +----------+-----------+
                  |
               Detect
                  |
            Main reasoning
                  |
        evidence-backed findings
                  |
              TimescaleDB
```

Each brain gate is an independent worker. Work is stored durably in TimescaleDB, claimed with row locking, retried with backoff, and marked complete in the same transaction that enqueues the next stage. In-memory worker count is fixed and small; there is no unbounded in-process queue.

The adapter never invents trace IDs, spans, outcome or duration. Missing information remains unknown, conflicting aliases become quality flags, and original source data is retained next to the canonical event.

## Run locally

```bash
cp .env.example .env
# Replace LITE_INGEST_TOKEN with a random secret (at least 24 bytes).
set -a; . ./.env; set +a
docker compose up -d --build
```

The three runtime components are TimescaleDB, brain and Fluent Bit. Published ports bind to loopback only for local development. Do not expose the plain-HTTP ingest endpoint on an untrusted network; use a private network or TLS-terminating ingress. The brain refuses to start without `LITE_INGEST_TOKEN` (minimum 24 bytes); replace the example token before exposing the stack beyond a local development machine.

- Brain health: `http://localhost:4000/health`
- Raw log ingest: `POST http://localhost:4000/ingest/raw`
- Fluent Bit metrics: `http://localhost:2020/api/v1/metrics/prometheus`

Place application log files under `./var/logs/`; Fluent Bit tails `*.log`. Container stdout/stderr is tailed automatically on Linux Docker hosts where `/var/lib/docker/containers` is available.

Direct JSON Lines example:

```bash
curl -X POST http://localhost:4000/ingest/raw \
  -H 'Content-Type: application/x-ndjson' \
  -H "X-ObservaLog-Token: $LITE_INGEST_TOKEN" \
  -H 'X-ObservaLog-Source: checkout-service' \
  --data-binary '{"timestamp":"2026-11-25T12:00:00Z","level":"ERROR","service":"checkout","trace_id":"t-001","message":"Payment provider timed out","error_code":"UPSTREAM_TIMEOUT"}'
```

Raw ingest requires `X-ObservaLog-Token: $LITE_INGEST_TOKEN`. A successful response is HTTP 202 only after the raw record and normalization work item are durably committed. Requests are limited to 16 MB. The durable in-flight gate queue is bounded (default 50,000 pending/leased work items); when full, ingest returns HTTP 503 and Fluent Bit retries from its filesystem buffer.

## Brain gates

1. **Normalize** — source-specific JSON aliases and common text layouts map to a permissive canonical event. Raw text is retained.
2. **Quality** — missing fields, invalid timestamps, unmapped formats and conflicting aliases are recorded.
3. **Correlate** — explicit trace, request, business entity and parent/child span relations are kept as different edge types. Conflicting reused trace IDs are surfaced, not blindly joined.
4. **Detect** — deterministic error, failure, latency, retry, error-payload and business-invariant signals.
5. **Reason** — evidence-linked hypotheses for authentication/configuration, release/config drift, dependency/network, database/concurrency, resources, data contracts, idempotency and business consistency.

A finding is a hypothesis, not a root-cause guarantee. The current deterministic reasoning gate explicitly avoids treating time proximity as causality and records evidence and limitations. LLM reasoning and automated remediation are not on the ingestion path.

## Storage

- `lite_raw_records`: original text, source metadata, optional source path/offset identity.
- `lite_event_payload`: full raw and canonical JSONB, provenance and quality.
- `lite_event_index`: typed hot columns in a Timescale hypertable with a composite key including event time.
- `lite_gate_work`: independently leased/retried durable work for each gate.
- `lite_event_links`: explicit correlation and parent/child edges.
- `lite_signals`: deterministic behavioral signals.
- `lite_findings`: incident hypotheses with evidence references and replay-stable event-time/occurrence counters.

## Incident lab

See [tests/incident_lab](tests/incident_lab/README.md). It generates 22 realistic scenarios across 1–12 service identities, heterogeneous log formats, arrival disorder, duplicate records and missing optional fields. Evaluator truth is stored separately and must never be mounted into Fluent Bit or exposed to the brain.

## Build

```bash
cd observalog-brain
cargo build --release
cargo test
cargo clippy -- -D warnings
```

See [docs/lite](docs/lite/README.md) for the source audit, gate contracts and reasoning model.
