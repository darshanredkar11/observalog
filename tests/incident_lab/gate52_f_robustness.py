#!/usr/bin/env python3
"""Gate 5.2-F robustness: Rust cooperative evaluator under repeated mutation.

Labels are joined only after Rust predictions. This measures invariance across
permutations, optional-field loss, and exact duplicate delivery.
"""
from __future__ import annotations
import argparse, importlib.util, json, random, subprocess
from pathlib import Path

FIELDS=("trace_id","span_id","parent_span_id","request_id","version","duration_ms")
SEED=20261128
PROFILES={"clean":{"drop":0.0,"duplicate":0.0},"moderate":{"drop":0.08,"duplicate":0.04},"heavy":{"drop":0.20,"duplicate":0.10}}
REPEATS=10

def load(path):
 spec=importlib.util.spec_from_file_location("generator",path)
 module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module

def rust(binary, events):
 p=subprocess.run([binary],input=json.dumps(events,separators=(",",":")),text=True,capture_output=True)
 if p.returncode: raise RuntimeError(f"Rust evaluator exit {p.returncode}: {p.stderr}")
 j=json.loads(p.stdout)
 if not isinstance(j,dict) or "abstained" not in j: raise RuntimeError("Malformed Rust output")
 return j

def matches(primary, typ, truth):
 if not primary or not typ or not truth: return False
 exp=truth.lower().strip(); actual=primary.lower().strip()
 if typ=="instance": return actual==exp
 if typ in ("service","component"): return exp==actual or exp.startswith(actual+"@") or exp.startswith(actual+"/") or exp.startswith(actual+":")
 return False

def mutate(events, seed, drop, duplicate):
 rng=random.Random(seed); out=[]
 for original in events:
  e=dict(original); attrs=dict(e.get("attrs") or {})
  for f in FIELDS:
   if e.get(f) is not None and rng.random()<drop:
    e[f]=None
  e["attrs"]=attrs
  out.append(e)
 original=list(out)
 for e in original:
  if rng.random()<duplicate: out.append(dict(e,attrs=dict(e.get("attrs") or {})))
 rng.shuffle(out)
 unique={}
 for e in out:
  eid=e.get("event_id")
  if not eid: raise ValueError("missing event_id")
  if eid in unique and unique[eid]!=e: raise ValueError(f"event id conflict: {eid}")
  unique.setdefault(eid,e)
 return list(unique.values())

def main():
 ap=argparse.ArgumentParser()
 ap.add_argument("--binary",required=True)
 ap.add_argument("--generator",default="tests/incident_lab/generate.py")
 ap.add_argument("--output",default="gate52-f-robustness.json")
 args=ap.parse_args(); g=load(args.generator)
 rows=[]
 for profile,params in PROFILES.items():
  for scenario,factory in sorted(g.CASES.items()):
   for rep in range(REPEATS):
    base,truth=factory()
    events=mutate(base,SEED+rep*991+len(rows)*31,params["drop"],params["duplicate"])
    prediction=rust(args.binary,events)
    target=truth.get("primary_entity")
    primary=prediction.get("primary_target"); typ=prediction.get("primary_target_type")
    event_ids={str(e.get("event_id")) for e in events}
    evidence_bad=[eid for h in prediction.get("accepted_specialist_hypotheses",[]) for eid in h.get("evidence_ids",[]) if str(eid) not in event_ids]
    localizable=truth.get("category") not in {"healthy","multiple_incidents","multiple_independent_events","external_dependency"} and bool(target)
    rows.append({"profile":profile,"scenario":scenario,"rep":rep,"target":target,"prediction":primary,"target_type":typ,
     "abstained":bool(prediction.get("abstained")),"localizable":localizable,
     "correct":bool(localizable and matches(primary,typ,target)),
     "false_attribution":bool(primary and not localizable),
     "confident_wrong":bool(primary and localizable and not matches(primary,typ,target)),
     "evidence_failures":evidence_bad,"source":prediction.get("decision_source")})
 summary={}
 failures=[]
 for profile in PROFILES:
  subset=[r for r in rows if r["profile"]==profile]
  loc=[r for r in subset if r["localizable"]]
  nonloc=[r for r in subset if not r["localizable"]]
  wrong=sum(r["confident_wrong"] for r in loc)
  false=sum(r["false_attribution"] for r in nonloc)
  abst=sum(r["abstained"] for r in subset)
  correct=sum(r["correct"] for r in loc)
  ev=sum(len(r["evidence_failures"]) for r in subset)
  summary[profile]={"runs":len(subset),"localizable_runs":len(loc),"correct_localizations":correct,
   "localizable_accuracy":correct/len(loc) if loc else None,"coverage":(len(subset)-abst)/len(subset) if subset else 0,
   "false_attributions":false,"confidently_wrong":wrong,"evidence_id_failures":ev}
  if false or wrong or ev: failures.append(profile)
 report={"gate":"5.2-F","status":"PASS" if not failures else "FAIL","seed":SEED,
  "repeats_per_scenario_per_profile":REPEATS,"scenario_count":len(g.CASES),"profiles":PROFILES,
  "summary":summary,"per_run":rows,"limitations":["Synthetic incident fixtures only; not production telemetry.","Passing means robustness under these specified mutations, not causal proof or field accuracy."]}
 Path(args.output).write_text(json.dumps(report,indent=2)+"\\n")
 print(json.dumps({"gate":report["gate"],"status":report["status"],"summary":summary},indent=2))
 if failures: raise SystemExit(1)
if __name__=="__main__": main()
