# ObservaLog — Sequence Diagrams

These diagrams describe the target flow. They are design contracts for implementation phases, not claims that the behavior is already implemented.

## Pipeline invariant: asynchronous stream gates

All event-processing gates are independently progressing stream processors. Each gate consumes, processes and emits without a synchronous cross-gate round trip. Internal communication uses bounded asynchronous queues/streams. Durable delivery ACKs are protocol signals tied to the explicit acceptance boundary; they do not make downstream analysis a synchronous dependency. Backpressure is explicit, bounded and may propagate upstream.

## 1. Connect a Kubernetes namespace

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant UI as ObservaLog UI
    participant Brain as Rust control/API
    participant K8s as Kubernetes API
    participant FB as Fluent Bit DaemonSet

    User->>UI: Connect cluster + namespace
    UI->>Brain: POST /connections (credentials, namespace)
    Brain->>K8s: Validate identity and namespace access
    K8s-->>Brain: Access and namespace metadata
    Brain->>K8s: Apply/update scoped ServiceAccount, RBAC, ConfigMap, DaemonSet
    K8s-->>Brain: Resources accepted
    K8s->>FB: Schedule one collector per eligible node
    FB->>K8s: Read pod metadata using scoped permissions
    FB-->>Brain: Health/collector status
    Brain-->>UI: Connection ready + discovered workload summary
    UI-->>User: Live namespace activity
```

Notes:
- The collector is node-level because container log files are node-local.
- Namespace selection is enforced in collector configuration; the brain still validates namespace identity on received events.
- The exact credential hand-off and RBAC manifest are security-reviewed in Phase 2.

## 2. Continuous pod/container log discovery

```mermaid
sequenceDiagram
    autonumber
    participant Kubelet
    participant FS as Node log filesystem
    participant FB as Fluent Bit tail input
    participant K8s as Kubernetes metadata filter
    participant Brain as Rust HTTP intake

    Kubelet->>FS: Create/update container log symlink and CRI log file
    loop Continuous file discovery and tail
        FB->>FS: Discover matching file / read new bytes
        FS-->>FB: CRI records
        FB->>FB: Parse runtime envelope and reconstruct multiline records
        FB->>K8s: Enrich record using pod/container identity
        K8s-->>FB: Namespace, pod, container, node, labels
        FB->>FB: Apply namespace filter and batch records
        FB->>Brain: POST compressed log batch
        Brain-->>FB: ACK after durable acceptance
    end

    Note over Kubelet,FB: New pod/container files are discovered automatically; no per-pod Rust task is required.
```

## 3. Normalize an arbitrary log into the existing contract

```mermaid
sequenceDiagram
    autonumber
    participant FB as Fluent Bit
    participant Intake as Rust HTTP intake
    participant Parse as Normalizer
    participant Adapter as Contract adapter v1
    participant Brain as Existing brain pipeline
    participant DB as TimescaleDB

    FB->>Intake: Batch of raw records + Kubernetes metadata
    Intake->>Intake: Validate size, source and batch envelope
    Intake-)Parse: Async bounded hand-off
    Parse->>Parse: Detect JSON / text / severity / timestamp / exception
    Parse-)Adapter: Async stream of parsed fields + raw evidence + provenance
    Adapter->>Adapter: Map known values; mark missing fields unknown/synthetic
    Adapter-)Brain: Async contract-compatible event stream
    Brain-)DB: Batched asynchronous persistence
    DB-->>Brain: Commit signal
    Brain-->>Intake: Durable acceptance signal (protocol ACK path)
    Intake-->>FB: HTTP success
```

Rules:
- Real trace/span/sequence/outcome values are preserved only when present in source evidence.
- Synthetic placeholders are provenance-marked and excluded from trace reconstruction/gap detection.
- Raw log content and collector metadata remain available for investigation.
- A batch is not ACKed merely because it entered an in-memory Tokio channel.

## 4. Evidence-to-finding flow

```mermaid
sequenceDiagram
    autonumber
    participant Event as Normalized event
    participant Router as Partition router
    participant Worker as Analysis worker
    participant DB as TimescaleDB
    participant LLM as Existing LLM triage
    participant UI as WebSocket/UI

    Event->>Router: Contract-compatible event
    Router->>Worker: Route by stable state key
    Worker->>Worker: Fingerprint, correlate, update owned state
    alt Known issue / deterministic classification
        Worker->>DB: Persist occurrence and finding update
    else New ambiguous failure requiring triage
        Worker->>DB: Load bounded supporting evidence
        DB-->>Worker: Evidence rows and raw payloads
        Worker->>LLM: Selective triage request
        LLM-->>Worker: Typed RepairId + explanation + confidence
        Worker->>DB: Persist finding and evidence references
    end
    Worker-->>UI: Finding created/updated/retracted
    UI-->>UI: Update live finding stream
```

## 5. Collector and brain recovery

```mermaid
sequenceDiagram
    autonumber
    participant FB as Fluent Bit
    participant Disk as Fluent Bit filesystem buffer
    participant Brain as Rust intake
    participant DB as TimescaleDB

    FB->>Brain: Send batch
    Brain--xFB: Connection timeout / unavailable
    FB->>Disk: Retain batch on disk
    loop Retry with backoff
        FB->>Brain: Retry batch
        alt Brain still unavailable
            Brain--xFB: Retryable failure
            FB->>Disk: Keep buffered batch
        else Brain available
            Brain->>DB: Persist batch idempotently
            DB-->>Brain: Commit
            Brain-->>FB: Durable ACK
            FB->>Disk: Advance/remove acknowledged batch
        end
    end
```

The delivery guarantee is at-least-once with idempotent ingestion. A durable-acceptance boundary and duplicate-key design are release gates before removing Kafka.

## 6. Backpressure and worker isolation

```mermaid
sequenceDiagram
    autonumber
    participant FB as Fluent Bit
    participant HTTP as Intake task
    participant Q as Bounded Tokio channel
    participant N as Normalizer worker
    participant A as Analysis worker
    participant P as Persistence worker

    FB->>HTTP: Submit batch
    HTTP->>Q: try_send / bounded admission
    alt Queue has capacity
        Q-->>N: Deliver owned batch
        N-)A: Emit normalized immutable event stream
        A-)P: Emit persistence commands
        P-->>HTTP: Separate durability ACK signal
        HTTP-->>FB: HTTP success
    else Queue at capacity
        Q-->>HTTP: Full
        HTTP-->>FB: Retryable response
        FB->>FB: Keep batch in filesystem buffer
    end
```

No worker writes into another worker's mutable state. Queue saturation is visible and propagates upstream; it must not turn into unbounded memory growth or silent log loss.
