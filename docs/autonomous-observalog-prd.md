# ObservaLog — Autonomous Namespace Log Intelligence

**Status:** Proposed product and architecture baseline  
**Date:** 2026-11-25  
**Target deployment:** One self-hosted Docker Compose control plane; Fluent Bit managed at the Kubernetes node boundary  
**Primary outcome:** Discover software defects from the logs applications already emit, without requiring application code or logging-contract changes.

## 1. Product statement

ObservaLog connects to a Kubernetes namespace, discovers its workloads and continuously receives the container logs produced by those workloads. It adapts heterogeneous logs into the existing ObservaLog contract and feeds the existing Rust brain, storage model and triage pipeline.

The product is not an infrastructure-monitoring dashboard. Its differentiator is finding software defects and suspicious behaviour from log evidence, then showing the evidence and confidence behind each finding.

**Product promise:** Connect a namespace. ObservaLog learns the running software from its logs and continuously looks for defects.

## 2. Product principles

1. **Adapt to the customer's logs.** No required SDK, code change, prescribed logger, or migration to the ObservaLog logging contract.
2. **Preserve the existing brain.** The v1 two-part contract, parser, fingerprinting, DB write model and triage pipeline remain the downstream compatibility boundary.
3. **Reuse proven infrastructure.** Fluent Bit collects logs. Tokio primitives coordinate Rust work. TimescaleDB remains the durable analytical store. Valkey remains until the current gap-detection dependency is deliberately removed.
4. **Keep the runtime small.** One Rust brain process with isolated modules/tasks; no Kafka, GraphDB, Flink, custom broker, or new microservice fleet in the target design.
5. **Make state ownership explicit.** Stateful workers own their state. Components communicate through bounded channels. Shared mutable state is avoided by default.
6. **Resilience before cleverness.** Backpressure, durable buffering, replay, idempotency, graceful shutdown and failure isolation are product requirements.
7. **Evidence before inference.** Preserve raw log evidence and distinguish observed values from inferred or unavailable values.
8. **AI is not on the ingestion hot path.** Deterministic parsing and analysis run first. Existing LLM triage remains selective; an SLM is not a v1 dependency.
9. **Stream through every gate.** Pipeline stages exchange streams asynchronously; no synchronous cross-gate round trips. Each stage owns its state and independently progresses.
10. **Simple, adaptive, pluggable.** Use the smallest implementation that meets the business requirement. Keep module boundaries replaceable, but avoid speculative interfaces/frameworks until a real variation needs them.
11. **Boundedness is mandatory.** Queues, batches, concurrency and retained state have explicit limits. Backpressure may propagate; it must never turn into unbounded memory growth or fire-and-forget data loss.

## 3. Target user and primary use case

**Primary user:** An engineering team operating one or more services in Kubernetes that wants to find defects without changing application logging code.

**Use case:** A user connects one namespace. The system automatically discovers pods and containers on all nodes, collects their stdout/stderr logs continuously, normalizes records, detects suspicious patterns and streams findings to the UI.

The system must work whether the namespace contains one service or many services, one pod or hundreds of pods, and whether pods are replaced or restarted while collection is running.

## 4. Scope

### In scope for the first release

- Connect one Kubernetes namespace through a guided, minimal workflow.
- Automatically install/configure Fluent Bit as a node-level DaemonSet for that namespace.
- Collect container stdout/stderr from node log files, including CRI/containerd formats supported by Fluent Bit.
- Enrich events with namespace, pod, container, node and available workload metadata.
- Support structured JSON logs and ordinary text logs; preserve raw input.
- Handle multiline exceptions and stack traces using Fluent Bit's supported parsers/multiline support.
- Convert heterogeneous records into the existing ObservaLog contract representation.
- Preserve the existing Rust parser/brain/triage pipeline wherever possible.
- Publish evolving findings to the existing real-time UI/WebSocket path.
- Run the ObservaLog control plane using one Docker Compose project.

### Explicitly out of scope for v1

- Requiring application teams to adopt ObservaLog SDKs or emit the custom contract.
- Replacing Fluent Bit with a custom collector.
- Building a message broker or a second streaming platform.
- Graph database, metrics platform, APM replacement, service-map product or general-purpose log warehouse.
- AI/SLM interpretation of every line.
- Reading arbitrary files inside containers that are not exposed through the container logging pipeline.
- Automatic code changes or repairs.
- Multi-cluster fleet management and hosted multi-tenant SaaS controls.

## 5. User experience

1. User starts the ObservaLog control plane with `docker compose up -d`.
2. User chooses **Connect Kubernetes namespace**.
3. ObservaLog validates the cluster and namespace permissions.
4. ObservaLog installs or updates the managed Fluent Bit DaemonSet and namespace-scoped configuration.
5. Fluent Bit begins tailing container log files on every eligible node and enriches records with Kubernetes metadata.
6. ObservaLog shows discovered workloads and live ingestion status.
7. The Rust brain normalizes records, analyzes them and streams findings as evidence accumulates.

