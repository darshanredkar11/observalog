# Gate 5.1 — ML Advisory Evidence Contract

Status: **Shadow boundary exists; no real ML provider/model is enabled.**

## Authority and safety
Gate 5 deterministic behavioral qualification remains authoritative. Gate 5.1 can emit advisory-only evidence; it cannot qualify signals, erase/suppress Gate 5 signals, invalidate Gate 5 results, or mutate deterministic findings. Provider execution is fail-open and independently bounded.

## Inputs
Only bounded tenant/service-scoped aggregates are supplied. Raw event text, raw JSON, messages, trace/request/business identifiers, or labels must not enter the provider boundary. Window size, duration sample cap, query time budget, output row cap, provider timeout and concurrency are bounded.

## Isolation
Host tenant/service partitioning occurs before provider invocation. Providers receive opaque tokens. Persisted provider state is tenant-scoped and never loaded across scope tokens.

## State envelope
Provider state is provider-owned and explicitly versioned with provider ID, model version, tenant scope token, feature-schema version, state-format version, lifecycle (cold/warming/ready/degraded), and opaque payload. Incompatibility must reject state and return to safe cold/abstaining behavior; no implicit migrations. The host does not prescribe persistence backend or core schema. Gate 5.1 does not persist state yet.

## Cold start and calibration
Cold state abstains; no score is fabricated. No real model is introduced until cold-start adequacy, held-out calibration, efficacy, false-positive/false-negative, detection delay, tenant/service isolation and completeness/drift gates are accepted. Scores are not root-cause probabilities, severity verdicts or remediation instructions.

## Fault isolation
Timeout, provider error/panic, invalid score, incompatible/unavailable state and capacity saturation must be captured as advisory outcomes. Gate 5 and primary pipeline must remain unaffected. Disabled mode must make no aggregate query/provider call and must not change Gate 5/schema behavior.

## Current implementation
- Five-minute DB aggregate windows, bounded to 100 groups, query timeout 2 seconds; the source scan is not hard-row-capped.
- Aggregate features: event/error/failure/trace/retry/slow/invariant counts and rates, latency aggregates, mapping-quality proxy and missing count. CPU/memory/queue features are unavailable and remain null.
- Opaque tenant/service tokens are host-assigned before scoring.
- Cold-start provider always abstains.
- Shadow worker is opt-in at startup via `LITE_ML_SHADOW_ENABLED=true`; disabled by default.
- Provider state envelope compatibility contract added, but no provider-owned state persistence or real provider.
- Existing timeout/panic/capacity tests cover runner-level fail-open behavior; DB query timeout/error handling is source-level only pending runtime worker instrumentation.
- Disabled runner test proves provider not called at runner boundary; it does **not yet** instrument the DB worker to prove zero queries.

## Gate status
Gate 5.1 remains **in progress**. Native PostgreSQL workflow previously passed 85 tests and Clippy. The release build/test/Clippy workflow run `45875722071` has now completed successfully at commit `79843dc`. This is not final cumulative validation for the new state-contract tests until a fresh CI run completes.

