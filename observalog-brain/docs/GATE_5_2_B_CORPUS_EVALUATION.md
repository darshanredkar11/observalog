# Gate 5.2-B Corpus Evaluation

**Branch:** `lite/chaos-gate-3-5`
**Purpose:** Measure deterministic service-localization prototype on the Incident Lab.

## Protocol
- Runs all 22 scenario factories across 3 deterministic permutations each (66 streams).
- Mutation: optional-field loss (8% sampled), duplicate-delivery sampling (4%), arrival-order shuffle, event-ID deduplication.
- Seed: 20261127.
- Ground truth is used only after candidate scoring for evaluation.
- Service target mapping is conservative: exact service, service@version, service/path, or exact instance. Healthy and multi-incident cases are excluded from single-root service localization.

## Metrics (latest corrected evaluation)
- **66 streams / 22 scenarios**
- **18 streams** conservatively service-localizable
- Candidate ranking Top-1: **66.7%**
- Candidate ranking Top-3: **100%**
- MRR: **80.6%**
- Diagnosis Top-1 (including abstentions as misses): **33.3%**
- Coverage: **9.1%**
- Abstention: **90.9%**
- False attribution among non-localizable streams: **0%**
- Confident wrong among mapped service-localizable streams: **0%**

These results reflect a deliberate high-precision/low-coverage posture. The 100% Top-3 is candidate retrieval, not causal accuracy. Top-1 candidate rank and final diagnosis decision are reported separately so abstention does not get disguised as ranking performance.

## Repairs triggered by the first run
1. Removed permissive target mapping that incorrectly treated external/compound entities as services.
2. Changed winner eligibility: direct failure volume alone is insufficient; a candidate must have explicit upstream failure-link evidence or a typed cohort contrast.
3. Added version contrast (failed version vs healthy alternate version) and replica contrast (failed instance vs healthy same-version peer).
4. Added abstention when top candidates are tied rather than selecting alphabetically.
5. Rejected parent-child causal scoring when the parent event timestamp is after the child event, preventing a later downstream timeout from being scored as the initiating event.

## Gate-specific test additions
- failure volume without typed causal/cohort evidence abstains;
- version regression contrast localizes the affected service;
- failed parent event occurring after child event cannot establish upstream causality;
- tied multiple-failure services abstain.

## Limitations
This is a harness/prototype evaluation, not an end-to-end runtime integration. It scores generator records directly with a deterministic Python mirror of the Rust rules; the scorer does not receive ground-truth labels. No raw logs are parsed through the Rust collector, database correlation runtime, or production `lite_findings` path. Scores are not calibrated probabilities.

## CI
- Evaluator: [GitHub Actions run](https://github.com/darshanredkar11/observalog/actions/runs/45967361169)
- Report artifact: `gate52-b-corpus-evaluation` from the run above.
- Rust Gate 5.2-B unit tests + full test-suite compilation: [run](https://github.com/darshanredkar11/observalog/actions/runs/45967034373)

## Review note
The measured outcome is conservative. It can rank likely services, but it frequently abstains where root cause is external, compound, or insufficiently supported by structural/cohort evidence. This is intentional; Gate 5.2-C should add sequence-based evidence and be evaluated against the same corpus without weakening evidence-verification rules.
