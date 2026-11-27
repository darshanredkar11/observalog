# ObservaLog ML Algorithm Selection Gate

**Status:** Approved to investigate; no algorithm selected for production.
**Branch:** `lite/chaos-gate-3-5`
**Principle:** Deterministic first. Use the smallest statistical/ML component that demonstrably adds value.

## Goal
Detect operational deviations and help prioritize investigation. An anomaly is not a root cause, an incident probability, or an instruction to take action. Deterministic Gates remain authoritative.

## Data constraints
- Available inputs are aggregated, tenant/service-partitioned telemetry windows.
- Incident labels are sparse, delayed, incomplete, and synthetic corpus labels are evaluator-only.
- Windows may have missing fields, incomplete telemetry, duplicates, replay, skew, and changing workload.
- Model must fail open/abstain and must not block deterministic processing.

## Candidate ladder (evaluate in this order)
1. **Deterministic rules** — known conditions, invariant violations, explicit failures. Existing authoritative layer; establish its scorecard first.
2. **Robust univariate baseline** — rolling median + MAD (robust z-score) per service/feature. Small, explainable, low-cost; handles individual deviations.
3. **EWMA / change detection** — detect gradual shifts and sustained changes where rolling point comparisons miss them. Evaluate only if a specific gap is demonstrated.
4. **Isolation Forest** — optional multivariate candidate for combinations of features that are individually moderate but jointly unusual. It earns inclusion only if it improves held-out incident recall/actionability without unacceptable false positives and has bounded resource cost.
5. **Heavier models (autoencoders, supervised classifiers, boosted trees)** — deferred until representative data, labels, and evidence justify them.

## Why Isolation Forest is only a candidate
It can identify unusual multivariate feature combinations without incident labels, but it does not inherently model time/seasonality, provide root cause, or guarantee useful calibrated probabilities. Feature scale, missing values, service-specific behavior, drift, and contamination assumptions need treatment. It must not be presumed superior to robust baselines.

## Evaluation design
- Use seeded corpus runs and preserve raw input and evaluator ground truth separately.
- Never expose evaluator-only labels, scenario metadata, or manifest fields to features or model training. The current mega-corpus raw generator includes a `scenario` field; strip/deny this field from model inputs and verify no label leakage.
- Split by scenario families and/or time blocks—not random adjacent windows—to prevent leakage. Keep an untouched holdout set.
- Evaluate healthy/no-incident cases, incident classes, overlapping incidents, recovery, tenant/service isolation, missing fields, duplicates, shuffled arrival, and noisy telemetry.
- Compare each candidate against the deterministic-only baseline and against robust-statistics baseline.

## Metrics (report separately)
- Precision, recall, F1; false positives per service-hour/window; false negatives by incident family.
- Detection delay from ground-truth onset; recovery lag and alert persistence.
- Performance on healthy controls and missing/incomplete telemetry.
- Calibration only if output is explicitly calibrated; otherwise do not call scores probabilities.
- CPU, memory, latency per window, throughput, cold-start behavior, and abstention rate.
- Tenant/service leakage tests and repeatability under fixed seed.

## Decision rule
A candidate is accepted only if it demonstrates measurable incremental value over the simpler baseline on the held-out corpus, with tolerable false-positive burden and bounded operational cost. If no ML candidate wins, ship deterministic + robust-statistics detection only.

## First experiment
Run a baseline comparison:
- Deterministic-only
- Rolling median + MAD
- Rolling median + MAD with EWMA/change detector (only as a separate arm)
- Isolation Forest as a separate arm, with documented normalization and missing-data handling

Produce a reproducible report before implementing any model in the production/shadow provider. No trained ML model is considered active until training/evaluation, artifact/versioning, runtime scoring, and E2E tests are all verified.
