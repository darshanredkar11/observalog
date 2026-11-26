# Gate 3.5 — Chaos, Concurrency, and Recovery

Required before Gate 4. Separate from the Gate 3 unit and existing three-permutation E2E tests.

## Guardrails
- Test only on `lite/chaos-gate-3-5`; do not merge to `main` during this gate.
- Isolated CI resources and test-only credentials.
- TimescaleDB remains durable source of truth.
- Capture logs, queue counts, accepted/rejected counts, duplicate counts and failover observations.
- Health checks alone are insufficient; assert durable outcomes.

## Stage 1 — Concurrent ingest and identity pressure
- Multiple HTTP clients send a high-volume heterogeneous fixture concurrently.
- Repeat source keys and logical events; include identity collision storms and cross-tenant pairs.
- Randomize arrival order, including both parent/child orders.
- Assert unique source records durable exactly once; duplicates are accounted for.
- Assert no cross-tenant links, duplicate edges/findings or stuck work.

## Stage 2 — Replica contention
- Run two Brain replicas against one isolated TimescaleDB.
- Verify both are healthy but only one correlation leader processes at a time.
- Measure leader ownership through test-only DB/session diagnostics, not logs alone.
- Verify outputs remain logically idempotent.

## Stage 3 — Crash during active ingest
- Ingest while work accumulates; kill one Brain without graceful shutdown.
- Continue ingest through the surviving replica; verify accepted records persist and queues drain.
- Restart the killed replica; verify it creates no duplicate outputs.

## Stage 4 — Leader failover
- Identify the active correlation leader using test diagnostics.
- Kill it with work in flight.
- Verify the session advisory lock releases and the other replica takes leadership.
- Exercise abandoned-work lease expiry/reclaim.
- Verify no lost records, duplicate findings/edges, dangling adjacency or split-brain after restart.

## Stage 5 — Backpressure and soak
- Build controlled backlog below and above admission limits.
- Verify excess input receives retryable rejection without losing accepted input.
- Run bounded high-rate soak; record throughput, queue age, retries, CPU/memory and drain time.
- Stop producers; require pending, leased and dead queues to be zero.

## Required final assertions
- Raw/source-key accounting is correct and accepted input is preserved.
- Pending=0, leased=0, dead=0 at final checkpoint.
- Exactly one active correlation leader at any time.
- No cross-tenant links, dangling adjacency or duplicate edges/findings.
- Replays/restarts are idempotent and recover durable work.
- Backpressure is explicit and lossless for accepted records.

## Exit criterion
Gate 3.5 is green only when every stage passes CI with artifacts retained. On any failure, fix it and rerun the cumulative suite from Stage 1. Gate 4 remains blocked until this verdict.