No per-service parser configuration should be required for common JSON, plain-text and standard stack-trace formats. Unsupported formats must remain visible as raw evidence and be counted—not silently discarded.

## 6. Target architecture

```text
Kubernetes namespace
  workloads -> pods -> containers -> node log files
                                  |
                         Fluent Bit DaemonSet
                         tail / parse / enrich
                         multiline / buffer
                                  |
                          HTTP batch output
                                  |
                 ObservaLog Rust ingestion endpoint
                                  |
                       bounded Tokio channels
                                  |
                       universal log adapter
                                  |
                  existing ObservaLog contract v1
                                  |
                     existing Rust brain/triage
                         /              \
                 TimescaleDB         live findings
                                      WebSocket/UI
```

### Component responsibilities

| Component | Responsibility | Ownership boundary |
|---|---|---|
| Fluent Bit | Node-level file tailing, CRI/Docker parsing, Kubernetes metadata, multiline, disk buffering and retry | Runs at the Kubernetes node boundary; no product-specific analysis |
| Rust ingestion endpoint | Receive compressed/batched events, validate envelope, apply admission/backpressure and acknowledge delivery | Owns request/batch lifecycle only |
| Normalizer/contract adapter | Parse JSON/text, extract known fields, retain raw record, map to contract v1 | Owns parser and multiline-independent normalization state |
| Existing brain | Fingerprint, trace reconstruction where valid, gap/failure classification, triage | Existing brain modules retain their state and DB contracts |
| Tokio workers/channels | Isolate stages and bound work in flight | Bounded queues; state owned by the receiving worker |
| TimescaleDB | Durable event/index/payload and finding history | Existing database is the durable analytical store |
| Valkey | Existing 30-second out-of-order gap grace window | Retained until a separately tested replacement exists |
| Existing UI/WebSocket | Stream new and changing findings | Subscriber state only |

### Kubernetes collector boundary

Fluent Bit must run where the container log files exist: on the Kubernetes nodes. A central Compose container cannot directly tail log files on remote cluster nodes. Therefore:

- The **ObservaLog control plane** is one Docker Compose deployment.
- The namespace-connect workflow automatically manages the Fluent Bit DaemonSet in the target cluster.
- The Compose Fluent Bit service, if retained, is for local Docker development/demo only; it is not a substitute for the Kubernetes DaemonSet.

This keeps one central Compose deployment and one user-facing namespace connection while respecting node-local log access.

## 7. Contract adapter rules

The current contract is the internal compatibility boundary, not an application-side requirement.

- If a trace ID, span ID, parent span, sequence, service, level or outcome exists in the source log, preserve it.
- Extract common JSON keys and standard severity/logger/timestamp formats deterministically.
- Preserve the complete original record and collector metadata in the context/payload.
- Never invent a genuine distributed trace, request sequence, business outcome, error code or root cause.
- Where the fixed v1 header requires a placeholder, mark the corresponding value as synthetic/unknown in payload metadata. The brain must not use synthetic trace/sequence values for trace reconstruction or gap detection.
- Preserve the source service/workload name in context even when the v1 service-code vocabulary cannot represent it directly.
- Keep inferred values distinguishable from observed values, with provenance and confidence.
- Unknown or malformed records remain inspectable and are quarantined/counted rather than dropped silently.

A compatibility spike must confirm the exact fixed-header sentinel strategy against the current parser before implementation. If schema v1 cannot represent provenance safely, introduce the smallest explicit schema-versioned extension rather than silently changing v1 semantics.

## 8. Streaming, isolation and resiliency requirements

### Concurrency model

- One Rust process; no internal microservice mesh.
- Every gate is an independently progressing stream processor: receive stream, process, emit stream.
- No synchronous cross-gate calls, round trips, or downstream result waits on the ingestion hot path.
- Cross-gate communication is asynchronous through bounded channels/streams. Backpressure is explicit and may propagate upstream.
- Control-plane operations (configuration, connect, health, shutdown) may use request/response; they are not per-event pipeline dependencies.
- Tokio bounded `mpsc` channels between pipeline stages.
- One logical owner for each state partition; route related events consistently to that owner.
- Immutable event data is passed between stages.
- No global mutable issue map and no shared lock on the normal event hot path.
- CPU-heavy work uses a bounded CPU executor only where profiling justifies it.
- No per-event synchronous database query.
- Database writes are batched and isolated from ingestion/analysis tasks.
- A stage never waits synchronously for another stage's business processing to complete; ACK/durability signals are separate protocol events and must not block unrelated stream progress.

### Delivery and recovery

Target delivery is **at-least-once with idempotent persistence**, not an unsupported claim of exactly-once processing.

