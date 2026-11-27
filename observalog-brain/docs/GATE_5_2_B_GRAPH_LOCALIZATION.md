# Gate 5.2-B — Deterministic Graph Localization

**Status:** Prototype implemented; shadow-only; not wired into production reasoning.
**Branch:** `lite/chaos-gate-3-5`

## Purpose
Rank plausible initiating services from typed structural evidence before introducing ML. It must not infer causality from temporal proximity or shared correlation identity.

## Implementation
- Pure Rust evaluator: `src/lite/graph_diagnosis.rs`.
- Inputs are event summaries and typed evidence edges.
- Only an explicit directional parent-child span edge can add upstream localization support.
- Same-trace/request/business-entity/message links are contextual only.
- Candidate score is inspectable: direct failures, typed upstream links, downstream-link penalty, healthy same-service/version controls.
- Healthy control contrast is bounded; it cannot prove cause.
- Ties are deterministic by service name.
- Abstains unless a candidate reaches threshold and has direct failure or typed upstream evidence.

## Deliberate limitations
- Not integrated with runtime DB queries or `lite_findings`.
- Does not interpret raw message text.
- No inferred dependency graph or temporal-only causal edges.
- No ML, SLM, or LLM involved.
- Score is a ranking heuristic, **not a probability or calibrated confidence**.
- A parent-child span edge can reflect instrumentation structure, not necessarily dependency causality; findings remain hypotheses.

## Tests
The module includes unit tests for:
1. failed parent vs downstream failure ranking;
2. same-trace-only abstention;
3. temporal-only abstention;
4. healthy controls as bounded contrast;
5. unrelated concurrent failures abstention;
6. unknown IDs in edges ignored.

## Gate 5.2-B corpus evaluation (first baseline)

Evaluated 22 scenarios × 3 deterministic permutations (66 streams), seed 20261127. Ground truth was consulted only after scoring. This was a prototype evaluator, not runtime ingestion.

| Metric | Result |
|---|---:|
| Service-localizable streams | 18 / 66 |
| Top-1 accuracy (localizable) | 33.3% |
| Top-3 accuracy (localizable) | 100% |
| MRR (localizable) | 0.806 |
| Winner coverage | 22.7% |
| Abstention rate | 77.3% |
| False attribution rate on non-localizable streams | 13.6% |
| Confident wrong on localizable streams | 0% |

**Interpretation:** The top-3 candidate ranking is promising, but Top-1 and coverage are weak. Most streams abstain. False winners are concentrated in provider retry storm, tenant feature-flag drift, and unrelated concurrent failures (3 permutations each). We must fix those false attributions before adding sequence ML. Metrics are synthetic-corpus only and are not product claims.

The truth mapper was tightened to reject composite and external-dependency labels as service targets. Healthy and multi-incident scenarios remain abstention targets. The Python scorer remains separate from the Rust prototype and must be kept aligned.

## Next
- Fix false attributions on non-localizable/multi-incident cases.
- Improve useful coverage only with explicit evidence (dependency, cohort, typed structural edges), never by lowering thresholds blindly.
- Re-run the full 66-stream evaluator after each change and inspect per-scenario regressions.
- Keep shadow-only; no production wiring.
