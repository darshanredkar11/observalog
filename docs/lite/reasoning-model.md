# Lite ObservaLog — Main Business Reasoning Gate

## Mission

Given imperfect logs from one or more services, discover what likely broke in the business/system behavior, identify the earliest supported cause, separate downstream symptoms and amplifiers, and tell an engineer what evidence to inspect next.

The gate must not merely label an incident by counting ERROR records. It must reason over evidence, alternatives, controls and uncertainty.

## Input

- Canonical events plus original raw records.
- Field-level provenance and quality profiles.
- Correlation relationships with type and confidence.
- Behavioral signals with event references and time windows.
- Service/instance/version/environment/region metadata when available.
- Optional deployment/configuration/health/metric events when present.

No one input is mandatory in every record. Missing data reduces what can be concluded; it does not imply success or failure.

## Output

A versioned finding containing:
- incident scope and time range;
- primary hypothesis and category;
- competing hypotheses;
- likely initiating cause;
- downstream symptoms;
- amplifiers (for example, aggressive retries);
- affected and unaffected services/instances/versions/tenants;
- supporting evidence references;
- counter-evidence and missing evidence;
- confidence plus explanation of uncertainty;
- recommended next investigation and safe mitigation;
- event IDs and correlation edges used to reach the conclusion.

The brain must be allowed to return `insufficient_evidence`, `multiple_causes` or `unresolved`. It must not force a single root cause.

## Hypothesis families

1. **Code regression:** failures start with a release/version change; older version or unaffected control instances succeed; the failure is localized to changed code paths.
2. **Configuration/secret/feature-flag issue:** same code behaves differently across instances, regions, tenants or environments; a configuration, issuer, endpoint, secret or flag differs.
3. **Infrastructure/resource saturation:** latency/queueing/GC/pool saturation/CPU/memory/file descriptors precede failures; multiple workloads on the same resource are affected.
4. **External dependency:** multiple consumers of the same provider/endpoint show aligned failures; provider/network evidence precedes dependent failures.
5. **Data/schema/contract problem:** a new payload or schema version triggers deterministic parse/validation failures at a consumer boundary.
6. **Async delivery/acknowledgement/idempotency:** repeated message IDs, missing acknowledgements, redelivery and duplicate side effects appear.
7. **Concurrency/ordering:** races, deadlocks, stale reads, conflicting writes or out-of-order state transitions occur under concurrency.
8. **Business logic/semantic defect:** transport returns success but business invariants, state transitions or financial/accounting relationships are wrong.
9. **Telemetry/collection defect:** parser loss, source filtering, buffer overflow, sequence anomalies, collector restart or schema mismatch explains missing evidence.
10. **Unknown or multiple causes:** evidence is insufficient or genuinely indicates more than one cause.

## Evidence reasoning

For each hypothesis, collect:

- **Causal structure:** parent/child spans, request propagation, message IDs, transaction IDs and explicit dependency links.
- **Temporal precedence:** candidate cause occurs before symptoms after accounting for clock skew and event-time uncertainty.
- **Blast radius:** which services, versions, instances, tenants or regions fail together.
- **Change point:** deployment, config/secret rotation, feature-flag update, dependency version, infra change or traffic shift.
- **Repeatability:** same evidence pattern across independent requests, not just repeated copies of one event.
- **Controls:** comparable successful requests, healthy instances, unaffected regions or previous versions.
- **Business invariants:** payment captured vs order paid, inventory reservation vs order fulfilment, message accepted vs processed, ledger debit/credit balance.
- **Data quality:** missing fields, parsing confidence, sampling, late arrival, dropped records and collector backpressure.
- **Counter-evidence:** facts that make a hypothesis less likely.

A useful hypothesis is not just one with positive signals; it must explain why plausible alternatives fit less well.

## Correlation safeguards

- Same timestamp alone is weak co-occurrence, never causality.
- Same service alone is not enough to join records.
- Same user or tenant alone is not enough to join transactions.
- A business ID is strong evidence only when its semantics and scope are known.
- Reused or malformed trace IDs reduce confidence and may indicate an identity/propagation defect.
- Clock-skewed timestamps must not reverse causality when span/message structure establishes order.
- Missing events are evidence only when an expected event can be established (for example, a durable publish/consume contract, heartbeat, completion expectation or sequence boundary).
- Separate concurrent failures must remain separate, even when their time windows overlap.
- One incident may contain root cause, secondary failures and feedback amplifiers. Do not label every affected service as an independent root cause.

## Diagnostic distinctions

### Code bug vs configuration issue

Compare the same service across versions, instances, environments and tenants:
- same version + one instance failing + instance-specific config difference → configuration/host hypothesis;
- new version fails across instances while prior version succeeds under comparable config → code regression hypothesis;
- failures across multiple services sharing an endpoint/secret immediately after rotation → shared configuration/dependency hypothesis.

Do not assert these conclusions without the corresponding comparison evidence.

### Dependency failure vs retry amplification

Identify the earliest dependency error, then separately quantify retries, concurrency and resource saturation. The provider may be the trigger while the caller's retry policy amplifies the blast radius.

### Transport success vs business success

HTTP 2xx, database commit, queue publish or SDK success is not equivalent to business completion. Model explicit business state transitions and invariants when logs expose them.

### Missing logs vs missing work

A missing completion event can indicate application failure, collector loss, parsing failure, sampling, or asynchronous delay. Check collector health, raw-source evidence, offsets, queue state and data-quality signals before declaring the business operation never completed.

## Initial deterministic scoring dimensions

Do not hard-code one universal numeric formula before calibration. Initially rank candidate hypotheses using explainable features:

- causal-link strength;
- temporal precedence confidence;
- service dependency compatibility;
- same version/config/host/tenant cohort;
- onset around a change;
- repeatability across independent traces;
- blast-radius consistency;
- healthy control evidence;
- contradictory evidence;
- telemetry completeness and adapter mapping confidence.

Keep each feature separately inspectable. Calibrate confidence against hidden-truth scenarios; a fixed label such as “high” is not meaningful until measured.

## Example reasoning case: provider timeout with retry storm

Observed:
1. Order request begins and inventory reservation succeeds.
2. Payment calls provider and receives repeated timeouts/429s.
3. Payment retries increase.
4. Payment connection-pool wait time rises after retries begin.
5. Order requests time out later.
6. Other services not depending on this provider remain healthy.

Expected reasoning:
- initiating cause candidate: provider latency/rate limiting;
- amplifier: caller retry/backoff or concurrency policy;
- downstream symptom: order timeout/pending state;
- not supported: inventory as root cause;
- next investigation: provider response patterns, retry configuration, pool saturation, circuit breaker behavior;
- confidence should be separated for trigger vs amplifier.

## Evaluation requirements

Ground truth is stored outside the log stream and never passed to the brain.

Score:
- root-cause category and entity accuracy;
- top-k hypothesis recall;
- affected service and symptom/amplifier distinction;
- evidence precision and coverage;
- false correlations across unrelated incidents;
- confidence calibration;
- correct abstention under insufficient evidence;
- time from event arrival to useful finding;
- update quality as late evidence arrives.

A fluent explanation without referenced evidence is a failed diagnosis.