- Fluent Bit filesystem buffering is enabled for the node collector.
- HTTP output retries transient failures.
- The brain acknowledges a batch only after the chosen durable-acceptance boundary has succeeded.
- The durable-acceptance boundary must be proven before Kafka is removed: either a committed TimescaleDB batch or a small append-only local journal with replay. Do not acknowledge an in-memory Tokio enqueue as durable.
- Retries may duplicate a batch; ingestion IDs and idempotent writes prevent duplicate logical records.
- Bounded channels enforce backpressure. When the brain cannot keep up, Fluent Bit's disk buffer absorbs the delay; it must not silently discard error evidence.
- Worker panic/failure is isolated and supervised; restart restores or rebuilds owned state.
- Graceful shutdown stops intake, drains bounded work, commits durable writes and closes streams.
- Malformed/poison records are isolated with the raw payload and failure reason.

### Performance

- Keep parsing and contract conversion allocation-conscious.
- Batch records at transport and database boundaries.
- Avoid extra serialization round trips where a typed internal representation can preserve the same contract semantics.
- Benchmark throughput, p50/p95/p99 latency, CPU, RSS, queue depth, DB write rate and recovery time with representative mixed logs.
- Do not publish throughput claims until measured on a documented environment. Set numeric release gates from Phase 0 baselines.

## 9. Single Docker Compose deployment

The target control-plane deployment is one Compose project:

| Service | Target role | Decision |
|---|---|---|
| `brain` | One Rust process: HTTP ingestion, normalizer, existing analysis, WebSocket | Keep |
| `timescaledb` | Existing durable storage | Keep |
| `valkey` | Existing gap-grace behavior | Keep initially; reassess after compatibility tests |
| `fluent-bit` | Optional local Docker log collector profile | Keep only for local/demo input; Kubernetes uses managed DaemonSet |
| `kafka` / `kafka-init` | Current transport | Remove only after direct HTTP delivery/recovery tests pass |

Target operator command for the control plane: `docker compose up -d`.

No separate Kafka, Zookeeper, GraphDB, Flink, Redis replacement, or custom broker is introduced. The Kubernetes collector is provisioned automatically as part of namespace connection, not manually configured service by service.

## 10. Acceptance criteria

### Namespace and collection
- Connecting a namespace discovers all eligible pods and containers across nodes.
- New pods/containers are picked up without restarting ObservaLog.
- Pod replacement, container restart and log rotation do not permanently stop collection.
- Events include namespace, pod, container and node identity when available.
- No application code or logging-contract change is required.

### Normalization and compatibility
- Existing contract-v1 logs continue through the existing parser and brain unchanged.
- JSON and plain-text fixtures normalize into contract-compatible events.
- Raw source log is retrievable for every normalized event.
- Missing trace/sequence/outcome fields are marked unknown/synthetic; they do not produce false trace-gap findings.
- Multiline exceptions remain one logical event where parser rules identify them.
- Unknown formats are visible and measurable.

### Reliability and performance
- Brain restart and transient DB outage do not cause silent loss within the configured buffer/retention window.
- Retried batches do not create duplicate logical findings.
- Memory remains bounded at configured queue limits.
- Slow persistence does not block collector intake indefinitely; backpressure propagates.
- Load tests report throughput and p99 latency at stated hardware/configuration.
- No SLM call is made in the per-event ingestion path.

### Deployment
- Control plane starts with one Compose command.
- Configuration and secrets are externalized; production defaults do not ship known passwords.
- Health checks cover database, brain intake and real-time API.
- Namespace collector RBAC is least-privilege and limited to required metadata/log access.

## 11. Key risks and open decisions

1. **Remote node log access:** The collector must run on each target node. Compose alone cannot mount remote Kubernetes node files; the connect workflow must manage the DaemonSet.
2. **No-Kafka durability:** Fluent Bit disk buffering plus HTTP retries is insufficient if the brain acknowledges before durable acceptance. The ACK/journal/batch-write spike is a release gate.
3. **V1 header semantics:** Fixed-width trace/service/level fields were designed for contract-emitting applications. Placeholder/provenance behavior must be verified so generic logs do not create false trace or gap analysis.
4. **Current collector filter:** The existing Fluent Bit configuration filters only contract-shaped lines. Generic log intake must remove that filter and retain arbitrary records.
5. **Current local path:** The current Compose collector tails Docker's `/var/lib/docker/containers`; Kubernetes requires node-level `/var/log/containers` and runtime-aware parsing.
6. **PII/secrets:** Raw logs may contain credentials or personal data. Redaction, access control and retention need explicit defaults before production rollout.

## 12. Success definition

A team can connect a namespace, make no application changes, and observe continuously collected logs plus evidence-backed software-defect findings. The system remains bounded, recoverable and understandable under pod churn, traffic bursts, worker failures and database interruptions.
