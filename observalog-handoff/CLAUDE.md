# ObservaLog — Claude Code Context File

## What this is

ObservaLog is a structured log contract system for distributed Go microservices.
It has three components built in sequence:

1. `observalog-go` — Go library imported by product services
2. `logscanner` — Rust binary, CI static analyser
3. `observalog-brain` — Rust binary, AI triage engine

We are building component 1 first. Do not touch 2 or 3 until 1 is complete and tested.

## Build and test commands

```bash
# observalog-go
cd observalog-go && go build ./... && go test ./...

# logscanner
cd logscanner && cargo build --release && cargo test

# observalog-brain
cd observalog-brain && cargo build --release && cargo test
```

## The three things you must NEVER change without reading DECISIONS.md first

1. **seq counter** — must be `*atomic.Uint32` stored as a pointer in context.
   Changing this to any shared state (Redis, global var, mutex) breaks Decision 1.

2. **Part A wire format byte positions** — defined in `observalog-go/wire_contract.go`
   and mirrored in `observalog-brain/src/ingest/wire_contract.rs`.
   Both files must stay in sync. Changing one without the other corrupts all logs.

3. **Fingerprint input** — must include `service | event | error_code | ctx_primary_key`.
   Removing ctx_primary_key causes semantic collisions (Gap 3).

## Read these files before touching any code

- `DECISIONS.md` — 13 closed decisions. Do not reopen them.
- `*/CONTRACT.md` in each module — invariants for that module specifically.

## Structural rules (enforced by repo structure, not by you)

- One concept per file. If a file exceeds 150 lines, it probably contains two concepts.
- Contract files (`*_contract.go`, `*_contract.rs`) are read-only reference.
  Never add logic to them. They contain constants and compile-time assertions only.
- Dependencies flow one direction. Check DEPENDENCY_GRAPH.md before adding an import.
- AGENTS.md is the machine-readable version of this file. Keep both in sync.

## Current build status

Nothing built yet. Start with `observalog-go`.
Sequence: config.go → context.go → fields.go → dict.go → wire_contract.go →
          wire.go → fingerprint.go → async.go → degrade.go → emit.go →
          emit_contract.go → middleware/http.go → middleware/kafka_consumer.go →
          middleware/kafka_producer.go → tests

## Wire format schema version

Current: `1`
Location: `observalog-go/wire_contract.go` constant `PartASchemaVersion`
Must match: `observalog-brain/src/ingest/wire_contract.rs` constant `PART_A_SCHEMA_VERSION`
