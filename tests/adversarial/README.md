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

## Run locally

```bash
cp .env.example .env
# Replace LITE_INGEST_TOKEN with a local random secret (24+ bytes).
set -a; . ./.env; set +a
docker compose up -d timescaledb brain
python3 tests/adversarial/generate_payloads.py --repetitions 10 --output /tmp/observalog-adversarial.jsonl
python3 tests/adversarial/run_negative.py --payload-file /tmp/observalog-adversarial.jsonl
docker compose down -v
```

CI: .github/workflows/lite-adversarial.yml builds the brain, starts a fresh TimescaleDB, generates the corpus, and executes the negative test.

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
