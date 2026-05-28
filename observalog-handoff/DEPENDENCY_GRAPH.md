# ObservaLog — Dependency Graph

Dependencies flow in ONE direction. No cycles permitted.
Check this file before adding any import.

---

## observalog-go (Go library)

```
emit.go
  → wire.go
      → dict.go         (terminal — no internal imports)
  → fingerprint.go      (terminal)
  → async.go            (terminal)
  → context.go          (terminal)
  → degrade.go          (terminal)
  → fields.go           (terminal)

config.go
  → async.go

middleware/http.go
  → context.go
  → degrade.go

middleware/kafka_consumer.go
  → context.go
  → degrade.go

middleware/kafka_producer.go
  → context.go
```

Contract files (`*_contract.go`) have zero internal imports. They are leaf nodes.

---

## observalog-brain (Rust crate)

```
main.rs
  → ingest/kafka.rs
      → ingest/parser.rs
          → ingest/wire_contract.rs  (terminal)
      → ingest/writer.rs
          → db/queries.rs
          → ingest/writer_contract.rs (terminal)

triage/chain.rs
  → db/queries.rs

triage/gap.rs
  → triage/chain.rs

triage/classify.rs
  → triage/chain.rs

triage/dedup.rs
  → db/queries.rs

triage/context.rs
  → (semble_rs — external crate)

triage/environment.rs
  → (redis — external crate)
  → (rdkafka — external crate)

triage/llm.rs
  → triage/repair.rs
  → triage/context.rs
  → triage/environment.rs

db/schema.rs      (terminal)
db/queries.rs     (terminal — only imports sqlx)
```

---

## logscanner (Rust binary)

```
main.rs
  → scanner.rs
      → go/walker.rs
          → go/rules/exit_log.rs
          → go/rules/outcome.rs
          → go/rules/error_struct.rs
          → go/rules/grammar.rs → grammar.rs
          → go/rules/pii.rs
          → go/classify.rs
      → finding.rs (terminal)

grammar.rs (terminal — DOMAINS, ACTIONS constants only)
finding.rs (terminal — types only)
```

---

## Cross-module contracts (must stay in sync manually)

| Go file | Rust file | What must match |
|---------|-----------|-----------------|
| `observalog-go/wire_contract.go` | `observalog-brain/src/ingest/wire_contract.rs` | Part A byte offsets, schema version, service codes, level codes |
| `observalog-go/dict.go` | `observalog-brain/src/ingest/parser.rs` | Abbreviation dictionary — every key/value pair |
