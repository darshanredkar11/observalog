#!/usr/bin/env python3
"""Evaluate Rust Gate 5.2 specialist binaries on the seeded Incident Lab fixtures.

Python only generates/mutates input and calculates metrics after Rust predictions.
The diagnostic rules themselves execute in gate52_specialist_eval (Rust).
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import random
import subprocess
from collections import defaultdict
from pathlib import Path

SEED = 20261127
PERMUTATIONS = 3
OPTIONAL_FIELDS = ("trace_id", "span_id", "parent_span_id", "request_id", "version", "duration_ms")

SPECIALISTS = {
    "database-deadlock-signature": "database_concurrency",
    "auth-cascade-signature": "configuration_or_authentication",
    "message-idempotency-invariant": "async_delivery_and_idempotency",
    "resource-theil-sen-trend": "resource_exhaustion",
}


def load_generator(path: str):
    spec = importlib.util.spec_from_file_location("incident_lab_generator", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load generator at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def target_matches(predicted: str, primary_entity: str | None) -> bool:
    if not primary_entity:
        return False
    target = primary_entity.strip().lower()
    candidate = predicted.strip().lower()
    return target == candidate or target.startswith(candidate + "/") or target.startswith(candidate + "@") or target.startswith(candidate + ":")


def rust_predict(binary: str, events: list[dict]) -> list[dict]:
    completed = subprocess.run(
        [binary],
        input=json.dumps(events, separators=(",", ":")),
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Rust specialist runner failed ({completed.returncode}): {completed.stderr}")
    try:
        predictions = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Rust runner returned invalid JSON: {completed.stdout[:500]}") from error
    if not isinstance(predictions, list):
        raise RuntimeError("Rust runner output must be a JSON array")
    return predictions


def prepare_events(factory, permutation: int) -> tuple[list[dict], dict]:
    events, truth = factory()
    rng = random.Random(SEED + permutation * 1009)
    mutated = []
    for original in events:
        event = dict(original)
        event["attrs"] = dict(event.get("attrs") or {})
        for field in OPTIONAL_FIELDS:
            if event.get(field) is not None and rng.random() < 0.08:
                event[field] = None
        mutated.append(event)

    originals = list(mutated)
    for event in originals:
        if rng.random() < 0.04:
            mutated.append(dict(event, attrs=dict(event.get("attrs") or {})))
    rng.shuffle(mutated)

    unique = {}
    for event in mutated:
        event_id = event.get("event_id")
        if not event_id:
            raise AssertionError("every incident fixture event must have a stable event_id")
        if event_id in unique and unique[event_id] != event:
            raise AssertionError(f"duplicate event_id {event_id} has conflicting payloads")
        unique.setdefault(event_id, event)
    return list(unique.values()), truth


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, help="path to compiled Rust gate52_specialist_eval binary")
    parser.add_argument("--generator", default="tests/incident_lab/generate.py")
    parser.add_argument("--output", default="gate52-specialist-evaluation.json")
    args = parser.parse_args()

    generator = load_generator(args.generator)
    stats = {name: {"tp": 0, "fp": 0, "fn": 0, "expected": 0, "predictions": 0} for name in SPECIALISTS}
    case_results = []
    streams = 0
    evidence_integrity_failures = 0

    for scenario, factory in sorted(generator.CASES.items()):
        for permutation in range(PERMUTATIONS):
            events, truth = prepare_events(factory, permutation)
            predictions = rust_predict(args.binary, events)
            streams += 1
            event_ids = {str(event.get("event_id")) for event in events}
            for prediction in predictions:
                evidence = set(map(str, prediction.get("evidence_ids", [])))
                if not evidence or not evidence.issubset(event_ids):
                    evidence_integrity_failures += 1

            expected_specialist = next(
                (name for name, category in SPECIALISTS.items() if truth.get("category") == category),
                None,
            )
            expected_target = truth.get("primary_entity")
            observed = []
            for prediction in predictions:
                name = prediction.get("specialist")
                if name not in stats:
                    continue
                observed.append({
                    "specialist": name,
                    "target_type": prediction.get("target_type"),
                    "target": prediction.get("target"),
                    "evidence_ids": prediction.get("evidence_ids", []),
                    "matches_ground_truth": bool(
                        name == expected_specialist
                        and target_matches(str(prediction.get("target", "")), expected_target)
                    ),
                })

            for name in SPECIALISTS:
                predicted = [p for p in observed if p["specialist"] == name]
                expected = name == expected_specialist
                stats[name]["expected"] += int(expected)
                stats[name]["predictions"] += len(predicted)
                correct = any(p["matches_ground_truth"] for p in predicted)
                if expected and correct:
                    stats[name]["tp"] += 1
                elif expected:
                    stats[name]["fn"] += 1
                elif predicted:
                    stats[name]["fp"] += len(predicted)

            case_results.append({
                "scenario": scenario,
                "permutation": permutation,
                "truth_category": truth.get("category"),
                "truth_primary_entity": expected_target,
                "predictions": observed,
                "event_count": len(events),
            })

    summary = {}
    for name, values in stats.items():
        tp, fp, fn = values["tp"], values["fp"], values["fn"]
        summary[name] = {
            **values,
            "precision": tp / (tp + fp) if tp + fp else 0.0,
            "recall": tp / (tp + fn) if tp + fn else 0.0,
            "f1": (2 * tp / (2 * tp + fp + fn)) if 2 * tp + fp + fn else 0.0,
        }

    report = {
        "gate": "5.2 conventional specialist research",
        "status": "PASS" if streams == len(generator.CASES) * PERMUTATIONS and evidence_integrity_failures == 0 else "FAIL",
        "execution": "actual Rust specialist binary; Python only generates fixtures and scores returned predictions",
        "seed": SEED,
        "scenario_count": len(generator.CASES),
        "permutations_per_scenario": PERMUTATIONS,
        "stream_count": streams,
        "evidence_integrity_failures": evidence_integrity_failures,
        "specialist_metrics": summary,
        "per_scenario": case_results,
        "limitations": [
            "Uses normalized generator event objects, not the production collector/parser/database path.",
            "Four targeted synthetic fault families only establish whether the specialist can detect these signatures.",
            "No learned model is trained in this experiment.",
            "Scores are hypothesis matching, not calibrated causal probabilities or product accuracy claims.",
        ],
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k != "per_scenario"}, indent=2))
    if report["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
