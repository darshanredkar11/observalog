#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

export FLUENT_BIT_CONFIG=./tests/incident_lab/fluent-bit-e2e.conf

cleanup() {
  rc=$?
  trap - EXIT
  if [ "$rc" -ne 0 ]; then
    echo "=== docker compose status (failure diagnostics) ==="
    docker compose ps -a || true
    echo "=== brain logs ==="
    docker compose logs --no-color --tail=300 brain || true
    echo "=== Fluent Bit logs ==="
    docker compose logs --no-color --tail=300 fluent-bit || true
    echo "=== TimescaleDB logs ==="
    docker compose logs --no-color --tail=200 timescaledb || true
    echo "=== remaining gate work ==="
    docker compose exec -T timescaledb psql -U observalog -d observalog -x -c \
      "SELECT gate_name,state,attempts,last_error,count(*) FROM lite_gate_work GROUP BY gate_name,state,attempts,last_error ORDER BY gate_name,state,attempts" || true
  fi
  docker compose down -v --remove-orphans || true
  rm -f var/logs/observalog-e2e-generated.log
  exit "$rc"
}
trap cleanup EXIT

docker compose up -d --build timescaledb brain

echo "Waiting for brain health..."
healthy=false
for _ in $(seq 1 90); do
  if curl -fsS http://localhost:4000/health >/dev/null; then
    healthy=true
    break
  fi
  sleep 2
done
if [ "$healthy" != true ]; then
  echo "Brain did not become healthy."
  exit 1
fi

python3 tests/incident_lab/generate.py \
  --all --permutations 1 --seed 20261125 \
  --duplicate-rate 0.03 --drop-optional-rate 0.05 \
  --output /tmp/observalog-e2e

cat /tmp/observalog-e2e/raw/*.log > var/logs/observalog-e2e-generated.log
expected_lines="$(wc -l < var/logs/observalog-e2e-generated.log | tr -d ' ')"
echo "Expected $expected_lines fixture records"

docker compose up -d fluent-bit

psql_query() {
  docker compose exec -T timescaledb \
    psql -U observalog -d observalog -Atqc "$1"
}

echo "Waiting for all fixture records to be durably accepted..."
accepted=false
for _ in $(seq 1 180); do
  raw_count="$(psql_query "SELECT COUNT(*) FROM lite_raw_records" 2>/dev/null || echo 0)"
  if [ "$raw_count" -eq "$expected_lines" ]; then
    accepted=true
    break
  fi
  sleep 2
done
if [ "$accepted" != true ]; then
  echo "Fluent Bit did not deliver all fixture records (expected=$expected_lines actual=$raw_count)."
  exit 1
fi

echo "All raw records accepted; waiting for durable gates to drain..."
drained=false
for _ in $(seq 1 300); do
  pending_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')" 2>/dev/null || echo 999999)"
  if [ "$pending_count" -eq 0 ]; then
    sleep 3
    stable_raw_count="$(psql_query "SELECT COUNT(*) FROM lite_raw_records")"
    stable_pending_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')")"
    if [ "$stable_raw_count" -eq "$expected_lines" ] && [ "$stable_pending_count" -eq 0 ]; then
      drained=true
      break
    fi
  fi
  sleep 2
done
if [ "$drained" != true ]; then
  echo "Gate queue did not drain within the test budget."
  exit 1
fi

raw_count="$(psql_query "SELECT COUNT(*) FROM lite_raw_records")"
index_count="$(psql_query "SELECT COUNT(*) FROM lite_event_index")"
payload_count="$(psql_query "SELECT COUNT(*) FROM lite_event_payload")"
signal_count="$(psql_query "SELECT COUNT(*) FROM lite_signals")"
finding_count="$(psql_query "SELECT COUNT(*) FROM lite_findings")"
parent_child_count="$(psql_query "SELECT COUNT(*) FROM lite_event_links WHERE relation_type='parent_child'")"
pending_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')")"
dead_count="$(psql_query "SELECT COUNT(*) FROM lite_gate_work WHERE state='dead'")"
duplicate_count="$(psql_query "SELECT COUNT(*) FROM lite_source_identity")"
raw_preserved="$(psql_query "SELECT COUNT(*) FROM lite_event_payload")"

printf 'expected=%s raw=%s indexed=%s payloads=%s signals=%s findings=%s parent_child_edges=%s duplicates_suppressed=%s pending=%s dead=%s raw_preserved=%s\n' \
  "$expected_lines" "$raw_count" "$index_count" "$payload_count" "$signal_count" \
  "$finding_count" "$parent_child_count" "$duplicate_count" "$pending_count" "$dead_count" "$raw_preserved"

test "$raw_count" -eq "$expected_lines"
test "$index_count" -gt 0
test "$payload_count" -eq "$index_count"
test "$raw_preserved" -eq "$index_count"
test "$signal_count" -gt 0
test "$finding_count" -gt 0
test "$parent_child_count" -gt 0
test "$pending_count" -eq 0
test "$dead_count" -eq 0

echo "Lite end-to-end ingest, normalization, correlation, detection and reasoning smoke test passed."
