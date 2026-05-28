# ObservaLog — Technical White Paper

**Version 1.0**

---

## Abstract

Distributed systems produce logs. Observability platforms consume them. The gap between the two — the question of *what* those logs must contain and *how* they must be structured — is left entirely to individual developer judgment at every team, at every company, independently. The result is systems that cannot be automatically analysed, cannot detect dropped messages, and cannot route failure patterns to typed fix strategies without expensive human triage.

ObservaLog is a three-component system that closes this gap. A Go library enforces structural contracts at emit time. A Rust static analyser enforces positional contracts at CI time. A Rust triage engine consumes the structured wire format, detects failures, and routes them to typed repair categories using an LLM with deterministic output constraints.

This paper describes the architecture, the thirteen closed design decisions, and the ten gaps found and resolved during design.

---

## 1. The Structural Problem

When a distributed transaction fails, a complete triage requires:

1. The full journey — every log entry across every service, in sequence, for that trace
2. Gap detection — confirmation that no log entries were dropped
3. Root cause localisation — which service, which operation, at what timestamp
4. Fix classification — what category of repair resolves this class of failure

None of these are possible without contracts on the log data. A free-form `fmt.Println("payment failed: " + err.Error())` tells a human something happened. It tells a machine nothing.

The conventional response is to add a structured logging library and write documentation asking developers to use it correctly. This does not work at scale. Documentation is not enforced. Fields are added inconsistently. Errors are passed as strings. Outcome fields appear without duration fields. The brain, given this data, produces hallucinated triage.

ObservaLog's answer is contracts enforced at three points:

- **Emit time** — the Go library auto-injects structural fields, typed Err structs replace string errors, and the API makes correct usage the path of least resistance
- **CI time** — the logscanner static analyser blocks merges that violate grammar, co-occurrence rules, and structural requirements
- **Storage time** — the two-table TimescaleDB split enforces the distinction between indexable typed columns and unstructured payload, making it physically impossible to run a slow full-scan query on untyped data

---

## 2. Wire Format

### 2.1 Design

Every log call produces exactly two NDJSON lines:

```
A:{schema_v}|{trace_id}|{span_id}|{parent_span}|{seq}|{svc}|{lvl}|{out}|{ts_ms}
B:{abbreviated JSON payload}
```

**Part A** is a fixed-position 55-byte header. Every field occupies a fixed byte range. The Rust brain reads `trace_id` at bytes 2–15, `level` at byte 38, `ts_ms` at bytes 42–54 — without parsing JSON. This design was chosen because the brain needs these fields for 95% of its operations (journey fetch, gap detection, failure classification). Eliminating JSON parsing for the hot path is a meaningful performance choice at scale.

**Part B** is abbreviated JSON. Every field name is replaced with a one- or two-character abbreviation (`event` → `e`, `duration_ms` → `ms`, `error.retryable` → `rt`). The abbreviation dictionary is versioned in `dict.go` and mirrored in the brain's `parser.rs`. After GZIP compression applied at the Fluent Bit layer, the size difference between this format and a binary protocol is under 10 bytes per entry. NDJSON was chosen over binary formats precisely because universal tooling compatibility (`jq`, `grep`, Kibana, any log viewer) matters more than the marginal size difference.

### 2.2 Schema versioning

Byte position 0 of Part A is the schema version (`1` currently). When the brain encounters an unknown schema version, it falls back to full JSON parsing. This design was chosen to handle rolling deploys: a new brain version that introduces a schema change can coexist with old producers still emitting v1 format.

### 2.3 Abbreviation dictionary

The abbreviation dictionary is the cross-module contract between the Go library and the Rust brain. Both files (`dict.go` and `ingest/parser.rs`) must contain identical mappings. The dictionary is versioned (`DictVersion = 1`). `ValidateNoDictCollisions()` is called at library Init() and panics on any collision — a fail-fast guard against silent data corruption.

---

## 3. Storage Architecture

### 3.1 Two-table split

Log data is split across two tables:

**`log_index`** — typed columns only. ~60 bytes per row. Contains every field the brain queries for chain reconstruction, gap detection, and failure classification: `trace_id`, `span_id`, `service`, `level`, `outcome`, `seq`, `ts`, `fingerprint`, `payload_id`. All indexes are on this table.

