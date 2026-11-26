# Gate 5.1 — ML Advisory Evidence Contract

Status: **Contract and type boundary started; no ML provider is enabled.**

## Authority

Gate 5 deterministic behavioral qualification remains authoritative. Gate 5.1 may add advisory anomaly evidence only. It cannot qualify a signal, erase/suppress Gate 5 signals, invalidate a Gate 5 result, or change existing findings when disabled.

## Input: aggregated windows only

Gate 5.1 consumes `AggregatedWindow` (`feature_schema_version=1`) from a tenant-scoped host aggregation boundary. Raw event text, raw JSON, messages, identifiers (trace/request/business IDs), or ground-truth labels are not provider inputs.

Candidate window features include:
- counts: total, error, failure outcomes, retry, slow, invariant violations, independent traces;
- rates: error, failure, retry;
- aggregate latency: mean and p50/p95/p99;
- resource aggregates: CPU/memory utilization;
- queue depth and delta;
- window completeness and missing/dropped telemetry counts.

Windows must be bounded and deterministic. Feature extraction must produce the same feature vector for the same scoped inputs, window boundaries and feature version. Late/partial data is reflected in completeness, not silently treated as normal.

## Tenant isolation

Tenant authorization and isolation occur outside the ML provider. The host partitions events and aggregates independently per tenant before scoring. The provider must never fetch cross-tenant state or use tenant identity for cross-tenant model lookup. The tenant partition field is an opaque scope token; provider isolation must be audited at the host/provider adapter boundary. A shared model may be used only if it cannot read tenant-specific raw or state without explicit host-scoped access.

## Advisory output

A provider result is versioned and contains:
- provider identifier and model version;
- feature schema version;
- anomaly score in [0,1], **not a probability of root cause**;
- or explicit abstention reason;
- optional feature contributions.

No score is a root-cause verdict, incident qualification, severity assignment, or automated remediation instruction. A cold model must abstain as `insufficient_baseline` (or equivalent) until its provider has adequate, validated baseline state.

## Provider boundary / fault containment

- Host invokes providers behind bounded concurrency and timeout.
- Provider failures, invalid scores, timeouts and unavailable model state are captured as advisory failure/abstention and cannot block Gate 5 or the main processing pipeline.
- Provider execution is optional and can be disabled at runtime/configuration.
- Provider adapters validate feature schema and output bounds.
- Provider must not be able to mutate deterministic signal or finding records.
- No ML provider dependency is added to ingestion/normalization or the Gate 5 path.

## Model state and persistence

Model state persistence is provider-owned. The host does not prescribe a shared table or serialization format. Provider-owned state must:
- have explicit model/provider version and state format version;
- support safe upgrade/migration or reject incompatible state;
- avoid loading state across tenant boundaries;
- fail into abstention/unavailable status without interrupting Gate 5;
- expose whether baseline state is cold, warming, ready or degraded.

No core schema migration is required for the advisory feature. Persisting scores later, if needed, must use a separately optional/versioned advisory storage surface; disabled mode must not alter schema or pipeline behavior.

## Cold-start / calibration policy

No reliable anomaly-detection claim is made from a cold model. Baseline sufficiency is provider-specific and must be documented, tested, and measured on held-out scenarios. Scores require calibration evaluation. Until calibration is accepted, outputs remain advisory and confidence labels must not imply calibrated probability.

## Acceptance gates (incremental)

1. **Contract tests** — aggregated feature shape, no raw-log field, validation, bounds and schema version.
2. **Cold start** — provider abstains; no fabricated score.
3. **Provider fault** — timeout/error/invalid score is isolated; Gate 5 path still completes.
4. **Tenant isolation** — host partitioning, provider state isolation and cross-tenant negative tests.
5. **Feature aggregation** — deterministic, bounded windows; duplicate/late/missing telemetry behavior.
6. **Persistence** — provider-owned versioned state load/save/upgrade tests; corruption and mismatch produce safe abstention.
7. **Disable switch** — off means no provider calls; Gate 5 outputs and existing schemas unchanged.
8. **Shadow evaluation** — advisory results collected only in evaluator scope; Gate 5 metrics remain separate.
9. **Calibration & efficacy** — evaluate false positives, true/false negatives, top-k anomaly usefulness, cold/warm behavior, detection delay, tenant/service boundaries, data completeness and drift. ML must demonstrate measured value before promotion beyond shadow advisory use.

## Implementation started

- `observalog-brain/src/lite/ml_advisory.rs`: feature-only input and advisory output types, provider interface, validation, and unit tests.
- `lite/mod.rs`: registers the module.
- No model library, provider, worker, persistence, schema migration, scoring path, or Gate 5 change is introduced in this first increment.

## Next

Build the deterministic aggregation/window producer from existing Gate 5 signals, then test it before any ML provider is connected. The first provider should be a cold-start abstaining stub to exercise the fault-isolated boundary—not a model marketed as anomaly detection.
