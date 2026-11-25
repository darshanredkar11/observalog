# Lite recovery and replay runbook

The raw journal and `lite_gate_work` are durable. The in-process workers are only executors; work is not held only in memory. The brain can restart without losing accepted raw records or queued gate work.

Admission is bounded by `LITE_MAX_INFLIGHT_GATE_WORK` (default 50,000 pending/leased items across all gates). The ingest transaction takes a short advisory lock while it checks capacity, preventing concurrent HTTP batches from collectively exceeding the configured bound. When full, ingest returns HTTP 503 and the transaction is rolled back; Fluent Bit's filesystem buffer retries after capacity becomes available.

## What an ingest 202 means

`POST /ingest/raw` returns HTTP 202 only after both the raw record and its first `normalize` work item have committed in TimescaleDB. Fluent Bit may retry a request when it cannot confirm delivery. The raw source key is derived from collector source, file path and byte offset so the same file record is not accepted twice.

## Inspect queue health

```sql
SELECT gate_name, state, count(*) AS work_items,
       max(attempts) AS max_attempts,
       min(created_at) AS oldest_created,
       min(available_at) FILTER (WHERE state = 'pending') AS next_pending
FROM lite_gate_work
GROUP BY gate_name, state
ORDER BY gate_name, state;

SELECT gate_name, last_error, count(*) AS failed_items
FROM lite_gate_work
WHERE state = 'dead'
GROUP BY gate_name, last_error
ORDER BY failed_items DESC;
```

Useful counters:

```sql
SELECT
  (SELECT count(*) FROM lite_raw_records) AS raw_records,
  (SELECT count(*) FROM lite_event_payload) AS canonical_payloads,
  (SELECT count(*) FROM lite_event_index) AS indexed_events,
  (SELECT count(*) FROM lite_signals) AS signals,
  (SELECT count(*) FROM lite_findings) AS findings,
  (SELECT count(*) FROM lite_gate_work WHERE state IN ('pending','leased')) AS in_flight,
  (SELECT count(*) FROM lite_gate_work WHERE state='dead') AS dead_work;
```

## Replay a downstream gate chain

Replay by stable raw event scope; do not delete raw records, payloads, source identity keys or event-index rows. First inspect the candidate set:

```sql
SELECT w.gate_name, w.state, count(*)
FROM lite_gate_work w
JOIN lite_event_index e ON e.id = w.payload_id AND e.event_ts = w.event_ts
WHERE e.service = 'orders'
  AND e.event_ts >= now() - interval '1 hour'
  AND w.gate_name IN ('quality','correlate','detect','reason')
GROUP BY w.gate_name, w.state
ORDER BY w.gate_name, w.state;
```

To replay from correlation (and therefore rerun detection and reasoning for the selected service/time window), reset those gate items in one transaction:

```sql
BEGIN;

UPDATE lite_gate_work w
SET state = 'pending',
    attempts = 0,
    available_at = now(),
    lease_until = NULL,
    last_error = NULL
FROM lite_event_index e
WHERE e.id = w.payload_id
  AND e.event_ts = w.event_ts
  AND e.service = 'orders'
  AND e.event_ts >= now() - interval '1 hour'
  AND w.gate_name IN ('correlate','detect','reason');

COMMIT;
```

To replay quality as well, include `quality` in the `gate_name` filter. Reprocessing is idempotent: relationship and signal uniqueness constraints prevent duplicate edges/signals; finding writes upsert by incident key, use event-time bounds for `first_seen`/`last_seen`, and keep `occurrence_count` at the number of unique evidence event IDs rather than incrementing on replay.

For a single gate, reset only that gate. For downstream consistency, prefer resetting that gate and all downstream gates in its chain.

## Normalization replay

Normalization is the identity boundary. It is not a routine downstream replay: source identities prevent a replay from creating a second canonical event. Rebuilding normalized events after changing adapter rules requires a deliberate reindex/migration strategy with a new adapter version and a controlled rebuild of the typed projection and downstream artifacts. Do not manually delete rows from `lite_source_identity` or rewrite `lite_event_payload` on a live system.

## Dead work

A dead work row is not discarded: the raw record and last error remain available. After fixing the cause, inspect the rows, then reset only the intended gate and scope using the SQL above. Do not blindly reset every dead row while an incompatible schema or parser bug remains.

## Poison record isolation

Raw input is stored before parsing. A malformed JSON record is retained as text and flagged; it does not prevent other raw records in the same HTTP batch from being inserted. Truly failed DB transactions return HTTP 503 so Fluent Bit can retry the batch. A gate parsing failure is retried independently and eventually remains visible as `dead` with its last error.

## Current limitations

- No authenticated administrative replay endpoint yet; replay is an operator-run SQL action.
- One worker per gate is started by the initial runtime; scale-out can run additional brain replicas because work claiming is row-locked, but pool sizing and concurrency should be load-tested.
- The first reasoning pass is deterministic and heuristic. It creates hypotheses, not proven root causes or automatic fixes.
- LLM-assisted analysis, service inventory enrichment, metrics/infra signals, sophisticated expected-event detection and formal finding lifecycle updates are not yet implemented.