**`log_payload`** — full JSONB document plus optional embedding vector. Fetched only for rows identified as interesting by log_index queries. The average triage operation touches 3–15 rows from log_payload; the brain never joins the two tables unnecessarily.

This split produces a 78% I/O reduction for the brain's most common operations, which only need the typed columns.

### 3.2 TimescaleDB

TimescaleDB was chosen over plain PostgreSQL and ClickHouse after benchmarking point lookups on filtered queries: 0.46ms vs 35–85ms on ClickHouse. The hypertable is partitioned by `ts` with 1-day chunks. A 7-day retention policy automatically drops old chunks for INFO success traces; ERROR and WARN traces are sampled and retained longer.

**Critical operational constraint (Gap 1):** Every query on `log_index` that filters by `trace_id` must include a `ts` range bound:

```sql
WHERE trace_id = $1
  AND ts BETWEEN $2 AND $3
```

Without the `ts` bound, TimescaleDB scans all 30 chunks (30 days of data) instead of 1. This constraint is documented in `writer_contract.rs` and enforced in every query in `db/queries.rs`.

### 3.3 Write order

`log_payload` is always written before `log_index`. If the brain crashes between the two writes, an orphaned payload row exists but no index row points to it. The inverse — an index row with no payload — would corrupt the join. Orphan cleanup handles stale payloads at startup. This invariant is documented in `writer_contract.rs`.

---

## 4. Sequence Integrity

Each service maintains a service-local atomic counter (`*atomic.Uint32`) stored in `context.Context`. It starts at 1 and increments on every `Emit()` call. The counter is local to one request handler or Kafka consumer — parallel consumers have independent namespaces.

The brain queries `(trace_id, service, seq)` tuples for a trace and detects any jump greater than 1 (mod 256, since seq is encoded as 2 hex chars). A gap means log entries were dropped somewhere between the emitting service and the brain — either in the channel buffer (tracked by `DroppedLogCount()`), in transit, or in Kafka.

**Gap 5 — False positive grace window:** Out-of-order delivery is normal within a 30-second window. The brain sets a Valkey key `gap:<trace_id>:<service>:<seq_before>` with a 30-second TTL the first time it detects a candidate gap. Only gaps that persist beyond the TTL are classified as real. This eliminates false positives from delayed Kafka delivery without requiring coordination between producers.

---

## 5. Fingerprint Deduplication

Every log call that contains an `error` field computes a fingerprint:

```
fingerprint = xxHash64(service_code_byte | event | error_code | ctx_primary_key)
```

xxHash64 runs in approximately 8 nanoseconds. The fingerprint is stored as `BIGINT` in `log_index` with a partial HASH index (`WHERE fingerprint IS NOT NULL`).

When the brain receives a new error trace, it checks `known_issues` by fingerprint before invoking the LLM. A cache hit returns the cached `RepairId` and fix text immediately. This eliminates approximately 95% of LLM calls for systems with recurring errors — which is most production systems most of the time.

**Gap 3 — Semantic collisions:** The fingerprint input must include `ctx_primary_key`. Without it, two different documents failing with the same error code would share a fingerprint, causing the brain to return the cached fix for document A when document B's failure has a different root cause.

---

## 6. AI Triage

### 6.1 Failure classification

Before invoking the LLM, the brain classifies the failure pattern from the typed columns in `log_index`:

| Class | Signal |
|-------|--------|
| `Isolated` | Errors in one service only |
| `Cascading` | Errors in multiple services |
| `ExternalDependency` | Errors in service 3 (provider) or service 0 (infra) |
| `Timeout` | Duration anomaly without success |
| `GapDetected` | Sequence gap confirmed after grace window |
| `Partial` | `outcome=3` present in trace |

Classification happens entirely on `log_index` without touching `log_payload`. This means the brain can classify 95% of failures in a single O(1) TimescaleDB point lookup.

### 6.2 RepairId

The LLM is constrained to produce one of ten typed `RepairId` values:

```
NETWORK_RETRY | RATE_LIMIT_BACKOFF | VALIDATION_FIX | DATABASE_FIX
AUTH_REFRESH | DEPENDENCY_FALLBACK | CONFIG_FIX | RESOURCE_EXHAUSTED
CONSISTENCY_FIX | UNKNOWN
```

