#!/usr/bin/env python3
"""Gate 5.2-E cooperative research evaluator.

The Rust binary performs specialist analysis, evidence verification, and graph
fallback. Python generates/mutates fixture inputs and scores predictions only.
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


def rust_predict(binary: str, events: list[dict]) -> dict:
    completed = subprocess.run(
        [binary],
        input=json.dumps(events, separators=(",", ":")),
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Rust cooperative evaluator failed ({completed.returncode}): {completed.stderr}")
    try:
        result = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Rust cooperative evaluator returned invalid JSON: {completed.stdout[:500]}") from error
    if not isinstance(result, dict) or "abstained" not in result:
        raise RuntimeError("Rust cooperative evaluator returned an invalid diagnosis object")
    return result


def map_target(primary_entity, category, events):
    if category in {"healthy", "multiple_incidents", "multiple_independent_events", "external_dependency"} or not primary_entity:
        return None
    target = primary_entity.lower().strip()
    matches = set()
    for event in events:
        service = event["service"].lower()
        if target == service or target.startswith(service + "@") or target.startswith(service + "/"):
            matches.add(event["service"])
        instance = event.get("instance")
        if instance and target == instance.lower():
            matches.add(event["service"])
    return next(iter(matches)) if len(matches) == 1 else None


def prediction_matches(target: str | None, target_type: str | None, primary_entity: str | None) -> bool:
    if not target or not target_type or not primary_entity:
        return False
    expected = primary_entity.lower().strip()
    actual = target.lower().strip()
    if target_type == "instance":
        return actual == expected
    if target_type in {"service", "component"}:
        return expected == actual or expected.startswith(actual + "@") or expected.startswith(actual + "/") or expected.startswith(actual + ":")
    return False


def prepare_events(factory, permutation: int):
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
            raise AssertionError("every fixture event must have a stable event_id")
        if event_id in unique and unique[event_id] != event:
            raise AssertionError(f"duplicate event_id {event_id} has conflicting payloads")
        unique.setdefault(event_id, event)
    return list(unique.values()), truth


def run_case(name, factory, permutation, binary):
    events, truth = prepare_events(factory, permutation)
    result = rust_predict(binary, events)
    target = map_target(truth.get("primary_entity"), truth.get("category"), events)
    primary = result.get("primary_target")
    target_type = result.get("primary_target_type")
    exact_target_match = prediction_matches(primary, target_type, truth.get("primary_entity"))
    if target_type == "instance":
        service_level_match = any(
            event.get("service") == target and event.get("instance") == primary
            for event in events
        )
    else:
        service_level_match = bool(primary and target and str(primary).lower() == str(target).lower())
    granularity_gap = bool(service_level_match and not exact_target_match)
    event_ids = {str(event.get("event_id")) for event in events}
    evidence_failures = [
        evidence_id
        for hypothesis in result.get("accepted_specialist_hypotheses", [])
        for evidence_id in hypothesis.get("evidence_ids", [])
        if str(evidence_id) not in event_ids
    ]
    return {
        "scenario": name,
        "permutation": permutation,
        "truth_category": truth.get("category"),
        "truth_primary_entity": truth.get("primary_entity"),
        "target_service": target,
        "primary_target": primary,
        "primary_target_type": target_type,
        "decision_source": result.get("decision_source"),
        "accepted_specialist_count": len(result.get("accepted_specialist_hypotheses", [])),
        "rejected_specialist_count": result.get("rejected_specialist_hypotheses", 0),
        "abstained": bool(result.get("abstained")),
        "localizable": target is not None,
        "diagnosis_top1_correct": bool(target is not None and exact_target_match),
        "exact_target_correct": bool(target is not None and exact_target_match),
        "service_localization_correct": bool(target is not None and service_level_match),
        "target_granularity_gap": granularity_gap,
        "false_attribution": bool(primary and target is None),
        "confident_wrong_localizable": bool(primary and target is not None and not service_level_match),
        "evidence_id_failures": evidence_failures,
        "event_count": len(events),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, help="compiled Rust gate52_cooperative_eval binary")
    parser.add_argument("--generator", default="tests/incident_lab/generate.py")
    parser.add_argument("--output", default="gate52-cooperative-evaluation.json")
    args = parser.parse_args()

    generator = load_generator(args.generator)
    results = []
    for name, factory in sorted(generator.CASES.items()):
        for permutation in range(PERMUTATIONS):
            results.append(run_case(name, factory, permutation, args.binary))

    local = [row for row in results if row["localizable"]]
    nonlocal_rows = [row for row in results if not row["localizable"]]
    count = len(results)
    correct = sum(row["diagnosis_top1_correct"] for row in local)
    service_correct = sum(row["service_localization_correct"] for row in local)
    granularity_gaps = sum(row["target_granularity_gap"] for row in local)
    coverage = sum(not row["abstained"] for row in results) / count if count else 0.0
    false_attributions = sum(row["false_attribution"] for row in nonlocal_rows)
    confidently_wrong = sum(row["confident_wrong_localizable"] for row in local)
    evidence_failures = sum(len(row["evidence_id_failures"]) for row in results)

    report = {
        "gate": "5.2-E cooperative specialists + graph fallback",
        "status": "PASS" if (
            count == len(generator.CASES) * PERMUTATIONS
            and evidence_failures == 0
            and service_correct == len(local)
            and false_attributions == 0
            and confidently_wrong == 0
        ) else "FAIL",
        "execution": "actual Rust gate52_cooperative_eval; Python only generates/mutates fixtures and scores results",
        "seed": SEED,
        "scenario_count": len(generator.CASES),
        "permutations_per_scenario": PERMUTATIONS,
        "stream_count": count,
        "localizable_streams": len(local),
        "accepted_diagnoses": sum(not row["abstained"] for row in results),
        "coverage": coverage,
        "exact_target_top1_accuracy_on_localizable": correct / len(local) if local else None,
        "service_localization_top1_accuracy_on_localizable": service_correct / len(local) if local else None,
        "target_granularity_gaps": granularity_gaps,
        "false_attributions_nonlocalizable": false_attributions,
        "false_attribution_rate_nonlocalizable": false_attributions / len(nonlocal_rows) if nonlocal_rows else 0.0,
        "confidently_wrong_localizable": confidently_wrong,
        "confidently_wrong_rate_localizable": confidently_wrong / len(local) if local else None,
        "evidence_id_failures": evidence_failures,
        "specialist_decisions": sum(row["decision_source"] == "specialist" for row in results),
        "graph_fallback_decisions": sum(row["decision_source"] == "graph" for row in results),
        "abstentions": sum(row["abstained"] for row in results),
        "per_scenario": results,
        "limitations": [
            "Uses normalized generator event objects, not the production collector/parser/database path.",
            "The four specialists are conventional deterministic/statistical algorithms, not trained ML models.",
            "Evidence verification is a research boundary and is not wired into production findings.",
            "Synthetic incident-corpus results are not production accuracy claims.",
        ],
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in report.items() if key != "per_scenario"}, indent=2))
    print(json.dumps({
        row["scenario"]: {
            "truth": row["truth_primary_entity"],
            "target": row["primary_target"],
            "target_type": row["primary_target_type"],
            "source": row["decision_source"],
            "abstained": row["abstained"],
            "exact_target_correct": row["exact_target_correct"],
            "service_localization_correct": row["service_localization_correct"],
            "granularity_gap": row["target_granularity_gap"],
        }
        for row in results if row["permutation"] == 0
    }, indent=2))
    if report["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
