# ObservaLog — Technical White Paper

**Version 1.2**

---

## Abstract

Distributed systems produce logs. Observability platforms consume them. The gap between the two — the question of *what* those logs must contain and *how* they must be structured — is left entirely to individual developer judgment, independently, at every team, at every company. The result is systems that cannot be automatically analysed, cannot detect dropped messages, and cannot route failure patterns to typed fix strategies without expensive human triage. The problem compounds in polyglot stacks: three services in three languages produce three log shapes, and no shared contract exists.

ObservaLog is a five-component system that closes this gap across Go, Java, and Node.js. Three language libraries enforce structural contracts at emit time using identical wire format output. A Rust static analyser enforces positional contracts at CI time across all three languages. A Rust triage engine consumes the structured wire format, detects failures, and routes them to typed repair categories using an LLM with deterministic output constraints.

This paper describes the architecture, the cross-language contract design, the fourteen closed design decisions, and the ten gaps found and resolved during design.

---

## 1. The Structural Problem

When a distributed transaction fails, a complete triage requires:

1. The full journey — every log entry across every service, in sequence, for that trace
2. Gap detection — confirmation that no log entries were dropped
3. Root cause localisation — which service, which operation, at what timestamp
4. Fix classification — what category of repair resolves this class of failure

None of these are possible without contracts on the log data. A free-form `fmt.Println("payment failed: " + err.Error())` (or `logger.error("failed")`, or `console.error(err)`) tells a human something happened. It tells a machine nothing.

The conventional response is to add a structured logging library and write documentation asking developers to use it correctly. This does not work at scale. Documentation is not enforced. Fields are added inconsistently. Errors are passed as strings. Outcome fields appear without duration fields. The brain, given this data, produces hallucinated triage.

The problem amplifies across languages. A Go service emitting `zap.Error(...)` and a Java service emitting `log.error(...)` and a Node.js service emitting `winston.error(...)` produce three structurally different payloads even if all three developers followed their own team's conventions. There is no shared contract.

ObservaLog's answer is contracts enforced at three points:

- **Emit time** — three language libraries (Go, Java, Node.js) auto-inject structural fields, typed `Err` structs replace string errors, and the API makes correct usage the path of least resistance in every language
- **CI time** — the logscanner static analyser blocks merges that violate grammar, co-occurrence rules, and structural requirements
- **Storage time** — the two-table TimescaleDB split enforces the distinction between indexable typed columns and unstructured payload, making it physically impossible to run a slow full-scan query on untyped data

---

## 2. Wire Format

### 2.1 Design

Every log call, in every language, produces exactly two NDJSON lines:

```
A:{schema_v}|{trace_id}|{span_id}|{parent_span}|{seq}|{svc}|{lvl}|{out}|{ts_ms}
{abbreviated JSON payload}
```

**Part A** is a fixed-position 55-byte header. Every field occupies a fixed byte range. The Rust brain reads `trace_id` at bytes 2–15, `level` at byte 38, `ts_ms` at bytes 42–54 — without parsing JSON. This design was chosen because the brain needs these fields for 95% of its operations (journey fetch, gap detection, failure classification). Eliminating JSON parsing for the hot path is a meaningful performance choice at scale.

**Part B** is abbreviated JSON. Every field name is replaced with a one- or two-character abbreviation (`event` → `e`, `duration_ms` → `ms`, `error.retryable` → `rt`). The abbreviation dictionary is versioned in `observalog-go/dict.go`, `observalog-java/src/main/java/com/observalog/Dict.java`, and `observalog-node/src/dict.ts` — all three must contain identical mappings. After GZIP compression applied at the Fluent Bit layer, the size difference between this format and a binary protocol is under 10 bytes per entry. NDJSON was chosen over binary formats because universal tooling compatibility (`jq`, `grep`, Kibana, any log viewer) matters more than the marginal size difference.

### 2.2 Part A byte layout

```
Bytes  Field             Length
0      schema_version    1 char digit
2-15   trace_id          14 chars
17-23  span_id           7 chars
25-31  parent_span       7 chars ("-------" when absent)
33-34  seq               2 hex chars (00–ff, wraps at 255)
36     service           1 char (0=system 1=auth 2=doc 3=provider)
38     level             1 char (0=debug 1=info 2=warn 3=error)
40     outcome           1 char (0=none 1=success 2=failure 3=partial 4=pending)
42-54  ts_ms             13 chars (Unix milliseconds)
```

