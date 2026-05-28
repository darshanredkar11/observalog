# observalog-brain

**AI triage engine for ObservaLog.**

The brain consumes structured logs from Kafka, stores them in TimescaleDB, and provides AI-powered triage via a WebSocket API. When a trace fails, the brain reconstructs the full call graph, detects sequence gaps, classifies the failure pattern, and invokes an LLM constrained to produce one of ten typed repair categories.

---

## Quick start

```bash
DATABASE_URL=postgres://observalog:password@timescaledb:5432/observalog \
KAFKA_BROKERS=kafka:9092 \
REDIS_URL=redis://valkey:6379 \
ANTHROPIC_API_KEY=sk-ant-... \
./observalog-brain
```

The brain applies its own schema DDL at startup — no manual migrations.

---

## Environment variables

| Variable             | Required | Default                  | Description |
|---------------------|----------|--------------------------|-------------|
| `DATABASE_URL`      | yes      | —                        | TimescaleDB connection string |
| `KAFKA_BROKERS`     | yes      | —                        | Comma-separated broker list |
| `KAFKA_TOPIC`       | no       | `observalog-logs`        | Log topic to consume |
| `KAFKA_GROUP_ID`    | no       | `observalog-brain`       | Consumer group ID |
| `REDIS_URL`         | no       | `redis://localhost:6379` | Valkey/Redis for gap detection |
| `ANTHROPIC_API_KEY` | yes      | —                        | LLM triage (claude-sonnet-4-6) |
| `LISTEN_ADDR`       | no       | `0.0.0.0:4000`           | WebSocket API bind address |
| `RUST_LOG`          | no       | `info`                   | Log level (info, debug, warn, error) |

---

## WebSocket API

Connect to `ws://<host>:4000/ws/triage`.

### Request

```json
{ "trace_id": "trc_7f2a1b9e4d" }
```

### Response

```json
{
  "trace_id": "trc_7f2a1b9e4d",
  "failure_class": "ExternalDependency",
  "repair_id": "RATE_LIMIT_BACKOFF",
  "confidence": 0.91,
  "summary": "Provider quota exceeded on doc-service outbound send. Retryable.",
  "call_graph": [
    { "service": 1, "span_id": "spn_001", "event": "auth.jwt.validated", "outcome": "success" },
    { "service": 2, "span_id": "spn_002", "event": "doc.document.queried", "outcome": "success" },
    { "service": 2, "span_id": "spn_003", "event": "provider.send.attempted", "outcome": "failure" },
    { "service": 3, "span_id": "spn_004", "event": "provider.send.rejected", "outcome": "failure" }
  ],
  "gaps": [],
  "cached": false
}
```

### Response fields

| Field            | Type    | Description |
|-----------------|---------|-------------|
| `trace_id`      | string  | Echo of the requested trace |
| `failure_class` | string  | See Failure Classes below |
| `repair_id`     | string  | See RepairId values below |
| `confidence`    | float   | 0.0–1.0 classification confidence |
| `summary`       | string  | LLM-generated human-readable diagnosis |
| `call_graph`    | array   | Ordered service/span/event nodes for the failing trace |
| `gaps`          | array   | Confirmed sequence gaps (empty = no dropped logs detected) |
| `cached`        | boolean | true = fingerprint cache hit, LLM not called |

---

## Failure classes

| Class               | Signal |
|--------------------|--------|
| `Isolated`         | Errors in one service only |
| `Cascading`        | Errors in multiple services |
| `ExternalDependency` | Errors at service 3 (provider) or service 0 (infra) |
| `Timeout`          | Duration anomaly without success |
| `GapDetected`      | Sequence gap confirmed after 30s grace window |
| `Partial`          | `outcome=partial` present in trace |
| `Unknown`          | Cannot classify from available signals |

Classification happens entirely on `log_index` typed columns — no `log_payload` access needed for 95% of classifications.

---

## RepairId values

The LLM is constrained to produce exactly one of:

