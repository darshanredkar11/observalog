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

## Next
Gate 5.2-B CI runs targeted module tests, then cumulative Rust test compilation. If broader suite is blocked by known existing failures, report that separately rather than treating the prototype gate as green.
