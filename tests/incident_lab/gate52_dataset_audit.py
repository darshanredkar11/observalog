#!/usr/bin/env python3
"""Gate 5.2-A: audit diagnostic scenario generator shape and raw/evaluator separation."""
import argparse, importlib.util, json
from pathlib import Path

FORBIDDEN_RAW = {"scenario", "ground_truth", "label", "primary_cause", "primary_entity",
                 "expected_observations", "must_not_correlate", "symptoms", "amplifiers"}

def load(path):
    spec = importlib.util.spec_from_file_location("incident_lab_generator", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def audit_case(name, factory):
    events, truth = factory()
    findings=[]
    event_ids=[]
    for i, event in enumerate(events):
        forbidden=FORBIDDEN_RAW.intersection(event)
        if forbidden:
            findings.append({"event_index":i,"forbidden_fields":sorted(forbidden)})
        event_ids.append(event.get("event_id"))
    required_truth={"category","primary_cause","primary_entity"}
    missing=required_truth-set(truth)
    return {"scenario":name,"event_count":len(events),"unique_event_ids":len(set(event_ids)),
            "duplicate_event_ids":len(event_ids)-len(set(event_ids)),
            "truth_schema_missing":sorted(missing),"raw_leakage":findings,
            "has_symptom_or_amplifier_annotations":bool(truth.get("symptom_services") or truth.get("amplifiers"))}

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--generator",default="tests/incident_lab/generate.py")
    p.add_argument("--output",default="gate52-audit.json")
    args=p.parse_args()
    g=load(args.generator)
    results=[audit_case(name,factory) for name,factory in sorted(g.CASES.items())]
    failures=[r for r in results if r["raw_leakage"] or r["truth_schema_missing"] or r["unique_event_ids"]!=r["event_count"]]
    report={"gate":"5.2-A","purpose":"dataset-shape/leakage audit only; no model score","scenario_count":len(results),
            "event_count":sum(r["event_count"] for r in results),"status":"FAIL" if failures else "PASS",
            "failures":failures,"scenarios":results}
    Path(args.output).write_text(json.dumps(report,indent=2),encoding="utf-8")
    print(json.dumps(report,indent=2))
    if failures: raise SystemExit(1)
if __name__=="__main__": main()
