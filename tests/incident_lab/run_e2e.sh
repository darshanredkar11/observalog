#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

export FLUENT_BIT_CONFIG=./tests/incident_lab/fluent-bit-e2e.conf
readonly PERMUTATIONS=3
readonly OUTPUT=/tmp/observalog-e2e

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

python3 tests/incident_lab/generate.py \
  --all --permutations "$PERMUTATIONS" --seed 20261125 \
  --shuffle-arrival --duplicate-rate 0.03 --drop-optional-rate 0.05 \
  --output "$OUTPUT"

# Build once; each permutation gets a fresh TimescaleDB and Fluent Bit state
# volume so the same logical source event is evaluated in that permutation's
# actual arrival order rather than being suppressed as a cross-run duplicate.
docker compose build brain

psql_query() {
  docker compose exec -T timescaledb \
    psql -U observalog -d observalog -Atqc "$1"
}

for permutation in $(seq 0 $((PERMUTATIONS - 1))); do
  suffix="$(printf '%03d' "$permutation")"
  echo "=== End-to-end permutation $suffix / $PERMUTATIONS ==="

  docker compose down -v --remove-orphans || true
  cat "$OUTPUT"/raw/*"__p${suffix}.log" > var/logs/observalog-e2e-generated.log
  expected_lines="$(wc -l < var/logs/observalog-e2e-generated.log | tr -d ' ')"
  echo "Expected $expected_lines fixture records"

  docker compose up -d timescaledb brain

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

  docker compose up -d fluent-bit

  echo "Waiting for all fixture records to be durably accepted..."
  accepted=false
  raw_count=0
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
  duplicate_count="$((raw_count - index_count))"
  raw_preserved="$(psql_query "SELECT COUNT(*) FROM lite_event_payload p JOIN lite_raw_records r ON r.id=p.raw_id WHERE p.raw_text IS NOT DISTINCT FROM r.raw_text")"

  printf 'permutation=%s expected=%s raw=%s indexed=%s payloads=%s signals=%s findings=%s parent_child_edges=%s duplicates_suppressed=%s pending=%s dead=%s raw_preserved=%s\n' \
    "$suffix" "$expected_lines" "$raw_count" "$index_count" "$payload_count" "$signal_count" \
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

  docker compose down -v --remove-orphans
done

echo "Lite end-to-end passed for all $PERMUTATIONS shuffled arrival permutations."