Total: 55 bytes. The `|` separators at positions 1, 16, 24, 32, 35, 37, 39, 41 are part of the fixed layout.

### 2.3 Schema versioning

Byte position 0 of Part A is the schema version (`1` currently). When the brain encounters an unknown schema version, it falls back to full JSON parsing. This design handles rolling deploys: a new brain version introducing a schema change can coexist with old producers still emitting v1 format.

### 2.4 Abbreviation dictionary

The abbreviation dictionary is the critical cross-language contract between all three libraries and the Rust brain. DictVersion 1 defines 18 entries:

| Full key | Abbreviation |
|----------|-------------|
| `event` | `e` |
| `message` | `m` |
| `duration_ms` | `ms` |
| `ctx` | `c` |
| `outcome` | `o` |
| `error` | `er` |
| `error.kind` | `ek` |
| `error.code` | `ec` |
| `error.message` | `em` |
| `error.retryable` | `rt` |
| `doc_id` | `di` |
| `topic` | `tp` |
| `partition` | `pt` |
| `offset` | `of` |
| `provider` | `pr` |
| `http_status` | `hs` |
| `user_id` | `ui` |
| `journey_stage` | `js` |

`ValidateNoDictCollisions()` / `Dict.validate()` is called at library init and panics on any collision — a fail-fast guard against silent data corruption.

---

## 3. Cross-Language Library Design

### 3.1 The polyglot challenge

The wire format is a language-agnostic contract, but the libraries that produce it must integrate naturally into each language's idioms. Go uses `context.Context` for request-scoped state. Java uses `ThreadLocal`. Node.js uses `AsyncLocalStorage`. The context propagation strategy differs; the wire output is identical.

### 3.2 Go library

Context is propagated through `context.Context` with typed keys. The module path is `github.com/darshanredkar11/observalog/observalog-go`. The HTTP middleware (`middleware.Middleware`) and Kafka consumer middleware (`middleware.KafkaConsumerMiddleware`) inject a `LogContext` struct at service entry. The `seq` field is a `*atomic.Uint32` pointer stored in context — it resets to 1 at service entry and auto-increments on every `Emit()` call.

Graceful degradation: when `context.Context` carries no `LogContext`, the library emits with `sys_<uuid>` trace IDs and `"system.background.untraced"` journey stage rather than panicking. Background jobs are legitimate emitters.

### 3.3 Java library

Context is propagated through `ThreadLocal<LogContext>`. The `HttpFilter` (implements `javax.servlet.Filter`) sets up a `LogContext` at the start of every HTTP request and calls `LogContext.clear()` in the `finally` block. The `seq` field is a per-`LogContext` `AtomicInteger` — never global, because a shared global counter would produce incorrect sequences across concurrent threads handling different requests.

Wire encoding uses `String.format(Locale.US, ...)` to prevent decimal separator issues on non-English locales, and explicit `\n` (not `%n`) to guarantee Unix line endings matching the wire format contract.

### 3.4 Node.js library

Context is propagated through `AsyncLocalStorage<LogContextData>` from Node.js `async_hooks`. The `httpMiddleware()` and `kafkaConsumerMiddleware()` functions call `storage.run(ctx, fn)`, which scopes the context to the async execution chain — including callbacks, Promises, and async/await chains — without requiring any explicit parameter threading.

The fingerprint is computed using `xxhashjs` with a signed int64 conversion to match the Go (`cespare/xxhash/v2`), Java (`net.openhft:zero-allocation-hashing`), and Rust (`xxhash_rust::xxh64`) implementations. The service code is passed as a raw byte (not an ASCII digit) to match the Go `string(uint8(code))` semantics.

### 3.5 Fingerprint cross-language parity

The fingerprint algorithm is identical across all four implementations:

```
fingerprint = xxHash64(service_code_byte | "|" | event | "|" | error_code | "|" | ctx_primary_key, seed=0)
```

The service code is always a raw byte value (e.g., `\x01` for auth, `\x02` for doc), not the ASCII character `'1'` or `'2'`. This is the most common cross-language mistake in xxHash implementations. All four test suites include a cross-language parity test using the same input bytes.

---

## 4. Storage Architecture

### 4.1 Two-table split

Log data is split across two tables:

