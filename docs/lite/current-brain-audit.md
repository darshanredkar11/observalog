# Lite ObservaLog — Existing System Audit

Scope: the parent repository `darshanredkar11/observalog`, branch `main`. This is a code-reading baseline, not a claim that every behavior has been integration-tested. The independent `observalog-brain` repository is out of scope.

## Existing path

The documented path is:

`application SDK stdout → Fluent Bit → Kafka → Rust brain → TimescaleDB`

The brain then exposes request-triggered triage through WebSocket. Triage is not currently an always-running event-analysis stream: the WebSocket handler reconstructs a trace and orchestrates classification, deduplication, environment/context retrieval and LLM triage after receiving a request.

## What exists today

| Area | Existing implementation | Actual constraints |
|---|---|---|
| Collection | Fluent Bit tails Docker JSON log files | Docker-specific input path and parser |
| Filtering | Keeps records whose `log` starts with `A:` or `{` | Assumes the legacy two-line wire representation |
| Transport | Fluent Bit Kafka output, GZIP | Kafka is a required intermediate in the current compose stack |
| Decode | Rust fixed-offset Part A + abbreviated Part B JSON | Requires exact legacy wire format and required `e` / `m` keys |
| Persistence | `log_payload` JSONB then `log_index` typed columns | Two independent inserts; no event-level idempotency key |
| Time-series | `log_index` hypertable, one-day chunks | Trace queries must include time bounds |
| Correlation | Trace ID query over a fixed time window | Not a general multi-signal or business-entity correlation engine |
| Sequence gaps | Per-service sequence comparison and Valkey grace marker | Depends on SDK sequence semantics and has edge cases |
| Classification | A small fixed priority rule set | Does not infer code/config/resource/business causes |
| Dedup | xxHash64 fingerprint lookup | Exact key only; first matching error fingerprint is used in request triage |
| Environment | Valkey gap-key count; other fields mostly empty | Kafka lag, error-rate and degraded-service information are placeholders |
| Code context | Semble interface | Retrieval implementation returns no snippets |
| LLM | Anthropic request for a typed repair category | Prompt receives classification, environment, call graph and code context; current call graph event labels are placeholders |
| Output | WebSocket response | A client request is needed to trigger triage |

## Confirmed implementation mismatches and defects to test

### 1. Fluent Bit record boundary does not match the brain consumer

The Go emitter calls `SendToChannel` separately for Part A and Part B. Those are separate stdout writes. Fluent Bit tails Docker log records and forwards each record matching its grep filter. The Rust Kafka consumer, however, expects each Kafka message to contain two lines (Part A followed by Part B).

There is no multiline aggregation in the supplied Fluent Bit configuration. Therefore, the configured producer/collector/consumer boundary does not match the consumer's message contract. Verify with an end-to-end run, but the code/configuration show the mismatch.

### 2. Fixed contract assumptions are embedded in storage

The parser requires Part A and Part B, rejects unknown Part A schema versions, and requires abbreviated event and message keys. The hot index also requires non-null `trace_id`, `span_id`, `service`, `level` and `seq`. This is incompatible with arbitrary third-party logs unless an adapter and a permissive canonical model are introduced.

### 3. The existing classifier has unreachable or unused behavior

- Partial outcomes are filtered as outcome `2` before code checks for outcome `3`; that partial branch cannot match.
- `Timeout` exists in the enum but the shown classifier never returns it.
- `TraceChain::reconstruct` initializes `has_gaps: false`; the WebSocket triage flow does not call `gap::detect`, so gap-based classification is not connected to that path.
- Multiple services with ERROR-level rows are classified as cascading without checking parent/span/dependency causality.
- External dependency is hard-coded to service codes `0` and `3`.

### 4. Current gap detection needs adversarial validation

The gap function casts stored sequence values to `u8`, sorts numerically, then applies wrapping arithmetic. Numeric sorting mishandles a sequence crossing 255 → 0. It only detects gaps between observed values, not missing first/last events. In `detect`, the first observation of a gap (when the key does not exist) is added to `confirmed_gaps` immediately; a pre-existing key is treated as still within the grace period. That is opposite to the stated “wait 30 seconds before confirmation” behavior.

### 5. Poison records can prevent partition progress

The Kafka consumer manually commits only after a successful database write. Non-UTF8 messages, empty messages, missing Part B, and parser failures are skipped without commit or dead-letter handling. Such a record can be repeatedly delivered after restart and can hold a partition at the same offset.

### 6. Persistence replay can duplicate logical events

Payload and index inserts are separate, not wrapped in one transaction, and no stable event identity is stored as a uniqueness constraint. A write followed by a crash before Kafka offset commit can be replayed and inserted again. The payload-first ordering prevents an index from pointing to a payload that was never written, but it does not provide atomicity or idempotency.

### 7. Operational and diagnosis metadata is incomplete

The current hot index does not contain service name, instance/host, service version, environment, region, tenant, source format, source event identity or field provenance. The parser/writer do not promote those fields from payload into typed columns. This prevents robust version/config/instance/tenant comparisons without expensive payload reads.

### 8. Retention description and implementation disagree

The DDL installs a seven-day retention policy on the whole `log_index` hypertable. Comments describe retaining WARN/ERROR longer, but the DDL shown does not implement outcome-based retention or sampling.

### 9. LLM evidence is currently weak

The call graph uses `log_index.<id>` as its event name instead of retrieving the real event. The prompt builder does not use its `TraceChain` argument directly. The code-context retrieval is a stub. Therefore, a plausible-sounding LLM answer should not be mistaken for an evidence-backed diagnosis.

### 10. Service identity is too narrow

The current Go SDK maps only auth, doc and provider to distinct numeric codes; other service names map to system. The new model must support arbitrary service names and instances without mapping all unknown services into one identity.

## TimescaleDB assets worth preserving and validating

- Separate hot typed index from full JSONB payload.
- Hypertable on event timestamp with one-day chunks.
- BRIN timestamp index and targeted trace/user/fingerprint indexes.
- Payload JSONB storage for raw and normalized context.
- Fingerprint lookup as an optimization, not as the identity of an incident.
- Time-bounded trace queries for chunk pruning.

Before changing schema, validate the current hypertable DDL against the exact TimescaleDB image. In particular, verify unique/primary-key constraints on the time-partitioned table; do not assume the startup DDL is valid solely because it exists in source.

## What must not be carried forward as assumptions

1. A source must emit the ObservaLog contract.
2. Every log has a trace ID, span ID, outcome, duration or error object.
3. Service identity can be represented by a small fixed integer dictionary.
4. Time proximity means causality.
5. Multiple services with errors automatically means cascading failure.
6. A sequence gap proves a dropped event.
7. A repeated fingerprint is always the same incident or has the same current root cause.
8. LLM output is a diagnosis without supporting evidence.

The adapter must absorb source diversity. The brain must explicitly represent unknown, missing, conflicting and inferred values.
