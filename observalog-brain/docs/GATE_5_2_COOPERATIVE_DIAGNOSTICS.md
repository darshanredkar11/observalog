# Gate 5.2 — Cooperative Diagnostic Intelligence (Sequence + Graph)

**Status:** Design/build gate; research candidates, not production-approved  
**Parent:** Gate 5 (ML advisory)  
**Isolation:** `lite/chaos-gate-3-5` only; no merge to main without explicit approval

## Goal

Shift the evaluation target from event anomaly detection to **possible-cause ranking from heterogeneous logs**. A diagnosis is a ranked hypothesis supported by evidence, not a declaration of causality.

## Problem statement

Given a bounded incident window of raw/canonical log events, identify:
1. abnormal event sequences or missing/rare transitions,
2. likely initiating component(s),
3. downstream symptoms and amplifiers,
4. contradictory/healthy comparison evidence,
5. confidence and reasons to abstain.

## Architecture under test

`Deterministic guards → Sequence specialist → Graph localization specialist → Evidence verifier → (optional) SLM explanation`

### Stage A — Sequence specialist

Research candidates: DeepLog (LSTM), LogBERT (Transformer). Inputs must be label-free sequence windows grouped by service/workflow, not shuffled corpus rows. Output:
- anomaly score per event/window
- learned sequence/transition evidence
- model/version and score calibration metadata

It makes no cause claim.

### Stage B — Graph localization specialist

Research candidates: graph ranking/GNN/GAT; begin with a non-neural graph baseline before a GNN. Inputs: typed correlation graph and Stage A evidence. Output:
- ranked candidate nodes/components
- supporting graph paths/edges
- contribution scores
- counter-evidence/healthy-peer comparisons

It makes no causal-certainty claim.

### Stage C — Evidence verifier (deterministic)

Reject/discount candidate explanations if:
- tenant boundary or correlation identity conflicts
- required evidence references are absent
- candidate is downstream only and a stronger upstream signal exists
- healthy peer evidence contradicts a broad component hypothesis
- chronology is unreliable due to clock skew (prefer causal edges over wall-clock alone)
- evidence is only temporal proximity

### Stage D — Optional SLM/LLM explanation

Only after hypotheses and evidence exist. It summarizes ranked hypotheses, evidence, uncertainty and next checks. It cannot create event IDs, edges, metrics, or override verifier decisions. Disabled by default; fail-open.

## Pairing hypothesis

Sequence specialist detects unusual behavior; graph specialist localizes a likely source using typed relationships, propagation patterns and healthy comparisons. The pair is useful only if it beats each specialist and the deterministic graph baseline on root-cause localization and false-correlation safety.

## Cumulative experiments

- **5.2-A:** Validate dataset shape, labels and leakage; scenario-specific sequence windows.
- **5.2-B:** Deterministic graph-ranking baseline (no ML).
- **5.2-C:** Sequence model only.
- **5.2-D:** Graph model only.
- **5.2-E:** Cooperative pair.
- **5.2-F:** Pair + deterministic evidence verifier.
- **5.2-G:** Optional SLM explanation, offline and evidence-constrained.

## Metrics

Primary:
- Root-cause Top-1 / Top-3 / MRR
- Correct initiating-component vs symptom vs amplifier separation
- False causal-edge / unrelated-incident merge rate
- Abstention calibration when evidence is insufficient

Secondary:
- Sequence anomaly precision/recall at event/window level
- Evidence coverage and contradiction rate
- Detection/localization delay where timestamps are trustworthy
- Runtime, memory, throughput and cost

Healthy scenarios must be reported separately. Do not aggregate event rows into one accuracy number that conceals incident-level failures.

## Data contracts and leakage rules

- Raw records only; never expose scenario names, ground truth, expected observations, root-cause strings, primary entities or label-derived features.
- Ground truth stays evaluator-only and joins after predictions by stable identifiers.
- Split train/test by scenario family or generated seed, not random event rows.
- Duplicate deliveries remain exact repeated raw records; no collector-side duplicate flag.
- Assert raw/evaluator identifiers and duplicate payload consistency.
- Sequence windows group by service/workflow and use clock-skew-aware ordering.
- Event IDs/timestamps may support grouping/evaluation but must not encode class labels.
- Use scenario generator causal relationships as evaluator truth; timestamps alone are not causal proof.

## Exit criteria

- Reproducible seeded experiment and leakage audit pass.
- Deterministic graph baseline established before ML.
- Each specialist evaluated alone.
- Cooperative model beats the best single specialist and deterministic baseline on Top-1/Top-3 or MRR without materially increasing false correlations.
- Added value demonstrated on independent incident scenarios/seeds.
- Verifier rejects unsafe or unsupported explanations.
- SLM evaluated for factuality/evidence fidelity, not discovery accuracy.

## Limitations/non-goals

No production activation, auto-remediation, causal certainty, assumed access to external metrics/tracing backends, or heavy model until corpus validity and simpler baselines are established.

## Research results — 2026-11-28 (research branch only)

The first cooperative implementation was evaluated by the compiled Rust `gate52_cooperative_eval` binary across all 22 incident-lab scenarios and 3 seeded permutations (66 streams). Python generated/mutated inputs and scored returned predictions only; it did not duplicate the diagnostic algorithm.

| Metric | Result |
|---|---:|
| Service-localizable streams | 18 / 66 |
| Accepted diagnoses / coverage | 18 / 66 (27.27%) |
| Service-level Top-1 on localizable streams | 18 / 18 (100%) |
| Exact target Top-1 on localizable streams | 15 / 18 (83.33%) |
| Target-granularity gaps | 3 |
| Specialist decisions | 12 |
| Graph-fallback decisions | 6 |
| Abstentions | 48 |
| False attributions on non-localizable streams | 0 |
| Confidently wrong service localizations | 0 |
| Missing evidence IDs | 0 |

The three granularity gaps are all the `replica-config-drift` case: graph fallback identifies service `identity`, while evaluator ground truth names instance `identity-03`. This is a service-localization success but not an instance-level diagnosis. It must not be reported as exact-target accuracy.

The four narrow conventional specialists (authentication cascade, PostgreSQL deadlock signature, duplicate-message/idempotency invariant, and robust resource trend + OOM endpoint) each achieved 3/3 expected detections, 0 false positives across the other 18 scenario families, and 0 missing evidence IDs on this corpus. This is targeted synthetic fixture coverage, not representative statistical precision/recall.

The Rust-backed Gate 5.2-B evaluator remains separately reported. Cooperative results do not replace that baseline; they measure the combination of specialist hypotheses, deterministic evidence verification, and conservative graph fallback.

### Interpretation and limitations

- This result demonstrates that narrow evidence-specific algorithms can repair several known failure modes without an LLM or trained model.
- It does not validate production collector parsing, durable correlation queries, runtime graph edge construction, or persisted finding quality.
- The 25K mega-corpus is event-level, not multi-event sequence data; it is not suitable for DeepLog/LogBERT sequence training in its current form.
- The graph fallback still needs instance-level target resolution for replica drift and a richer typed evidence model for other unhandled families.
- No learned model has been trained or enabled. The cooperative module remains research-only and is not wired into production reasoning/findings.

CI: [Gate 5.2-E Cooperative Diagnosis Research](https://github.com/darshanredkar11/observalog/actions/runs/46129681583)
