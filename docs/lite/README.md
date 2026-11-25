# ObservaLog Lite

A reduced ObservaLog runtime built inside the parent `darshanredkar11/observalog` repository. The active runtime is:

`Fluent Bit → HTTP raw-ingest → durable TimescaleDB journal → independent brain workers`

Kafka, Valkey/Redis and an LLM are not required by this runtime. No independent `observalog-brain` repository is used.

## Runtime status

- Fluent Bit raw collection and filesystem buffering: configured.
- HTTP ingestion: accepts Fluent Bit JSON Lines, arrays, JSON objects and plain text; acknowledges only after raw records and their first work items commit.
- Canonical adapter: JSON aliases, nested OpenTelemetry-style attributes, Pino numeric levels, common Spring/Python-style text and key/value context; preserves raw text and provenance.
- Durable stages: normalization, quality, correlation, signal detection and deterministic evidence-based reasoning.
- Storage: raw journal, canonical JSONB payload, TimescaleDB typed event projection, durable work rows, relationship edges, signals and findings.
- CI: Rust build, tests and Clippy pass on the current implementation line; incident generator checks pass. The real Fluent Bit → TimescaleDB workflow is still the integration acceptance gate and must be green before this branch is considered release-ready.

The legacy SDKs and legacy source directories are retained in the branch until the replacement path passes the real end-to-end test. They are not compiled into the current lite binary.

## Start locally

Requirements: Docker Compose v2 and a working Docker daemon.

```bash
docker compose up -d --build
curl -fsS http://localhost:4000/health
```

The three runtime services are:

- `timescaledb` — durable raw journal, gate work, canonical payloads and event projections.
- `brain` — raw-ingest endpoint plus independent workers.
- `fluent-bit` — tails container logs and `./var/logs/*.log`, writes file offsets to its persistent state volume, buffers output to filesystem and retries HTTP delivery.

For local direct file ingestion, place logs under `./var/logs/`. The HTTP endpoint is `POST http://localhost:4000/ingest/raw`. Send JSON Lines, a JSON array/object, or plain text. Optional header: `X-ObservaLog-Source`.

`DATABASE_URL`, `LISTEN_ADDR`, `DB_MAX_CONNECTIONS` and `LITE_SLOW_THRESHOLD_MS` and `LITE_MAX_INFLIGHT_GATE_WORK` configure the brain. The default in-flight durable work cap is 50,000; when it is reached the ingest endpoint returns HTTP 503 so Fluent Bit retries from its filesystem buffer. The default slow-operation threshold is 1000 ms.

## Processing stages

Each gate has its own durable work row in `lite_gate_work`. Workers claim work with row locking and `SKIP LOCKED`; completion and downstream work creation are committed transactionally. Failed work is retried with backoff and marked `dead` after ten attempts. A worker restart can reclaim expired leases.

1. `normalize` — parse one raw record; preserve source text and source envelope; persist canonical event, provenance and hot typed projection.
2. `quality` — calculate mapping confidence and report missing or conflicting optional information.
3. `correlate` — create explicit links from trace, request, business-entity and parent/span evidence; reject contradictory identity joins.
4. `detect` — emit bounded event signals such as errors, slow operations, retry activity, error/severity mismatches and business invariant markers.
5. `reason` — group evidence and write deterministic hypotheses/findings with alternatives and uncertainty.

A missing trace ID is normal input, not a parse failure. The adapter does not invent trace IDs, span IDs, outcomes or durations. Timestamp fallback uses the collector observation time and marks it as inferred.

## Data layout

- `lite_raw_records`: durable raw text, source metadata and collector receipt time; `source_key` deduplicates Fluent Bit file/offset retries.
- `lite_event_payload`: original raw record, canonical JSON, provenance and quality.
- `lite_event_index`: typed projection partitioned by event time; its composite `(event_ts,id)` primary key includes the hypertable partition key.
- `lite_gate_work`: durable independent gate queue, attempts, lease, next-available time and last error.
- `lite_source_identity`: best-effort source event identity deduplication.
- `lite_event_links`, `lite_signals`, `lite_findings`: correlation graph, event signals and current finding materialization.

## Verification

```bash
python3 tests/incident_lab/generate.py --list
python3 tests/incident_lab/generate.py --all --permutations 3 --shuffle-arrival \
  --duplicate-rate 0.05 --drop-optional-rate 0.10 \
  --output /tmp/observalog-incident-lab
```

The generator writes separate `raw/` and `ground-truth/` trees. Only raw input may be mounted into Fluent Bit. Ground truth is evaluator-only.

GitHub Actions runs:
- `.github/workflows/brain.yml` — release build, unit tests and Clippy.
- `.github/workflows/incident-lab.yml` — deterministic corpus generation and truth-isolation checks.
- `.github/workflows/lite-e2e.yml` — actual Fluent Bit → brain → TimescaleDB ingestion and gate-drain smoke test.

## Recovery and replay

See [replay-and-recovery.md](replay-and-recovery.md) for durable gate recovery, replay SQL, duplicate semantics and failure diagnosis.

## Design notes

- [Current legacy brain audit](current-brain-audit.md)
- [Gate responsibilities and invariants](gate-responsibilities.md)
- [Evidence-led reasoning model](reasoning-model.md)
