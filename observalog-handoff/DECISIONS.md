# ObservaLog — Decision Register

All decisions are CLOSED. Do not reopen without a new structured debate.
Each decision has a one-line rationale. If you disagree with a decision,
document it as a new debate — do not silently work around it.

---

## Decision 1 — seq ownership: service-local atomic counter

**Verdict:** `seq` is a `*atomic.Uint32` stored as a pointer in `context.Context`.
Resets to 0 at service entry. Auto-incremented by library on every `Emit()` call.

**Rationale:** Global shared state requires a network call per log emit. Parallel Kafka
consumers have independent seq namespaces. Gap detection operates per service:
`WHERE trace_id = X AND service = Y ORDER BY seq ASC`.

**Never:** Store seq in Valkey, a global var, or behind a mutex.

---

## Decision 2 — journey_stage: immutable from boundary middleware

**Verdict:** Set once by HTTP or Kafka middleware. Immutable for the lifetime
of that trace within that service. The API does not expose a parameter to override it.

**Rationale:** Per-log overrides create cardinality that degrades brain pattern matching.
Simplicity over granularity.

**Never:** Expose journey_stage as a parameter in Info(), Warn(), Error(), Debug().

---

## Decision 3 — event naming: grammar not constants

**Verdict:** `domain.object.action` — three segments, dot-separated, lowercase.
domain = finite set. object = free within domain. action = 15-verb vocabulary.
Scanner validates structure. No constants file.

**Rationale:** No PR required to add new events. 15 verbs cover all cases.
`domain.object.action` is self-documenting and queryable with LIKE patterns.

**Never:** Create an events constants file. Validate grammar at runtime in the library.

---

## Decision 4 — library API shape: explicit with auto-injection

**Verdict:** Structural fields (trace_id, span_id, seq, user_id, journey_stage,
service, version, host, env, ts) are auto-injected from context. Positional fields
(event, message, level, outcome, duration_ms, error, ctx) are developer-supplied.

**Rationale:** AI-assisted development (Kiro, Claude Code) writes natural Go.
Typed builders create friction. Scanner enforces correctness at CI time.

---

## Decision 5 — missing context: graceful degradation with sys_ prefix

**Verdict:** Missing trace_id → `sys_<uuidv4>`. Missing user_id → `"system"`.
Missing journey_stage → `"system.background.untraced"`. Library never panics
on missing context.

**Rationale:** Background jobs and crons are legitimate. sys_ prefix signals
the brain instantly — no special logic needed.

**Never:** Panic or return error when ambient fields are missing from context.

---

## Decision 6 — wire format: NDJSON on wire, JSONB in TimescaleDB, GZIP at Fluent Bit

**Verdict:** Go library writes NDJSON to stdout. Fluent Bit applies GZIP before
forwarding to Kafka. TimescaleDB stores as JSONB. Application has zero involvement
in compression.

**Rationale:** After GZIP, the size difference between NDJSON and binary formats
is under 10 bytes per entry. NDJSON has universal tooling compatibility.

---

## Decision 7 — compact two-part wire format

**Verdict:** Every log emits two lines:
- `A:{schema_v}|{trace_id}|{span_id}|{parent_span}|{seq}|{svc}|{lvl}|{out}|{ts_ms}`
- `B:{abbreviated JSON payload}`

Schema version is byte 0 of Part A. Unknown version → fall back to full JSON.
Abbreviated key dictionary is versioned in `dict.go`.

**Rationale:** 70% wire size reduction. Rust brain reads Part A at fixed byte
offsets — zero JSON parsing for the fields it needs 95% of the time.

**Critical:** Part A byte positions are a contract between Go library and Rust brain.
Both `wire_contract.go` and `wire_contract.rs` must stay in sync.

---

## Decision 8 — storage split: log_index + log_payload

