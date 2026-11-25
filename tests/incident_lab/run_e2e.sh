#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$BASH_SOURCE")/../.." && pwd)"
cd "$ROOT"

cleanup() {
  docker compose down -v --remove-orphans || true
  rm -f var/logs/*.log
}
trap cleanup EXIT

docker compose up -d --build timescaledb brain

echo "Waiting for brain health..."
for _ in $(seq 1 90); do
  if curl -fsS http://localhost:4000/health >/dev/null; then
    break
  fi
  sleep 2
done
curl -fsS http://localhost:4000/health >/dev/null

python3 tests/incident_lab/generate.py \
  --all --permutations 1 --seed 20261125 \
  --duplicate-rate 0.03 --drop-optional-rate 0.05 \
  --output /tmp/observalog-e2e

cp /tmp/observalog-e2e/raw/*.log var/logs/
expected_lines="$(cat var/logs/*.log | wc -l | tr -d ' ')"
echo "Generated $expected_lines raw log records"

docker compose up -d fluent-bit

psql_query() {
  docker compose exec -T timescaledb \
    psql -U observalog -d observalog -Atqc "$1"
}

echo "Waiting for Fluent Bit delivery and brain gates..."
for _ in $(seq 1 180); do
  raw_count="$(psql_query "SELECT COUNT(*) FROM lite_raw_records" 2>/dev/null || echo 0)"
  index_count="$(psql_query "SELECT COUNT(*) FROM lite_event_index" 2>/dev/null || echo 0)"
  pending_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')" 2>/dev/null || echo 999999)"
  dead_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state='dead'" 2>/dev/null || echo 999999)"
  if [ "$raw_count" -gt 0 ] && [ "$index_count" -gt 0 ] && [ "$pending_count" -eq 0 ]; then
    break
  fi
  sleep 2
done

raw_count="$(psql_query "SELECT COUNT(*) FROM lite_raw_records")"
index_count="$(psql_query "SELECT COUNT(*) FROM lite_event_index")"
signal_count="$(psql_query "SELECT COUNT(*) FROM lite_signals")"
finding_count="$(psql_query "SELECT COUNT(*) FROM lite_findings")"
parent_child_count="$(psql_query "SELECT COUNT(*) FROM lite_event_links WHERE relation_type='parent_child'")"
pending_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')")"
dead_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state='dead'")"
raw_preserved="$(psql_query "SELECT COUNT(*) FROM lite_event_payload WHERE raw_text <> ''")"

printf 'raw=%s indexed=%s signals=%s findings=%s parent_child_edges=%s pending=%s dead=%s raw_preserved=%s\n' \
  "$raw_count" "$index_count" "$signal_count" "$finding_count" "$parent_child_count" \
  "$pending_count" "$dead_count" "$raw_preserved"

test "$raw_count" -gt 0
test "$index_count" -gt 0
test "$raw_preserved" -eq "$index_count"
test "$signal_count" -gt 0
test "$finding_count" -gt 0
test "$parent_child_count" -gt 0
test "$pending_count" -eq 0
test "$dead_count" -eq 0

echo "Lite end-to-end ingest and gate smoke test passed."
