# Incident Lab — Runtime Log Scenario Generator

This directory generates raw, intentionally heterogeneous logs for testing the lite ObservaLog path.

## Safety boundary: never send ground truth to the brain

The generator creates three sibling paths:

- `incident-output/raw/` — raw log lines only; this is the only path Fluent Bit may watch.
- `incident-output/ground-truth/` — evaluator-only expected causes, symptoms, amplifiers and correlation constraints.
- `incident-output/manifest.json` — perturbation metadata for the test runner.

Never mount `ground-truth/` or `manifest.json` into Fluent Bit or expose them to the brain. The evaluator compares a finding with ground truth after the brain has produced the finding.

## Run

List cases:

```bash
python3 tests/incident_lab/generate.py --list
```

Generate one raw stream:

```bash
python3 tests/incident_lab/generate.py --scenario provider-rate-limit-retry
```

Generate every scenario in five arrival permutations, with replay duplicates and missing optional metadata:

```bash
python3 tests/incident_lab/generate.py \
  --all \
  --permutations 5 \
  --seed 20261125 \
  --shuffle-arrival \
  --duplicate-rate 0.05 \
  --drop-optional-rate 0.10 \
  --output /tmp/observalog-incident-run
```

Each permutation preserves source event timestamps but can alter arrival order. Duplicate records retain the same event ID. Optional-field removal tests degraded observability. The seed makes a run repeatable.

To test Fluent Bit, mount only the generated `raw/` directory as an input. Keep `ground-truth/` outside the collector's file paths and container mounts.

## Included cases

| Scenario | Number of service identities | What it tests |
|---|---:|---|
| healthy-single | 1 | Baseline request lifecycle |
| healthy-six-service | 6 | Fan-out, parent/child spans, mixed formats |
| auth-cascade | 3 | Upstream authentication failure and downstream symptoms |
| provider-rate-limit-retry | 4 | External rate limiting, retry amplification, pool saturation |
| replica-config-drift | 1 service / 4 instances | One stale issuer configuration against healthy controls |
| version-regression | 1 service / 4 instances | New version fails while previous version succeeds |
| database-deadlock | 3 | Database lock contention across separate traces |
| duplicate-message-side-effect | 3 | Message redelivery and non-idempotent business side effect |
| business-state-invariant | 4 | HTTP 200 and payment success while order state is wrong |
| unrelated-concurrent-failures | 2 | Coincident but independent requests |
| missing-trace-business-link | 3 | Missing trace/span IDs with a shared business entity |
| clock-skew-parent-span | 3 | Event-time inversion with explicit parent/child relation |
| async-queue-backlog | 3 | Publish succeeds but consumer completion is delayed |
| resource-pressure-oom | 1 service / 1 instance | Heap, GC, latency growth before OOM |
| tenant-feature-flag-drift | 1 service / 2 tenant cohorts | Tenant-specific flag/config defect |
| shared-dns-failure | 4 | Multiple services fail on one shared DNS dependency |
| producer-consumer-schema-drift | 2 | Producer field type incompatible with consumer |
| poison-record-isolation | 2 + malformed input | Bad record must not block valid records around it |
| mixed-format-sparse-logs | 3 | Sparse text/JSON formats and partial correlation |
| two-incidents-one-window | 2 | Two independent root causes in one time window |
| trace-id-reuse | 3 | Trace ID reused across distinct requests |
| twelve-service-fanout | 12 | Large distributed fan-out with a tax-provider trigger and downstream symptoms |

The generator uses Java/Spring-style JSON and text, Node/Pino-style JSON, OpenTelemetry-style nested JSON, Python logging text, Nginx access JSON, PostgreSQL text and unknown-alias JSON.

## Important interpretation

These are incident streams, not a list of parser assertions. The expected result includes constraints such as:

- distinguish root cause from symptoms and amplifiers;
- keep unrelated concurrent failures separate;
- preserve a partial association without inventing a trace;
- use healthy instances, versions or tenants as controls;
- avoid calling missing events a loss unless an expectation is established;
- maintain causal ordering when source clocks are skewed;
- quarantine malformed records while continuing adjacent valid records.

The source text deliberately does not include a `ground_truth` field or root-cause label. A scenario may be generated in different arrival orders and with missing metadata to test the system's tolerance.

## Current integration limitation

The current `main` Fluent Bit configuration is Docker-tail + grep + Kafka, and the current Rust consumer expects the legacy two-line A/B pair in one Kafka message. The existing configuration does not show multiline aggregation that combines the two separately emitted stdout lines. This lab's raw heterogeneous streams are designed for the new adapter path; they are not claimed to work end-to-end with the old parser.

The next integration step is to connect Fluent Bit raw input to the new adapter, persist canonical events, then replay each gate independently. Do not weaken the corpus to accommodate the legacy two-line parser.