**Verdict:** Two tables in TimescaleDB.
- `log_index`: typed columns only, ~60 bytes/row, all indexes
- `log_payload`: full JSONB + embedding vector, fetched only for interesting rows

**Rationale:** 78% I/O reduction. Brain queries log_index for chain reconstruction,
gap detection, and failure classification without touching JSONB.

---

## Decision 9 — fingerprint: xxHash64

**Verdict:** `fingerprint = xxHash64(service_code | event | error_code | ctx_primary_key)`
Computed at emit time (~8ns). Stored as BIGINT in log_index.
Partial hash index where `fingerprint IS NOT NULL`.

**Rationale:** O(1) exact-match deduplication. Eliminates ~95% of LLM calls for
repeated known errors. ctx_primary_key prevents semantic collisions across different
domain objects with same event+error_code.

**Never:** Use SHA256 or any cryptographic hash. Never omit ctx_primary_key.

---

## Decision 10 — storage engine: TimescaleDB

**Verdict:** TimescaleDB over plain Postgres and ClickHouse.
log_index is a hypertable partitioned by ts, 1-day chunks.

**Rationale:** 0.46ms point lookup vs 35-85ms ClickHouse on filtered queries.
Same SQL, same sqlx driver, same pgvector. 90%+ compression. Automatic chunk pruning.

**Critical (Gap 1):** Brain must always include ts range bound on trace_id queries.
`WHERE trace_id = X AND ts BETWEEN now() - interval '2 hours' AND now() + interval '5 minutes'`
Without the ts bound, TimescaleDB scans all 30 chunks instead of 1.

---

## Decision 11 — code search: Semble semantic search

**Verdict:** Semble (MinishLab/semble) integrated into Rust brain at
`triage/context.rs`. Model2Vec + BM25 + RRF fusion. Runs on CPU in milliseconds.
Version-pinned to the git SHA from the `version` log field.

**Rationale:** Brain reads the actual code that was running at failure time.
Not grep — semantic search across function boundaries.

---

## Decision 12 — mechanisms borrowed from Zero language

**Verdict:** Four mechanisms adopted from vercel-labs/zero:
1. RepairId enum — typed fix categories replacing prose-only LLM output
2. Call graph JSON — structural dependency context per triage finding
3. Environment snapshot — service health at triage trigger time
4. Version-pinned code indexing — Semble indexes exact git SHA from version field

**Rationale:** Zero proved these mechanisms work. Applied to runtime triage domain.

---

## Decision 13 — file structure: repo-map-friendly

**Verdict:** One concept per file (60-150 lines). Contract files adjacent to code
they govern. Every exported symbol is self-describing. Dependencies flow one direction.
AGENTS.md max 30 lines, human-written only.

**Rationale:** Repo-map pattern (Aider/tree-sitter/PageRank) surfaces function
signatures without reading file bodies. Agent navigates to exact file needed without
reading irrelevant ones. 4x fewer tokens than embedding-based context loading.

---

## Gap Registry — ten gaps found and resolved

| # | Gap | Fix |
|---|-----|-----|
| 1 | trace_id hash index vs chunk partitioning | Always include ts bound in trace_id queries |
| 2 | Positional wire format on rolling deploys | schema_version byte at position 0 of Part A |
| 3 | Fingerprint semantic collision | Include ctx_primary_key in fingerprint input |
| 4 | Two-write atomicity failure | Write log_payload first, then log_index |
| 5 | False positive gap detection | 30-second Valkey grace window before classifying gap |
| 6 | Unmapped HTTP routes | Auto-derive journey_stage for unregistered routes |
| 7 | Valkey role undefined | Valkey = out-of-order sequence buffer, 30s TTL |
| 8 | Abbreviation dictionary collision | Versioned dict in dict.go, scanner validates |
| 9 | Brain downtime loses logs | Fluent Bit → Kafka → brain (resume from offset) |
| 10 | JOIN scale at high payload fetch | Monitor threshold 15 rows, fix at scale |