**`log_index`** — typed columns only. ~60 bytes per row. Contains every field the brain queries for chain reconstruction, gap detection, and failure classification: `trace_id`, `span_id`, `service`, `level`, `outcome`, `seq`, `ts`, `fingerprint`, `payload_id`. All indexes are on this table.

**`log_payload`** — full JSONB document plus optional embedding vector. Fetched only for rows identified as interesting by `log_index` queries. The average triage operation touches 3–15 rows from `log_payload`; the brain never joins the two tables unnecessarily.

This split produces a 78% I/O reduction for the brain's most common operations, which only need the typed columns.

### 4.2 TimescaleDB

TimescaleDB was chosen over plain PostgreSQL and ClickHouse after benchmarking point lookups on filtered queries: 0.46ms vs 35–85ms on ClickHouse. The hypertable is partitioned by `ts` with 1-day chunks. A 7-day retention policy automatically drops old chunks for INFO success traces; ERROR and WARN traces are sampled and retained longer.

**Critical operational constraint (Gap 1):** Every query on `log_index` that filters by `trace_id` must include a `ts` range bound:

```sql
WHERE trace_id = $1
  AND ts BETWEEN $2 AND $3
```

Without the `ts` bound, TimescaleDB scans all 30 chunks (30 days of data) instead of 1. This constraint is documented in `writer_contract.rs` and enforced in every query in `db/queries.rs`.

### 4.3 Write order

`log_payload` is always written before `log_index`. If the brain crashes between the two writes, an orphaned payload row exists but no index row points to it. The inverse — an index row with no payload — would corrupt the join. Orphan cleanup handles stale payloads at startup. This invariant is documented in `writer_contract.rs`.

---

## 5. Sequence Integrity

Each service maintains a service-local atomic counter stored in request context (Go: `*atomic.Uint32` in `context.Context`; Java: `AtomicInteger` in `ThreadLocal<LogContext>`; Node.js: `{value: number}` in `AsyncLocalStorage`). It starts at 1 and increments on every `Emit()` call. The counter is local to one request handler or Kafka consumer — parallel handlers have independent namespaces.

The brain queries `(trace_id, service, seq)` tuples for a trace and detects any jump greater than 1 (mod 256, since seq is encoded as 2 hex chars). A gap means log entries were dropped somewhere between the emitting service and the brain — either in the channel buffer, in transit, or in Kafka.

**Gap 5 — False positive grace window:** Out-of-order delivery is normal within a 30-second window. The brain sets a Valkey key `gap:<trace_id>:<service>:<seq_before>` with a 30-second TTL the first time it detects a candidate gap. Only gaps that persist beyond the TTL are classified as real. This eliminates false positives from delayed Kafka delivery without requiring coordination between producers.

---

## 6. Fingerprint Deduplication

Every log call that contains an `error` field computes a fingerprint:

```
fingerprint = xxHash64(service_code_byte | event | error_code | ctx_primary_key, seed=0)
```

xxHash64 runs in approximately 8 nanoseconds in all languages. The fingerprint is stored as `BIGINT` in `log_index` with a partial HASH index (`WHERE fingerprint IS NOT NULL`).

When the brain receives a new error trace, it checks `known_issues` by fingerprint before invoking the LLM. A cache hit returns the cached `RepairId` and fix text immediately. This eliminates approximately 95% of LLM calls for systems with recurring errors — which is most production systems most of the time.

**Gap 3 — Semantic collisions:** The fingerprint input must include `ctx_primary_key`. Without it, two different documents failing with the same error code would share a fingerprint, causing the brain to return the cached fix for document A when document B's failure has a different root cause.

---

## 7. AI Triage

### 7.1 Failure classification

Before invoking the LLM, the brain classifies the failure pattern from the typed columns in `log_index`:

| Class | Signal |
|-------|--------|
| `Isolated` | Errors in one service only |
| `Cascading` | Errors in multiple services |
| `ExternalDependency` | Errors in service 3 (provider) or service 0 (infra) |
| `Timeout` | Duration anomaly without success |
| `GapDetected` | Sequence gap confirmed after grace window |
| `Partial` | `outcome=3` present in trace |

Classification happens entirely on `log_index` without touching `log_payload`. The brain classifies 95% of failures in a single O(1) TimescaleDB point lookup.

### 7.2 RepairId

The LLM is constrained to produce one of ten typed `RepairId` values:

