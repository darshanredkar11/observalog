#!/usr/bin/env python3
"""Gate 5.2-B corpus evaluator: invoke the actual Rust graph_diagnosis::rank.

Python generates/mutates fixtures and scores returned predictions only. It does not
contain a second implementation of the diagnostic algorithm.
"""
from __future__ import annotations
import argparse
import importlib.util
import json
import random
import subprocess
from pathlib import Path

SEED = 20261127
PERMUTATIONS = 3
OPTIONAL_FIELDS = ("trace_id", "span_id", "parent_span_id", "request_id", "version", "duration_ms")


def load_generator(path: str):
    spec = importlib.util.spec_from_file_location("incident_lab_generator", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load generator at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rust_rank(binary: str, events: list[dict]) -> dict:
    completed = subprocess.run(
        [binary],
        input=json.dumps(events, separators=(",", ":")),
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Rust graph evaluator failed ({completed.returncode}): {completed.stderr}")
    try:
        result = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Rust evaluator returned invalid JSON: {completed.stdout[:500]}") from error
    if not isinstance(result, dict) or not isinstance(result.get("candidates"), list):
        raise RuntimeError("Rust evaluator returned an invalid diagnosis object")
    return result


def map_target(primary_entity, category, events):
    """Conservative target mapping. Ground truth is not passed to the Rust scorer."""
    if category in {"healthy", "multiple_incidents", "multiple_independent_events", "external_dependency"} or not primary_entity:
        return None
    text = primary_entity.lower().strip()
    matches = set()
    for event in events:
        service = event["service"].lower()
        if text == service or text.startswith(service + "@") or text.startswith(service + "/"):
            matches.add(event["service"])
        instance = event.get("instance")
        if instance and text == instance.lower():
            matches.add(event["service"])
    return next(iter(matches)) if len(matches) == 1 else None


def run_case(name, factory, seed, permutation, binary):
    events, truth = factory()
    rng = random.Random(seed + permutation * 1009)
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
            mutated.append(dict(event, attrs=dict(event.get("attrs", {}))))
    rng.shuffle(mutated)

    unique = {}
    for event in mutated:
        event_id = event.get("event_id")
        if not event_id:
            raise AssertionError("every fixture event must have a stable event_id")
        if event_id in unique and unique[event_id] != event:
            raise AssertionError(f"duplicate event_id {event_id} has conflicting payloads")
        unique.setdefault(event_id, event)
    scored_events = list(unique.values())

    # The actual production-language ranker sees the event set before labels are joined.
    diagnosis = rust_rank(binary, scored_events)
    ranked = diagnosis["candidates"]
    winner = diagnosis.get("likely_initiator")
    target = map_target(truth.get("primary_entity"), truth.get("category"), events)
    top3 = [candidate["service"] for candidate in ranked[:3]]
    target_rank = next((i + 1 for i, service in enumerate(top3) if service == target), None) if target else None
    localizable = target is not None
    correct = bool(localizable and winner == target)
    return {
        "scenario": name,
        "permutation": permutation,
        "target_service": target,
        "winner": winner,
        "ranked_candidates": ranked[:5],
        "localizable": localizable,
        "candidate_top1_correct": bool(localizable and ranked and ranked[0]["service"] == target),
        "diagnosis_top1_correct": correct,
        "top1_correct": correct,
        "top3_correct": bool(localizable and target in top3),
        "mrr": 1.0 / target_rank if target_rank else 0.0,
        "abstained": bool(diagnosis["abstained"]),
        "false_attribution": bool(winner and not localizable),
        "confident_wrong_localizable": bool(winner and localizable and winner != target),
        "truth_category": truth.get("category"),
        "truth_primary_entity": truth.get("primary_entity"),
        "note": diagnosis.get("note"),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, help="compiled gate52_graph_eval Rust binary")
    parser.add_argument("--generator", default="tests/incident_lab/generate.py")
    parser.add_argument("--output", default="gate52-b-evaluation.json")
    args = parser.parse_args()

    generator = load_generator(args.generator)
    results = []
    for name, factory in sorted(generator.CASES.items()):
        for permutation in range(PERMUTATIONS):
            results.append(run_case(name, factory, SEED, permutation, args.binary))

    local = [result for result in results if result["localizable"]]
    count = len(results)
    by_scenario = {}
    for name in sorted(generator.CASES):
        rows = [result for result in results if result["scenario"] == name]
        by_scenario[name] = {
            "target_service": rows[0]["target_service"],
            "top1_correct_all_permutations": all(row["top1_correct"] for row in rows) if rows[0]["localizable"] else None,
            "top3_correct_all_permutations": all(row["top3_correct"] for row in rows) if rows[0]["localizable"] else None,
            "winners": [row["winner"] for row in rows],
            "abstentions": sum(row["abstained"] for row in rows),
            "false_attributions": sum(row["false_attribution"] for row in rows),
            "confident_wrong": sum(row["confident_wrong_localizable"] for row in rows),
        }

    report = {
        "gate": "5.2-B",
        "status": "PASS" if count == len(generator.CASES) * PERMUTATIONS else "FAIL",
        "evaluator": "actual Rust graph_diagnosis::rank via gate52_graph_eval; Python only mutates fixtures and scores predictions",
        "seed": SEED,
        "permutations_per_scenario": PERMUTATIONS,
        "scenario_count": len(generator.CASES),
        "stream_count": count,
        "localizable_streams": len(local),
        "candidate_top1_accuracy_on_localizable": sum(row["candidate_top1_correct"] for row in local) / len(local) if local else None,
        "diagnosis_top1_accuracy_on_localizable": sum(row["diagnosis_top1_correct"] for row in local) / len(local) if local else None,
        "top3_accuracy_on_localizable": sum(row["top3_correct"] for row in local) / len(local) if local else None,
        "mrr_on_localizable": sum(row["mrr"] for row in local) / len(local) if local else None,
        "coverage": sum(not row["abstained"] for row in results) / count if count else 0.0,
        "abstention_rate": sum(row["abstained"] for row in results) / count if count else 0.0,
        "false_attribution_rate_nonlocalizable": (
            sum(row["false_attribution"] for row in results if not row["localizable"])
            / sum(not row["localizable"] for row in results)
            if any(not row["localizable"] for row in results) else 0.0
        ),
        "confident_wrong_localizable_rate": (
            sum(row["confident_wrong_localizable"] for row in local) / len(local) if local else None
        ),
        "scenario_summary": by_scenario,
        "per_scenario": results,
        "limitations": [
            "The Rust ranker is evaluated on normalized generator event objects, not raw collector/parser/database output.",
            "The parent-child edge adapter in the evaluator is a test adapter; runtime integration remains a separate gate.",
            "Ground truth is consulted only after the Rust prediction is returned.",
            "Scores are synthetic-corpus metrics, not production accuracy claims.",
        ],
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in report.items() if key not in ("per_scenario", "scenario_summary")}, indent=2))
    print(json.dumps(report["scenario_summary"], indent=2))
    if report["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