| RepairId              | Automated? | Escalation? |
|----------------------|------------|-------------|
| `NETWORK_RETRY`      | yes        | no          |
| `RATE_LIMIT_BACKOFF` | yes        | no          |
| `VALIDATION_FIX`     | no         | no          |
| `DATABASE_FIX`       | no         | yes         |
| `AUTH_REFRESH`       | yes        | no          |
| `DEPENDENCY_FALLBACK`| yes        | no          |
| `CONFIG_FIX`         | no         | yes         |
| `RESOURCE_EXHAUSTED` | no         | yes         |
| `CONSISTENCY_FIX`    | no         | yes         |
| `UNKNOWN`            | no         | yes         |

`UNKNOWN` always escalates to a human. Automatable repairs can trigger fix playbooks directly.

---

## Triage pipeline

For each trace ID received via WebSocket:

```
1. Reconstruct trace chain (log_index, ts-bounded query)
2. Check fingerprint dedup (known_issues table — O(1) HASH index)
   → Cache hit: return cached RepairId immediately, no LLM call
3. Detect sequence gaps (Valkey 30s grace window)
4. Classify failure pattern (typed columns, no LLM)
5. Capture environment snapshot (Valkey gap key count, Kafka lag)
6. Retrieve code context (Semble version-pinned index — stub)
7. Call LLM (Anthropic claude-sonnet-4-6, RepairId constraint)
8. Cache result in known_issues by fingerprint
9. Return TriageResult via WebSocket
```

---

## Storage

### log_index (hot path)

~60 bytes per row. Contains every field the brain queries for chain reconstruction, gap detection, and failure classification. All indexes live here.

Hypertable partitioned by `ts`, 1-day chunks, 7-day retention. Always query with `ts BETWEEN $2 AND $3` when filtering by `trace_id`.

### log_payload (cold path)

Full JSONB payload plus `VECTOR(384)` embedding column. Fetched only for rows identified as interesting by `log_index` queries. The brain monitors JOIN operations and flags when more than 15 rows are fetched in a single triage operation.

### known_issues

Fingerprint dedup table. Keyed by `xxHash64(service_byte | event | error_code | ctx_primary_key)`. A cache hit returns the stored `RepairId` and `fix_summary` without an LLM call — eliminating ~95% of LLM calls for recurring errors.

---

## Gap detection

Each service maintains a per-request atomic `seq` counter (1, 2, 3, ...). The brain queries `(trace_id, service, seq)` tuples and detects any jump > 1 (mod 256).

**False positive prevention (Gap 5):** Out-of-order Kafka delivery is normal within 30 seconds. The brain sets a Valkey key `gap:<trace_id>:<service>:<seq_before>` with a 30s TTL on first detection. Only gaps that persist beyond the TTL are classified as real.

---

## Write order invariant

`log_payload` is always written before `log_index`. If the brain crashes between the two writes, an orphaned payload row exists but no index row points to it — the join is safe. The inverse (index row with no payload) would corrupt queries. Orphan cleanup runs at startup.

---

## Build

```bash
cargo build --release   # production binary (~9MB)
cargo test              # run all unit tests
```

---

## Source layout

```
src/
├── main.rs              # Entry point: DB, Redis, Kafka, WebSocket
├── db/
│   ├── schema.rs        # DDL (applied at startup)
│   └── queries.rs       # All TimescaleDB queries (runtime sqlx, ts-bounded)
├── ingest/
│   ├── wire_contract.rs # Byte offset constants (mirrors observalog-go/wire_contract.go)
│   ├── parser.rs        # Part A fixed-offset reads + Part B abbreviated JSON decode
│   ├── writer.rs        # log_payload then log_index write sequence
│   ├── writer_contract.rs # Write order and query constraint documentation
│   └── kafka.rs         # Kafka consumer, manual offset commit after DB write
├── triage/
│   ├── repair.rs        # RepairId enum (10 typed fix categories)
│   ├── chain.rs         # Trace chain reconstruction + call graph
│   ├── gap.rs           # Sequence gap detection with Valkey grace window
│   ├── classify.rs      # Failure pattern classification (typed columns only)
│   ├── dedup.rs         # Fingerprint dedup against known_issues
│   ├── environment.rs   # Environment snapshot (Valkey gap count, Kafka lag)
│   ├── context.rs       # Code context retrieval (Semble stub)
│   └── llm.rs           # Anthropic API call with RepairId constraint
└── ws.rs                # WebSocket handler and triage orchestration
```