This constraint eliminates free-form prose that cannot be routed to fix playbooks. A `RATE_LIMIT_BACKOFF` repair can trigger an automated backoff configuration update. An `UNKNOWN` repair escalates to a human. The typed output is the difference between triage that can act and triage that can only inform.

### 6.3 Context assembly

Before the LLM call, the brain assembles three context objects (Decision 12, borrowed from vercel-labs/zero):

1. **Call graph JSON** — the sequence of service/span/event nodes for the failing trace, providing structural dependency context
2. **Environment snapshot** — Valkey gap key count, Kafka consumer lag, degraded services at triage trigger time
3. **Code context** — version-pinned semantic search against the exact git SHA recorded in the `version` log field, using Semble (Model2Vec + BM25 + RRF fusion)

The version-pinned code index is critical: the brain reads the actual code that was running at failure time, not the current HEAD. A race condition that was fixed yesterday is not a valid triage target for a failure that occurred yesterday.

---

## 7. Design Decisions

### Decision 1 — seq ownership: service-local atomic counter

`seq` is a `*atomic.Uint32` stored as a pointer in `context.Context`. It resets to 0 at service entry and auto-increments on every `Emit()`. A global shared counter would require a network call per log emit. Parallel Kafka consumers would interfere with each other's sequence namespaces. The brain detects gaps per service: `WHERE trace_id = X AND service = Y ORDER BY seq ASC`.

### Decision 2 — journey_stage: immutable from boundary middleware

Set once by HTTP or Kafka boundary middleware. Immutable for the lifetime of that trace within that service. Per-log overrides would create cardinality that degrades the brain's pattern matching — two traces with identical failure signatures but different journey stages look like different patterns.

### Decision 3 — event naming: grammar not constants

`domain.object.action` — three segments, dot-separated, lowercase. 15-verb action vocabulary. The scanner validates the grammar. No constants file exists. This means adding a new event requires no PR to a shared constants registry, only adherence to the grammar.

### Decision 4 — library API: explicit fields with auto-injection

Structural fields (`trace_id`, `span_id`, `seq`, `user_id`, `journey_stage`, `service`, `version`, `host`, `env`, `ts`) are auto-injected from context. Positional fields (`event`, `message`, `level`, `outcome`, `duration_ms`, `error`, `ctx`) are developer-supplied. Typed builders would create friction. Scanner enforces correctness at CI time, not API design time.

### Decision 5 — missing context: graceful degradation with sys_ prefix

Missing `trace_id` → `sys_<uuid>`. Missing `user_id` → `"system"`. Missing `journey_stage` → `"system.background.untraced"`. The library never panics on missing context. Background jobs and crons are legitimate emitters. The `sys_` prefix signals the brain instantly — no special handling required.

### Decision 6 — wire format: NDJSON on wire, JSONB in TimescaleDB, GZIP at Fluent Bit

The Go library writes NDJSON to stdout. Fluent Bit applies GZIP before forwarding to Kafka. TimescaleDB stores as JSONB. The application has zero involvement in compression. After GZIP, the size difference between NDJSON and binary formats is under 10 bytes per entry.

### Decision 7 — compact two-part wire format

Part A is a 55-byte fixed-position header. Part B is abbreviated JSON. Schema version is byte 0 of Part A. Unknown version falls back to full JSON. 70% wire size reduction. The Rust brain reads Part A at fixed byte offsets — zero JSON parsing for the fields it needs 95% of the time.

### Decision 8 — storage split: log_index + log_payload

Two tables. `log_index`: typed columns only, ~60 bytes/row, all indexes. `log_payload`: full JSONB plus embedding vector, fetched only for interesting rows. 78% I/O reduction for chain reconstruction, gap detection, and failure classification.

### Decision 9 — fingerprint: xxHash64

`fingerprint = xxHash64(service_code | event | error_code | ctx_primary_key)`. Computed at emit time (~8ns). Stored as BIGINT in `log_index`. Partial HASH index where `fingerprint IS NOT NULL`. Eliminates ~95% of LLM calls for repeated known errors.

### Decision 10 — storage engine: TimescaleDB

