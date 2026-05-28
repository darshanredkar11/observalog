# ObservaLog

**A structured log contract system for distributed Go microservices.**

ObservaLog enforces *what* every log must contain, *how* it travels on the wire, and *what* your AI triage engine does when something fails — all three as first-class, versioned contracts.

---

## The problem

Distributed systems fail silently. A JWT expires in `auth-service`, triggers a cascade through `doc-service` and `provider-service`, and by the time an engineer investigates, they are staring at 40,000 lines of unstructured text across three Kibana tabs, manually correlating timestamps.

The root cause is not a missing alert. It is missing *structure*. Logs that were never designed to be read by a machine cannot be triaged by one.

ObservaLog fixes this at the source.

---

## How it works

```
Go service                    Fluent Bit          Kafka          observalog-brain
──────────────────────────    ──────────────      ──────         ─────────────────────────────
observalog.Info(ctx, ...)  →  GZIP compress   →  topic    →     parse Part A (fixed offsets)
observalog.Error(ctx, ...) →  forward         →           →     parse Part B (abbreviated JSON)
                                                               ↓
                                                         TimescaleDB
                                                         log_index  (hot — 60 bytes/row)
                                                         log_payload (cold — full JSONB)
                                                               ↓
                                                     fingerprint dedup (xxHash64)
                                                     gap detection (seq counter)
                                                     failure classification
                                                     LLM triage → RepairId
                                                     WebSocket → dashboard
```

Every log emits two lines:

```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
{"e":"provider.send.rejected","m":"Provider rejected document send","er":{"ek":"RateLimitExceeded","ec":"PROVIDER_QUOTA_EXCEEDED","rt":true},"c":{"di":"doc123","hs":429},"o":"failure","ms":87}
```

**Part A** is a 55-byte fixed-position header. The Rust brain reads `trace_id`, `span_id`, `service`, `level`, `outcome`, and `ts_ms` at hard-coded byte offsets — zero JSON parsing for the fields it needs 95% of the time.

**Part B** is abbreviated JSON. `event` → `e`, `duration_ms` → `ms`, `error.kind` → `ek`. After GZIP the size difference vs. binary formats is under 10 bytes per entry.

---

## Components

| Component | Language | Role |
|-----------|----------|------|
| [`observalog-go`](observalog-go/) | Go | Library imported by product services. Emits two-line NDJSON to stdout. |
| [`logscanner`](logscanner/) | Rust | CI static analyzer. Enforces log contracts at merge time. Exit 1 = block. |
| [`observalog-brain`](observalog-brain/) | Rust | AI triage engine. Kafka → TimescaleDB → LLM → WebSocket. |

---

## Quick start

### 0. Start the infrastructure

```bash
cp .env.example .env          # add your ANTHROPIC_API_KEY
docker compose up -d
```

Starts: TimescaleDB · Kafka (KRaft, no Zookeeper) · Valkey · Fluent Bit · observalog-brain.

The brain applies its own schema at startup. Fluent Bit tails all container stdout and forwards ObservaLog lines to Kafka automatically.

WebSocket API available at `ws://localhost:4000/ws` once the brain is healthy.

---

### 1. Add the Go library

```go
import log "github.com/darshanredkar11/observalog-go"

func main() {
    log.Init(log.ConfigFromEnv("v1.2.3-abc123def"))
    defer log.Shutdown()
}
```

### 2. Wrap your HTTP handler

```go
handler := middleware.Middleware(yourRouter)
http.ListenAndServe(":8080", handler)
```

### 3. Emit logs

```go
// Informational
log.Info(ctx, "doc.storage.saved", "Document written to storage", log.F{
    "doc_id":  docID,
    "bytes":   n,
    "backend": "postgres",
})

// Decision point
log.Info(ctx, "auth.permission.checked", "Permission granted", log.F{
    "doc_id":      docID,
    "outcome":     log.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})

// Failure
log.Error(ctx, "provider.send.rejected", "Provider rejected document send", log.F{
    "doc_id":      docID,
    "outcome":     log.Failure,
    "duration_ms": time.Since(start).Milliseconds(),
    "error": &log.Err{
        Kind:      "RateLimitExceeded",
        Code:      "PROVIDER_QUOTA_EXCEEDED",
        Message:   err.Error(),
        Retryable: true,
    },
})
```

### 4. Add logscanner to CI

```yaml
- name: Scan logs
  run: logscanner --files $(git diff --name-only origin/main...HEAD | grep '\.go$')
```

### 5. Start the brain

```bash
DATABASE_URL=postgres://... KAFKA_BROKERS=localhost:9092 ANTHROPIC_API_KEY=sk-... \
  ./observalog-brain
```

---

## Log contract

Every log call must contain:

| Field | Required | Source |
|-------|----------|--------|
| `event` | always | Developer — `domain.object.action` grammar |
| `message` | always | Developer — past tense, min 10 chars |
| `outcome` | decision points | Developer — `success/failure/partial/pending` |
| `duration_ms` | when outcome present | Developer — wall clock of operation |
| `error` | level=Error or outcome=failure | Developer — `log.Err` struct, never string |
| `trace_id` | auto | Library — from context or `sys_<uuid>` |
| `span_id` | auto | Library — generated at service entry |
| `seq` | auto | Library — service-local atomic counter |
| `ts` | auto | Library — RFC3339 UTC at emit time |

The scanner enforces these at CI time. A PR that emits `Error` with a string `"error"` field never merges.

---

## Event grammar

```
domain.object.action
```

**Domains:** `auth` · `doc` · `provider` · `infra`

**Actions (15 verbs):** `received` · `validated` · `rejected` · `published` · `failed` · `exhausted` · `expired` · `attempted` · `succeeded` · `created` · `updated` · `deleted` · `queried` · `connected` · `disconnected`

Valid: `auth.jwt.validated` · `doc.document.created` · `provider.send.rejected`

---

## Performance

| Metric | Value |
|--------|-------|
| Emit latency (hot path) | ~200ns per log call |
| Fingerprint computation | ~8ns (xxHash64) |
| Wire size reduction | ~70% vs unabbreviated JSON (before GZIP) |
| TimescaleDB point lookup | 0.46ms (vs 35–85ms ClickHouse) |
| LLM call elimination | ~95% via fingerprint dedup |
| log_index row size | ~60 bytes |

---

## Deployment

See [docs/deployment-guide.md](docs/deployment-guide.md) for full infrastructure setup.

**Required services:**
- Kafka (log transport, Fluent Bit → brain)
- TimescaleDB (log storage, hypertable with 1-day chunks)
- Valkey / Redis (gap detection grace window, 30s TTL)
- Anthropic API key (LLM triage)

---

## Documentation

| Document | Description |
|----------|-------------|
| [docs/whitepaper.md](docs/whitepaper.md) | Architecture decisions, design rationale, gap analysis |
| [docs/wire-format.md](docs/wire-format.md) | Part A byte positions, Part B abbreviation dictionary |
| [docs/event-grammar.md](docs/event-grammar.md) | Event naming contract and scanner rules |
| [docs/deployment-guide.md](docs/deployment-guide.md) | Infrastructure setup and configuration |

---

## Build

```bash
# Go library
cd observalog-go && go build ./... && go test ./...

# Rust scanner
cd logscanner && cargo build --release && cargo test

# Rust brain
cd observalog-brain && cargo build --release && cargo test
```

---

## License

MIT