```
NETWORK_RETRY | RATE_LIMIT_BACKOFF | VALIDATION_FIX | DATABASE_FIX
AUTH_REFRESH | DEPENDENCY_FALLBACK | CONFIG_FIX | RESOURCE_EXHAUSTED
CONSISTENCY_FIX | UNKNOWN
```

This constraint eliminates free-form prose that cannot be routed to fix playbooks. A `RATE_LIMIT_BACKOFF` repair can trigger an automated backoff configuration update. An `UNKNOWN` repair escalates to a human.

### 7.3 Context assembly

Before the LLM call, the brain assembles three context objects (Decision 12, borrowed from vercel-labs/zero):

1. **Call graph JSON** — the sequence of service/span/event nodes for the failing trace
2. **Environment snapshot** — Valkey gap key count, Kafka consumer lag, degraded services at triage trigger time
3. **Code context** — version-pinned semantic search against the exact git SHA recorded in the `version` log field, using Semble (Model2Vec + BM25 + RRF fusion)

The version-pinned code index is critical: the brain reads the actual code that was running at failure time, not the current HEAD. A race condition that was fixed yesterday is not a valid triage target for a failure that occurred yesterday.

---

## 8. Design Decisions

### Decision 1 — seq ownership: service-local counter, not global

`seq` is a per-request atomic counter in all three languages. A global shared counter would require coordination across concurrent request handlers. The brain detects gaps per service: `WHERE trace_id = X AND service = Y ORDER BY seq ASC`. Parallel requests on the same service have independent seq namespaces.

### Decision 2 — journey_stage: immutable from boundary middleware

Set once by HTTP or Kafka boundary middleware. Immutable for the lifetime of that trace within that service. Per-log overrides would create cardinality that degrades pattern matching — two traces with identical failure signatures but different journey stages would look like different patterns.

### Decision 3 — event naming: grammar not constants

`domain.object.action` — three segments, dot-separated, lowercase. 15-verb action vocabulary. The scanner validates the grammar. No constants file exists. Adding a new event requires no PR to a shared constants registry, only adherence to the grammar.

### Decision 4 — library API: explicit fields with auto-injection

Structural fields (`trace_id`, `span_id`, `seq`, `user_id`, `journey_stage`, `service`, `version`, `host`, `env`, `ts`) are auto-injected from context. Positional fields (`event`, `message`, `level`, `outcome`, `duration_ms`, `error`, `ctx`) are developer-supplied. This applies identically in Go, Java, and Node.js — the same API shape, different context propagation mechanisms.

### Decision 5 — missing context: graceful degradation with sys_ prefix

Missing `trace_id` → `sys_<uuid>`. Missing `user_id` → `"system"`. Missing `journey_stage` → `"system.background.untraced"`. The library never panics on missing context in any language. Background jobs and crons are legitimate emitters.

### Decision 6 — wire format: NDJSON on wire, JSONB in TimescaleDB, GZIP at Fluent Bit

The libraries write NDJSON to stdout. Fluent Bit applies GZIP before forwarding to Kafka. TimescaleDB stores as JSONB. The application has zero involvement in compression. After GZIP, the size difference between NDJSON and binary formats is under 10 bytes per entry.

### Decision 7 — compact two-part wire format

Part A is a 55-byte fixed-position header. Part B is abbreviated JSON. Schema version is byte 0. Unknown version falls back to full JSON. 70% wire size reduction. The Rust brain reads Part A at fixed byte offsets — zero JSON parsing for the fields it needs 95% of the time.

### Decision 8 — storage split: log_index + log_payload

Two tables. `log_index`: typed columns only, ~60 bytes/row, all indexes. `log_payload`: full JSONB plus embedding vector, fetched only for interesting rows. 78% I/O reduction for chain reconstruction, gap detection, and failure classification.

### Decision 9 — fingerprint: xxHash64

`fingerprint = xxHash64(service_code_byte | event | error_code | ctx_primary_key, seed=0)`. Computed at emit time (~8ns). Stored as BIGINT in `log_index`. Partial HASH index where `fingerprint IS NOT NULL`. Eliminates ~95% of LLM calls for repeated known errors. Identical implementation across Go, Java, Node.js, and Rust verified by cross-language parity tests.

### Decision 10 — storage engine: TimescaleDB

