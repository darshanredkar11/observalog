#!/usr/bin/env python3
"""Leakage audit for the seeded mega-corpus used in ML experiments.

The audit checks raw/evaluator separation, label-bearing text and identifiers,
ordinal feature leakage, duplicate integrity, and randomized scenario scheduling.
"""
from __future__ import annotations
import argparse
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path


def load_generator(path: str):
    spec = importlib.util.spec_from_file_location("mega_corpus", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load generator: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def strings(value):
    if isinstance(value, dict):
        for key, child in value.items():
            yield str(key)
            yield from strings(child)
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)
    elif isinstance(value, str):
        yield value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--generator", default="tests/incident_lab/mega_corpus.py")
    parser.add_argument("--count", type=int, default=6200)
    parser.add_argument("--seed", type=int, default=20261127)
    parser.add_argument("--duplicate-rate", type=float, default=0.04)
    parser.add_argument("--output", default="mega-corpus-leakage-audit.json")
    args = parser.parse_args()

    generator = load_generator(args.generator)
    if args.count < len(generator.CASES) * 2:
        parser.error(f"count must cover at least two complete case groups ({len(generator.CASES) * 2})")

    with tempfile.TemporaryDirectory(prefix="observalog-leakage-") as tmp:
        subprocess.run([
            sys.executable, args.generator,
            "--count", str(args.count),
            "--seed", str(args.seed),
            "--duplicate-rate", str(args.duplicate_rate),
            "--output", tmp,
        ], check=True, capture_output=True, text=True)
        root = Path(tmp)
        raw_records = [
            json.loads(line)
            for line in (root / "raw" / "corpus.jsonl").read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        label_doc = json.loads((root / "ground-truth" / "labels.json").read_text(encoding="utf-8"))

    labels = label_doc["labels"]
    label_by_id = {row["event_id"]: row["scenario"] for row in labels}
    seen_payloads = {}
    conflicting_duplicates = []
    raw_ids = set()
    raw_label_tokens = []
    scenario_tokens = [name.lower() for name in generator.CASES]
    message_label_hits = []
    identifier_pattern_failures = []
    forbidden_fields = {"scenario", "ground_truth", "label", "expected_class", "primary_cause",
                        "primary_entity", "expected_observations", "must_not_correlate"}

    for index, event in enumerate(raw_records):
        event_id = event.get("event_id")
        raw_ids.add(event_id)
        if event_id in seen_payloads and seen_payloads[event_id] != event:
            conflicting_duplicates.append(event_id)
        else:
            seen_payloads.setdefault(event_id, event)

        keys = set(event)
        if keys & forbidden_fields:
            raw_label_tokens.append({"record": index, "fields": sorted(keys & forbidden_fields)})

        serialized = json.dumps(event, sort_keys=True, ensure_ascii=False).lower()
        direct_tokens = [token for token in scenario_tokens if token in serialized]
        if direct_tokens:
            raw_label_tokens.append({"record": index, "scenario_tokens": direct_tokens})

        message = str(event.get("message") or "").lower()
        direct_message_tokens = [token for token in scenario_tokens if token in message]
        if direct_message_tokens:
            message_label_hits.append({"record": index, "scenario_tokens": direct_message_tokens})

        for field, prefix in (("event_id", "evt"), ("request_id", "req"), ("entity_id", "biz")):
            value = event.get(field)
            if value is not None and not re.fullmatch(prefix + r"-[0-9a-f]{16}", str(value)):
                identifier_pattern_failures.append({"record": index, "field": field, "value": value})
        trace = event.get("trace_id")
        if trace is not None and not re.fullmatch(r"trc-[0-9a-f]{16}", str(trace)):
            identifier_pattern_failures.append({"record": index, "field": "trace_id", "value": trace})

    label_ids = set(label_by_id)
    raw_unique_ids = set(seen_payloads)
    missing_from_raw = sorted(label_ids - raw_unique_ids)
    unknown_in_raw = sorted(raw_unique_ids - label_ids)

    case_count = len(generator.CASES)
    ordinal_hits = sum(row["scenario"] == generator.CASES[i % case_count] for i, row in enumerate(labels))
    ordinal_accuracy = ordinal_hits / len(labels) if labels else 0.0

    # Each complete generator group should contain one sample from each scenario;
    # order inside the group is shuffled to prevent periodic temporal labels.
    group_errors = []
    by_group = {}
    for row in labels:
        by_group.setdefault(row["group"], []).append(row["scenario"])
    for group, names in sorted(by_group.items()):
        if len(names) == case_count and Counter(names) != Counter(generator.CASES):
            group_errors.append(group)

    failures = {
        "raw_label_tokens": raw_label_tokens[:30],
        "message_label_hits": message_label_hits[:30],
        "identifier_pattern_failures": identifier_pattern_failures[:30],
        "conflicting_duplicate_ids": conflicting_duplicates[:30],
        "missing_label_ids_in_raw": missing_from_raw[:30],
        "unknown_raw_ids": unknown_in_raw[:30],
        "ordinal_schedule_accuracy": ordinal_accuracy,
        "group_composition_errors": group_errors[:30],
    }
    failed = bool(
        raw_label_tokens or message_label_hits or identifier_pattern_failures
        or conflicting_duplicates or missing_from_raw or unknown_in_raw
        or ordinal_accuracy > 0.20 or group_errors
    )
    report = {
        "audit": "mega-corpus label leakage and duplicate integrity",
        "status": "FAIL" if failed else "PASS",
        "seed": args.seed,
        "base_events": len(labels),
        "raw_records_including_duplicates": len(raw_records),
        "unique_raw_event_ids": len(raw_unique_ids),
        "duplicates_in_raw": len(raw_records) - len(raw_unique_ids),
        "scenario_count": case_count,
        "ordinal_schedule_accuracy": ordinal_accuracy,
        "random_schedule_baseline": 1.0 / case_count,
        "checks": {
            "scenario_label_strings_absent_from_raw": not raw_label_tokens,
            "scenario_label_strings_absent_from_messages": not message_label_hits,
            "opaque_ids_are_not_ordinal": not identifier_pattern_failures,
            "duplicates_are_exact_copies": not conflicting_duplicates,
            "raw_and_label_ids_match": not missing_from_raw and not unknown_in_raw,
            "scenario_order_is_not_periodic": ordinal_accuracy <= 0.20,
            "each_complete_group_has_one_of_each_scenario": not group_errors,
        },
        "failures": failures,
        "limitations": [
            "This is a leakage/integrity audit, not proof that all semantic shortcuts are absent.",
            "Operational attributes such as HTTP status, error code, heap, retries and tenant cohort remain intentionally predictive features.",
            "Any ML feature pipeline must exclude event IDs, raw timestamp sequence index, and ground-truth metadata from model features.",
        ],
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    if failed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
