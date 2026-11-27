# Gate 5.2-B Corpus Evaluation

**Branch:** `lite/chaos-gate-3-5`
**Purpose:** Measure deterministic service-localization prototype on the Incident Lab.

## Protocol
- Runs all 22 scenario factories across 3 deterministic permutations each (66 streams).
- Mutation: 8% optional-field loss, 4% duplicate-delivery sampling, arrival-order shuffle, event-ID deduplication.
- Seed: 20261127.
- Ground truth is used only after candidate scoring to evaluate results.
- Candidate service target mapping is explicit in evaluator; non-service/external causes are treated as non-localizable by this primitive and false confident service attribution is measured.

## Metrics
- Top-1 and Top-3 accuracy on mapped service-localizable incidents.
- MRR on mapped service-localizable incidents.
- Coverage and abstention rate across all streams.
- False attribution rate for non-localizable/external incidents.
- Confident-wrong rate on mapped localizable incidents.

## Important limitations
This first evaluator is a harness and scoring baseline, not yet a production ingest/runtime adapter. It scores generator records directly using explicit parent-child span structure and deterministic failure indicators. No model reads labels or truth metadata. The target-service map is evaluator-only. Report metrics are descriptive; they are not calibrated probabilities.

## Review note
The score baseline intentionally tests whether the graph-only primitive has enough typed evidence to localize. Low accuracy or high abstention is expected to guide Gate 5.2-B repairs rather than be hidden.