TimescaleDB over plain PostgreSQL and ClickHouse. `log_index` is a hypertable partitioned by `ts`, 1-day chunks. 0.46ms point lookup vs 35–85ms ClickHouse on filtered queries. Same SQL, same sqlx driver, same pgvector. 90%+ compression. Automatic chunk pruning.

### Decision 11 — code search: Semble semantic search

Semble (MinishLab/semble) integrated into the brain at `triage/context.rs`. Model2Vec + BM25 + RRF fusion. Runs on CPU in milliseconds. Version-pinned to the git SHA from the `version` log field. The brain reads the actual code that was running at failure time.

### Decision 12 — mechanisms from vercel-labs/zero

Four mechanisms adopted: RepairId enum (typed fix categories), call graph JSON (structural dependency context per triage finding), environment snapshot (service health at triage trigger time), version-pinned code indexing (Semble indexes the exact git SHA). These were proven in the Zero project and applied to the runtime triage domain.

### Decision 13 — file structure: repo-map-friendly

One concept per file (60–150 lines). Contract files adjacent to the code they govern. Every exported symbol is self-describing. Dependencies flow one direction. Optimised for repo-map patterns (Aider/tree-sitter/PageRank).

### Decision 14 — Java seq: per-LogContext AtomicInteger, not global

Java's `ThreadLocal` means one `LogContext` per thread. The `seq` counter is an `AtomicInteger` on `LogContext` — never a global static — because a global counter would interleave sequence numbers from concurrent request threads, producing sequence gaps that look like dropped logs.

---

## 9. Gap Registry

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
| 8 | Abbreviation dictionary collision | Versioned dict in all three libraries, scanner validates |
| 9 | Brain downtime loses logs | Fluent Bit → Kafka → brain (resume from committed offset) |
| 10 | JOIN scale at high payload fetch | Monitor threshold at 15 rows, fix at scale |

---

## 10. Operational Characteristics

### Throughput

The Go library's hot path is a non-blocking channel send (buffer default 10,000 entries). When the buffer is full, entries are dropped and counted by `DroppedLogCount()`. The Java library writes synchronously to the log sink (typically SLF4J/Logback which handles its own async buffer). The Node.js library uses a non-blocking async drain. In all cases, Fluent Bit tails container stdout and forwards ObservaLog lines to Kafka.

### Retention

`log_index` retains 7 days of data under the TimescaleDB retention policy. ERROR and WARN traces are sampled before this policy applies — the brain's outcome-based sampling logic ensures failure traces are retained beyond the default window. Chunk pruning is automatic.

### Resilience

Kafka provides durability for the transport layer. The brain commits Kafka offsets only after a successful TimescaleDB write — a brain crash recovers from the last committed offset with no log loss (Gap 9). The 30-second Valkey grace window provides resilience against transient delivery delays without coordinator overhead.

---

## 11. Future Work

- **Semble integration** — `triage/context.rs` is interface-complete; the Semble index population pipeline requires a CI step that runs `semble index --version <git-sha>` on each service build
- **Embedding pipeline** — `log_payload.embedding VECTOR(384)` is schema-ready; the embedding computation step (at write time or async) is not yet implemented
- **Outcome-based sampling** — the retention policy section documents the intent; the sampling logic before `insert_log_index` is not yet implemented
- **Orphan cleanup** — payloads written without a corresponding index row are detectable via `log_payload LEFT JOIN log_index`; cleanup runs at brain startup
- **MISSING_EXIT_LOG and MISSING_OUTCOME scanner rules** — the rule interfaces exist in `logscanner` for all three languages; full AST branch analysis requires a Go parser integration (tree-sitter or go/ast via subprocess) and TypeScript AST traversal
- **Python and Ruby libraries** — `observalog-python` and `observalog-ruby` would follow the same wire contract; Node.js serves as the reference implementation for dynamic-language libraries
- **logscanner Java and Node.js** — five static-analysis rules (`UNDECLARED_EVENT`, `UNSTRUCTURED_ERROR`, `MISSING_DURATION`, `RAW_PII_IN_LOG`, `UNDECLARED_ABBREVIATION`) are now implemented and tested for Java and Node.js/TypeScript alongside Go. Scanner dispatches on `.go`, `.java`, `.ts`, `.tsx`, `.js`, `.mjs`, `.cjs` extensions.

---

*ObservaLog v1.2 — wire format contracts frozen at schema version 1. Three-language library support added in v1.1. logscanner extended to Java and Node.js/TypeScript in v1.2.*
