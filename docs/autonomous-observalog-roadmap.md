# ObservaLog — Delivery Phases

**Delivery strategy:** small, independently verifiable phases. Keep \`main\` stable; each phase is implemented on a short-lived feature branch and merged through a reviewed PR.

## Phase 0 — Baseline and compatibility freeze

**Goal:** Know exactly what must remain stable before changing transport or collection.

Work:
- Map the current \`observalog-brain\` ingestion, parser, writer, DB schema, triage and WebSocket path.
- Record contract-v1 fixtures and end-to-end behavior for existing Go/Java/Node producers.
- Confirm the exact two-part line framing and current Fluent Bit filter behavior. The current README and wire-format examples must be reconciled before generic input is added.
- Baseline throughput, p50/p95/p99 latency, CPU, RSS, database write rate and restart recovery.
- Validate TimescaleDB + Valkey dependencies and existing Compose health checks.
- Define raw-log fixture corpus: JSON, plain text, Java stack trace, CRI partial records, unknown severity, missing timestamp and trace-less logs.

Exit gate:
- Existing contract-v1 logs still pass golden tests.
- Baseline numbers and test hardware are recorded.
- V1 placeholder/provenance strategy is agreed.
- No production behavior changed.

## Phase 1 — Direct intake and minimal Compose target

**Goal:** Remove unnecessary broker infrastructure while preserving durable delivery.

Work:
- Add a Rust HTTP batch-ingestion endpoint to the existing brain (Axum is already present).
- Configure Fluent Bit HTTP output with compression, retry and filesystem buffering.
- Add bounded Tokio channels between intake, normalization, brain and persistence.
- ACK only after durable batch acceptance; choose and test either committed TimescaleDB batch persistence or an append-only local journal with replay.
- Add deterministic idempotency keys and duplicate-safe writes.
- Keep existing parser and downstream contract path intact.
- Retain TimescaleDB and Valkey. Remove Kafka and \`rdkafka\` only after delivery/recovery tests pass.
- Reduce Compose to one project with the required services and named volumes; keep a local Fluent Bit profile only for Docker-host log testing.

Exit gate:
- One \`docker compose up -d\` starts the control plane.
- Fluent Bit can send a batch directly to the brain.
- Brain/DB restart tests demonstrate no silent loss within configured buffers.
- Retries do not create duplicate logical events/findings.
- No Kafka service or Kafka client remains on the target runtime path.

## Phase 2 — Kubernetes namespace connection

**Goal:** One namespace connection automatically starts reliable node-level collection.

Work:
- Provide a least-privilege namespace-scoped connection/install flow.
- Install/update Fluent Bit as a DaemonSet on eligible nodes.
- Mount node log paths read-only; configure CRI/containerd parsing, Kubernetes metadata and namespace filtering.
- Set filesystem buffering, bounded storage limits, retry and health reporting.
- Detect collector rollout/health and show discovered namespace/workload/pod/container counts.
- Handle new pods, restarts, replacement pods, node drain and collector restart.

Exit gate:
- A namespace with multiple services and pods across multiple nodes is collected without app changes.
- New/restarted pods are picked up automatically.
- Events carry correct namespace/pod/container/node identity.
- Out-of-scope namespaces are not forwarded.

## Phase 3 — Universal log normalization and contract adapter

**Goal:** Accept what applications already emit and keep the existing brain stable.

Work:
- Add deterministic parsers for JSON and common plain-text formats.
- Extract timestamp, severity, logger, message, exception and available trace/span identifiers.
- Reconstruct multiline exceptions where Fluent Bit parser rules do not fully resolve them.
- Preserve raw event and Kubernetes metadata.
- Map known values into contract v1; annotate inferred, absent and synthetic values.
- Gate trace reconstruction and sequence-gap analysis when trace/sequence values are synthetic.
- Add parser fallback and quarantine visibility for unknown formats.

Exit gate:
- Golden fixtures from all supported formats produce deterministic contract-compatible events.
- Raw evidence is recoverable for every normalized event.
- Missing semantic fields never become fabricated facts.
- Existing contract-v1 producers remain unchanged.

## Phase 4 — Isolated, resilient high-throughput brain

**Goal:** Make concurrency behavior predictable under load and component failure.

Work:
- Partition stateful analysis by stable correlation key and assign one owner per partition.
- Use bounded Tokio \`mpsc\` channels and explicit admission/backpressure policies.
- Keep immutable messages between stages; avoid shared mutable state in the hot path.
- Supervise tasks and implement cancellation, graceful drain and restart recovery.
- Separate I/O-bound tasks from CPU-heavy parsing/analysis; add a bounded CPU executor only when benchmarks justify it.
- Batch DB writes; isolate DB stalls from ingestion while respecting the durable ACK boundary.
- Add queue-depth, lag, retry, drop/quarantine, worker-failure and recovery metrics.

Exit gate:
- Load and soak tests meet the numeric throughput/latency envelope agreed from Phase 0.
- Memory remains bounded under sustained overload.
- A worker failure does not corrupt other partitions.
- DB outage and restart recovery tests pass.
- No silent error-evidence drops.

## Phase 5 — Defect discovery and live intelligence

**Goal:** Deliver ObservaLog's differentiated product value.

Work:
- Add deterministic recurring-pattern, retry-amplification, temporal and sequence anomaly detectors where evidence supports them.
- Distinguish observations from hypotheses; attach raw log evidence and confidence.
- Reuse existing fingerprinting, known-issue cache, classification and selective LLM triage.
- Stream new, updated and retracted findings to the UI.
- Add investigation drill-down: original records, affected workloads, first/last seen, occurrence trend and supporting evidence.
- Evaluate SLM only against measured failure cases where deterministic parsing/analysis and the existing LLM path are insufficient.

Exit gate:
- Fault-injection scenarios produce evidence-backed findings with measured precision/recall.
- Repeated known issues avoid redundant LLM calls.
- UI updates continuously without becoming a generic infrastructure dashboard.
- SLM remains optional and outside the ingestion hot path unless benchmarks justify it.

## Branch and PR discipline

- This initiative starts from \`initiative/autonomous-namespace-ingestion\`.
- Each implementation phase gets a separate branch from the current integration branch.
- Each phase lands through a small PR with tests, benchmark evidence and rollback notes.
- Do not combine transport migration, normalization, analysis changes and UI redesign in one PR.
- Do not change frozen contract-v1 semantics without a versioned migration and compatibility tests.