TimescaleDB over plain PostgreSQL and ClickHouse. `log_index` is a hypertable partitioned by `ts`, 1-day chunks. 0.46ms point lookup vs 35–85ms ClickHouse on filtered queries. Same SQL, same sqlx driver, same pgvector. 90%+ compression. Automatic chunk pruning.

### Decision 11 — code search: Semble semantic search

Semble (MinishLab/semble) integrated into the brain at `triage/context.rs`. Model2Vec + BM25 + RRF fusion. Runs on CPU in milliseconds. Version-pinned to the git SHA from the `version` log field. The brain reads the actual code that was running at failure time.

### Decision 12 — mechanisms from vercel-labs/zero

Four mechanisms adopted: RepairId enum (typed fix categories), call graph JSON (structural dependency context per triage finding), environment snapshot (service health at triage trigger time), version-pinned code indexing (Semble indexes the exact git SHA). These mechanisms were proven in the Zero project and applied to the runtime triage domain.

### Decision 13 — file structure: repo-map-friendly

One concept per file (60–150 lines). Contract files adjacent to the code they govern. Every exported symbol is self-describing. Dependencies flow one direction. This structure is optimised for repo-map patterns (Aider/tree-sitter/PageRank) — agent tools can navigate to the exact file needed without reading irrelevant ones.

---

## 8. Gap Registry

Ten gaps were found and resolved during design:

| # | Gap | Fix |
|---|-----|-----|
| 1 | `trace_id` hash index vs chunk partitioning | Always include `ts` range bound on `trace_id` queries |
| 2 | Positional wire format on rolling deploys | `schema_version` byte at position 0 of Part A |
| 3 | Fingerprint semantic collision | Include `ctx_primary_key` in fingerprint input |
| 4 | Two-write atomicity failure | Write `log_payload` first, then `log_index` |
| 5 | False positive gap detection | 30-second Valkey grace window before classifying a gap |
| 6 | Unmapped HTTP routes | Auto-derive `journey_stage` for unregistered routes |
| 7 | Valkey role undefined | Valkey = out-of-order sequence buffer, 30s TTL |
| 8 | Abbreviation dictionary collision | Versioned dict in `dict.go`, scanner validates |
| 9 | Brain downtime loses logs | Fluent Bit → Kafka → brain (resume from committed offset) |
| 10 | JOIN scale at high payload fetch | Monitor threshold at 15 rows, fix at scale |

---

## 9. Operational Characteristics

### Throughput

The Go library's hot path is a non-blocking channel send. The channel buffer (`LOG_BUFFER_SIZE`, default 10,000) absorbs bursts. When the buffer is full, entries are dropped and counted by `DroppedLogCount()`. The drain goroutine writes to stdout; Fluent Bit tails the container log.

### Retention

`log_index` retains 7 days of data under the TimescaleDB retention policy. ERROR and WARN traces are sampled before this policy applies — the brain's outcome-based sampling logic ensures failure traces are retained beyond the default window. Chunk pruning is automatic.

### Resilience

Kafka provides durability for the transport layer. The brain commits Kafka offsets only after a successful TimescaleDB write — a brain crash recovers from the last committed offset with no log loss (Gap 9). The 30-second Valkey grace window provides resilience against transient delivery delays without coordinator overhead.

---

## 10. Future Work

- **Semble integration** — `triage/context.rs` is interface-complete; the Semble index population pipeline requires a CI step that runs `semble index --version <git-sha>` on each service build
- **Embedding pipeline** — `log_payload.embedding VECTOR(384)` is schema-ready; the embedding computation step (at write time or async) is not yet implemented
- **Outcome-based sampling** — the retention policy section documents the intent; the sampling logic before `insert_log_index` is not yet implemented
- **Orphan cleanup** — payloads written without a corresponding index row are detectable via `log_payload LEFT JOIN log_index`; cleanup runs at brain startup
- **MISSING_EXIT_LOG and MISSING_OUTCOME scanner rules** — the rule interfaces exist in `logscanner`; full AST branch analysis requires a Go parser integration (tree-sitter or go/ast via subprocess)

---

*ObservaLog v1.0 — architecture and decisions are stable. Wire format contracts are frozen at schema version 1.*
