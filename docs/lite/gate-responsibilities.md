# Lite ObservaLog — Streaming Gate Responsibilities

The gates below are logical responsibilities inside the brain. They do not imply one process or microservice per gate. Keep deployment simple: one brain process can host independent workers with bounded queues and independent recovery.

## Design invariants

- Every gate has one responsibility and an explicit input/output contract.
- Gates communicate asynchronously. No gate waits synchronously for a downstream gate to finish its work.
- In-memory queues are bounded. Queue saturation must cause backpressure, not unbounded memory growth.
- In-memory queues are not the durability boundary. Persisted inputs and per-gate checkpoints allow replay after restart.
- Each gate is independently restartable and idempotent.
- A failed record is isolated; it must not stop unrelated records or an entire stream partition.
- Preserve original bytes/text and source metadata throughout normalization.
- Unknown fields and missing values are data, not fatal contract violations.
- Deterministic processing comes first. Model reasoning is optional and never sits on ingestion's critical path.

## Gate 0 — Collection and transport (Fluent Bit)

**Responsibility:** collect raw source records and deliver them reliably to the brain.

Inputs: container stdout/stderr, files, journald/syslog, Kubernetes/container logs and other configured Fluent Bit inputs.

Outputs: raw record envelope containing original record, observed source, collector timestamp and source metadata.

Must handle: multiline records, partial lines, rotation, file offsets, retries, filesystem buffering, output backpressure, oversized records, malformed source envelopes and delivery retries.

Must not: infer root cause, invent trace IDs, discard unknown fields, or require a source-specific ObservaLog SDK.

Acceptance: an emitted source record is either delivered or remains durably queued/retryable; loss is observable through collector metrics and counters.

## Gate 1 — Adapter / normalization

**Responsibility:** turn heterogeneous input formats into one internal canonical event without losing source information.

Adapters are selected by configured source/parser or robust format detection. Initial formats should include JSON objects, nested OpenTelemetry-style JSON, Pino/Winston-style JSON, Spring/Logback text, Python logging text, Nginx access/error records and PostgreSQL text logs.

Output groups:

- Canonical fields when reliably mapped: event timestamp, observed timestamp, severity, service name, instance, version, environment, region, trace ID, span ID, parent span, request/correlation ID, business entity IDs, event name, message, duration, outcome, error kind/code/message, attributes.
- Per-field provenance: source key/path, mapping rule, explicit/inferred/unknown.
- Data quality: parse status, mapping confidence, type mismatch, conflicting aliases, truncated input.
- Original raw record and raw source envelope.

Rules:
- Never fabricate a trace/span/request/business identifier.
- Do not convert missing outcome into success.
- Do not infer error-free status from the absence of an error object.
- Preserve unknown fields in attributes/raw payload.
- Support aliases and nested paths through configuration, not hard-coded source-specific business logic.
- If format detection is ambiguous, retain raw data and emit an unparsed/partially parsed event.

## Gate 2 — Contract quality and identity enrichment

**Responsibility:** assess completeness and reliability; resolve source identity where possible.

Examples: missing trace ID, invalid timestamp, conflicting `service` aliases, unknown severity, duration represented as text, duplicate source event ID, conflicting service version, clock skew, service name changing across instances.

Outputs: event quality profile, identity confidence, normalization warnings and optional enrichment from a service inventory/config registry.

This gate cannot reject a valid raw event solely because optional fields are missing. It must distinguish:
- absent,
- explicitly null,
- malformed,
- conflicting,
- inferred,
- unknown.

## Gate 3 — Durable event journal (TimescaleDB)

**Responsibility:** persist raw and canonical event data before downstream analysis depends on it.

Preserve the existing hot/cold separation where it remains advantageous:
- hot typed projection for common filters, time bounds, service/instance/severity/status and correlation;
- cold JSONB for original and normalized payload, unknown attributes and provenance.

Add stable ingestion/event identity and idempotent writes. Persist source timestamp and collector/ingestion timestamp separately. Store service name as text; do not force arbitrary services into a fixed numeric enum.

Each downstream gate has an independent checkpoint/cursor. TimescaleDB is the replay source; bounded in-memory queues are wake-up/throughput aids, not the sole copy of an event.

## Gate 4 — Correlation and relationship graph

**Responsibility:** determine which events are related and label the strength/type of every relationship.

Prioritized evidence:
1. explicit trace ID plus coherent span/parent relation;
2. propagated request/message/correlation ID;
3. exact business entity ID with compatible workflow/service relationship;
4. source-specific message/transaction identifiers;
5. service dependency and temporal ordering;
6. semantic similarity as weak supporting evidence only.

Relationship types must remain distinct: parent-child, same request, same message, same business entity, dependency, temporal proximity, semantic similarity, possible coincidence.

A timestamp-only edge must never be represented as causal. Detect conflicting/reused IDs and clock skew. Correlation can be partial; do not force every event into one trace.

## Gate 5 — Behavioral signal detection

**Responsibility:** turn event streams and correlated activity into explicit, bounded signals. Do not make final root-cause claims.

Examples:
- error-rate/change-point increase;
- latency regression and tail-latency growth;
- repeated attempts / retry amplification;
- missing expected completion when an expectation is actually known;
- duplicate business operation;
- queue age/backlog growth;
- sequence discontinuity;
- resource saturation;
- contradictory state transitions;
- success-shaped transport response with failed business result.

Signals include scope, time range, affected entities, supporting event IDs, baseline/control comparison, severity and confidence.

## Gate 6 — Incident grouping and main reasoning

**Responsibility:** identify incident candidates, generate competing root-cause hypotheses, rank them, identify symptoms/amplifiers and produce an evidence-backed finding.

This is the business logic gate, described in detail in `reasoning-model.md`.

It may consume the outputs of earlier gates asynchronously and re-evaluate an incident as new evidence arrives. It must allow multiple independent incidents in the same time window and multiple contributing causes within one incident.

## Gate 7 — Finding persistence and publication

**Responsibility:** persist versioned findings and publish updates to downstream consumers.

A finding includes:
- incident and hypothesis identity;
- primary cause candidate and alternatives;
- affected services/instances/versions/tenants;
- symptom vs amplifier distinction;
- supporting and contradictory evidence references;
- confidence and uncertainty;
- recommended investigation/mitigation;
- lifecycle state and revision.

Do not let an LLM execute remediation. Findings and remediation are separate capabilities.

## Cross-gate resilience contract

For every gate define:
- input cursor or durable source;
- output record identity;
- idempotency key;
- bounded concurrency and queue capacity;
- retry policy with jitter/backoff;
- poison-record quarantine path;
- checkpoint only after durable output;
- metrics for lag, throughput, errors, drops, queue depth and replay count.

A restart must resume from the last durable checkpoint. Reprocessing the same input must not create duplicate canonical events, relationships, signals or findings.

## Things intentionally not introduced yet

- No new standalone microservices per gate.
- No new message broker just to preserve the old topology.
- No LLM on the ingestion path.
- No mandatory embedding for each event.
- No fixed set of allowed service names.
- No assumption that every record is traceable.
- No dashboard-first product layer.
