# Lite Adversarial Gate Testing

This branch is a test-only follow-up to lite/streaming-adapter. It does not alter the MVP branch or main.

## Intent

Do not add product features until the pipeline demonstrates safe behavior when input is adversarial, incomplete, misleading, malformed, contradictory, or noisy. Tests validate the whole gate chain, not just the adapter.

## Payload families

- Empty records, whitespace, scalars, booleans, nulls, arrays and nested objects
- Identity/correlation fields missing, null, empty, wrong type, truncated, or dotted OTel aliases
- Service, serviceName, level and severity aliases that disagree
- Contradictory outcome/status/error clues
- Malformed timestamps, far-future/past dates, negative/huge durations and numeric extremes
- Unknown/unexpected fields, deep objects, wide objects, large messages, exception stack noise
- Retry markers in nested fields, natural-language descriptions and key=value prose
- Plain text, invalid JSON, partial JSON, Unicode, multilingual text, emoji and control characters
- Identical event IDs delivered twice to validate raw retention vs normalization idempotency

The generator randomizes record order deterministically from a seed. Repetitions vary event IDs so ordinary repetition does not become accidental deduplication.

## Gate-level expectations

| Gate | Negative assertions |
|---|---|
| Normalize | Never crashes; raw text remains available; malformed records still get canonical representation; absent IDs remain absent; no invented outcomes/durations |
| Quality | Missing/invalid/conflicting aliases are marked; flags are bounded and stable; bad types do not cause panics |
| Correlate | No false relationship solely from shared time; absent trace/span is not fabricated; contradictory or reused identifiers remain weak evidence |
| Detect | Negative/extreme durations do not trigger slow-operation signals; explicit ERROR/FATAL and failure outcomes can signal; malformed fields do not crash detection |
| Reason | Evidence stays attached; findings are hypotheses; no unsupported certainty; replay must not inflate occurrence counts |

The HTTP boundary is also tested for wrong token (401), empty body (400), and over-limit body (413). Ingestion requests are sent in parallel (four clients), then the durable queue must drain without dead or pending work.

## Gate 1 - Normalize (current focus)

Gate 1 is tested independently. The current unit corpus covers:

- Scalar, null, boolean, array and empty input
- Missing/null/empty/wrong-type aliases
- Top-level fields vs nested exception noise
- Array-wrapped event objects
- Invalid/negative/non-finite/extreme duration values
- Conflicting duration aliases
- Invalid and extreme timestamps
- Unknown severity and ambiguous outcome
- Incidental key=value phrases in ordinary prose
- Malformed/truncated JSON preservation
- Large multilingual records

Gate 1 regression suite is run with native Rust only; no Docker and no database are required:

```bash
cd observalog-brain
cargo test gate1_adversarial_tests -- --nocapture
cargo test adapter
```

CI workflow: `.github/workflows/gate1-normalize.yml`.

**Progress rule:** Gate 1 must pass its adversarial corpus and existing adapter regressions before Gate 2 is attacked. Findings are fixed within Gate 1's responsibility and added to the permanent corpus.

## Gate 1 bombardment result

Verified on native GitHub Actions (no Docker):

- **18 adversarial Normalize tests passed**
- **29 adapter regression tests passed**
- CI run: https://github.com/darshanredkar11/observalog/actions/runs/45630770706

Failures found and fixed during the bombardment:

1. **Text message overwritten by a single KV token** — a long multilingual log containing `message=...` lost its full text body. KV parsing now does not overwrite the parsed message.
2. **Nested noise could override record-root values** — field extraction now prioritizes valid root scalar aliases.
3. **Nested exception details could be promoted as the log message** — recursive lookup is limited to recognized telemetry envelope containers.
4. **Array-wrapped event objects could be treated as a single event** — arrays are no longer recursively promoted into event fields.
5. **Invalid durations were accepted** — negative, NaN, infinity and out-of-range durations are not canonicalized; invalid duration is flagged.
6. **Numeric timestamp extremes could saturate during conversion** — non-finite/out-of-range timestamp numbers fall back to collector observed time.
7. **Unknown numeric severity values could become FATAL** — numeric severity mappings now use bounded known ranges; unknown values remain unknown.
8. **Conflicting duration/severity aliases were not consistently reported** — alias conflict flags expanded.

These failures are now part of the permanent regression corpus.


## What this does not prove

This is not a mathematical proof or a fuzzing campaign of every byte sequence. It is a deterministic adversarial integration corpus. Future work:

1. Gate-specific fault injection (kill worker mid-transaction, disconnect DB, delay queue, restart container)
2. Property-based adapter fuzzing with minimized crash inputs
3. Backpressure/queue-cap saturation test (expect 503, retry and eventual acceptance)
4. Replay-after-crash tests at each gate boundary
5. Metamorphic tests for permutations: conclusions should be stable when arrival order changes, accounting for time-skew uncertainty
6. False-positive scoring against scenario ground truth, with strict separation between truth and runtime
7. Malicious input safety (pathological text, deeply nested JSON, very large arrays, invalid UTF-8 transport)

A test suite exposes regressions and establishes bounded confidence. It must never be called full proof.
