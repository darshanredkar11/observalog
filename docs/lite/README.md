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
- CI: Rust release build, unit tests and Clippy are green; incident generator checks are green. The final three-permutation Fluent Bit → TimescaleDB acceptance run is in progress and must pass before calling the branch release-ready.

The branch has been physically reduced to the lite runtime: the brain's compiled source tree contains only the lite modules, and the SDK/Kafka/Valkey/LLM paths are absent from the active repository runtime.

## Start locally

Requirements: Docker Compose v2 and a working Docker daemon.

```bash
cp .env.example .env
# Replace LITE_INGEST_TOKEN with a random secret of at least 24 bytes.
set -a; . ./.env; set +a
docker compose up -d --build
curl -fsS http://localhost:4000/health
```

Compose publishes TimescaleDB (5432), brain (4000) and Fluent Bit metrics (2020) on loopback only. The raw-ingest token is not transport encryption: keep the service on a trusted private network or place it behind TLS-terminating ingress before remote access.

The three runtime services are:

- `timescaledb` — durable raw journal, gate work, canonical payloads and event projections.
- `brain` — raw-ingest endpoint plus independent workers.
- `fluent-bit` — tails container logs and `./var/logs/*.log`, writes file offsets to its persistent state volume, buffers output to filesystem and retries HTTP delivery.

For local direct file ingestion, place logs under `./var/logs/`. The HTTP endpoint is `POST http://localhost:4000/ingest/raw`. Send JSON Lines, a JSON array/object, or plain text with `X-ObservaLog-Token: $LITE_INGEST_TOKEN`. Optional header: `X-ObservaLog-Source`. The brain refuses to start without a token of at least 24 bytes; use a secret manager outside local development.

`DATABASE_URL`, `LITE_INGEST_TOKEN`, `LISTEN_ADDR`, `DB_MAX_CONNECTIONS`, `LITE_SLOW_THRESHOLD_MS` and `LITE_MAX_INFLIGHT_GATE_WORK` configure the brain. The default in-flight durable work cap is 50,000; when it is reached the ingest endpoint returns HTTP 503 so Fluent Bit retries from its filesystem buffer. The default slow-operation threshold is 1000 ms.

## Processing stages

Each gate has its own durable work row in `lite_gate_work`. Workers claim work with row locking and `SKIP LOCKED`; outputs and downstream work are committed transactionally. Failed work is retried with backoff and marked `dead` after ten attempts. Per-gate supervisors restart unexpectedly exited workers, and an expired lease can be reclaimed after restart.

1. `normalize` — parse one raw record; preserve source text and source envelope; persist canonical event, provenance and hot typed projection.
2. `quality` — calculate mapping confidence and report missing or conflicting optional information.
3. `correlate` — create explicit links from trace, request, business-entity and parent/span evidence; reject contradictory identity joins.
4. `detect` — emit bounded event signals such as errors, slow operations, retry activity, error/severity mismatches and business invariant markers.
5. `reason` — group evidence and write deterministic hypotheses/findings with alternatives and uncertainty.

A missing trace ID is normal input, not a parse failure. The adapter does not invent trace IDs, span IDs, outcomes or durations. Timestamp fallback uses the durable raw-receipt time (not worker execution time) and marks it as inferred. The request body is capped at 16 MB and total in-flight durable work is capped at 50,000 by default; overload returns HTTP 503 so Fluent Bit retries from its filesystem buffer.

## Data layout

- `lite_raw_records`: durable raw text, source metadata and collector receipt time; `source_key` deduplicates Fluent Bit file/offset retries.
- `lite_event_payload`: original raw record, canonical JSON, provenance and quality.
- `lite_event_index`: typed projection partitioned by event time; its composite `(event_ts,id)` primary key includes the hypertable partition key.
- `lite_gate_work`: durable independent gate queue, attempts, lease, next-available time and last error.
- `lite_source_identity`: best-effort source event identity deduplication, scoped by collection file, service and instance so local IDs from separate streams do not collide.
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
