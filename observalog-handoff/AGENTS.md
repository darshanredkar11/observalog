# ObservaLog

## Build
```
cd observalog-go && go build ./... && go test ./...
cd logscanner && cargo build --release && cargo test
cd observalog-brain && cargo build --release && cargo test
```

## Never change without reading DECISIONS.md
- seq counter must be `*atomic.Uint32` pointer in context (Decision 1)
- Part A byte positions in wire_contract.go and wire_contract.rs must stay in sync (Decision 7)
- Fingerprint must include ctx_primary_key (Decision 9, Gap 3)

## Contract files — read before touching paired source file
- `observalog-go/wire_contract.go` — before touching wire.go
- `observalog-go/emit_contract.go` — before touching emit.go
- `observalog-brain/src/ingest/wire_contract.rs` — before touching parser.rs
- `observalog-brain/src/ingest/writer_contract.rs` — before touching writer.rs

## Wire format schema version: 1
## Build sequence: observalog-go → logscanner → observalog-brain
