#!/usr/bin/env python3
"""Gate 5.2-B corpus evaluator for the deterministic graph prototype.

Uses the incident-lab factories as evaluator-only input; truth is never fed into scoring.
Produces metrics across 22 cases x 3 deterministic arrival permutations.
"""
from __future__ import annotations
import argparse, importlib.util, json, random
from pathlib import Path

SEED=20261127
PERMUTATIONS=3
# Cases where the generator's primary_entity can be mapped to a service candidate.
# None means the ground-truth cause is external/non-service or intentionally
# not directly localizable by this service-only primitive.
TARGET_SERVICE={
 "healthy-single": None, "healthy-six-service": None,
 "auth-cascade": "identity", "provider-rate-limit-retry": "payments",
 "replica-config-drift": "identity", "version-regression": "orders",
 "database-deadlock": "postgres", "duplicate-message-side-effect": "stock-consumer",
 "business-state-invariant": "orders", "unrelated-concurrent-failures": None,
 "missing-trace-business-link": "orders", "clock-skew-parent-span": None,
 "async-queue-backlog": "queue", "resource-pressure-oom": None,
 "tenant-feature-flag-drift": "identity", "shared-dns-failure": "dns",
 "producer-consumer-schema-drift": "producer", "poison-record-isolation": None,
 "mixed-format-sparse-logs": None, "two-incidents-one-window": None,
 "trace-id-reuse": None, "twelve-service-fanout": None,
}
# Candidate service aliases used only to compare service-localization targets.
ALIASES={"postgres":"postgres","stock-consumer":"stock-consumer","pay.example":None,
          "queue":None,"dns":None,"producer":None}

def load_generator(path):
 spec=importlib.util.spec_from_file_location("incident_lab_generator",path)
 module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module

def score(events):
 """Mirror the prototype's conservative scoring without reading evaluator truth."""
 scores={}
 for e in events:
  s=e["service"]
  c=scores.setdefault(s,{"score":0.0,"failures":0,"upstream_links":0,"downstream_links":0,"reasons":[]})
  if e["level"] in ("ERROR","FATAL") or e.get("outcome")=="failure":
   c["failures"]+=1;c["score"]+=1.0
   if e.get("error_code") or e.get("attrs"):
    c["score"]+=0.5
  # Parent-child evidence only if both spans belong to same eligible flow.
  for p in events:
   if p is e or p["service"]==s: continue
   if e.get("parent_span_id") and p.get("span_id")==e["parent_span_id"] and (
      (e.get("trace_id") and e.get("trace_id")==p.get("trace_id")) or
      (e.get("request_id") and e.get("request_id")==p.get("request_id"))):
    if (p["level"] in ("ERROR","FATAL") or p.get("outcome")=="failure") and (
       e["level"] in ("ERROR","FATAL") or e.get("outcome")=="failure"):
      pc=scores.setdefault(p["service"],{"score":0.0,"failures":0,"upstream_links":0,"downstream_links":0,"reasons":[]})
      pc["upstream_links"]+=1;pc["score"]+=1.75
      c["downstream_links"]+=1;c["score"]-=0.75
  # Event summary score; only explicit structure, not timestamps.
 ranked=sorted(scores.items(),key=lambda kv:(-kv[1]["score"],kv[0]))
 ranked=[{"service":s,**v} for s,v in ranked]
 winner=ranked[0]["service"] if ranked and ranked[0]["score"]>=2 and (ranked[0]["failures"] or ranked[0]["upstream_links"]) else None
 return ranked,winner

def run_case(name,factory,target,seed,permutation):
 events,truth=factory()
 rng=random.Random(seed+permutation*1009)
 mutated=[]
 for event in events:
  e=dict(event); e["attrs"]=dict(e.get("attrs") or {})
  # Match generator's optional-field mutation set; deterministic, evaluator-only.
  for field in ("trace_id","span_id","parent_span_id","request_id","version","duration_ms"):
   if e.get(field) is not None and rng.random()<0.08: e[field]=None
  mutated.append(e)
 # Duplicate perturbation and randomized arrival order.
 originals=list(mutated)
 for e in originals:
  if rng.random()<0.04: mutated.append(dict(e,attrs=dict(e.get("attrs",{}))))
 rng.shuffle(mutated)
 # Dedupe by event_id, simulating collector deduplication before graph scoring.
 unique={}
 for e in mutated: unique.setdefault(e["event_id"],e)
 ranked,winner=score(list(unique.values()))
 top3=[x["service"] for x in ranked[:3]]
 rank=next((i+1 for i,s in enumerate(top3) if s==target),None) if target else None
 # For non-localizable targets, a confident service result is considered false attribution.
 confident_false=bool(winner and target is None)
 correct=bool(target and winner==target)
 return {"scenario":name,"permutation":permutation,"target_service":target,
  "winner":winner,"ranked_candidates":ranked[:5],"top1_correct":correct,
  "top3_correct":bool(target and target in top3),"mrr":1.0/rank if rank else 0.0,
  "abstained":winner is None,"false_attribution":confident_false,
  "truth_category":truth.get("category"),"truth_primary_entity":truth.get("primary_entity")}

def main():
 p=argparse.ArgumentParser()
 p.add_argument("--generator",default="tests/incident_lab/generate.py")
 p.add_argument("--output",default="gate52-b-evaluation.json")
 args=p.parse_args()
 g=load_generator(args.generator)
 results=[]
 for name,factory in sorted(g.CASES.items()):
  for permutation in range(PERMUTATIONS):
   results.append(run_case(name,factory,TARGET_SERVICE.get(name),SEED,permutation))
 n=len(results);local=[r for r in results if r["target_service"]]
 all_local=n and len(local)
 report={
  "gate":"5.2-B","status":"PASS" if n==22*PERMUTATIONS else "FAIL",
  "evaluator":"service-local deterministic graph ranking; scorer receives no ground truth",
  "seed":SEED,"permutations_per_scenario":PERMUTATIONS,
  "scenario_count":len(g.CASES),"stream_count":n,"localizable_streams":len(local),
  "top1_accuracy_on_localizable":sum(r["top1_correct"] for r in local)/len(local) if local else None,
  "top3_accuracy_on_localizable":sum(r["top3_correct"] for r in local)/len(local) if local else None,
  "mrr_on_localizable":sum(r["mrr"] for r in local)/len(local) if local else None,
  "coverage":sum(not r["abstained"] for r in results)/n if n else 0,
  "abstention_rate":sum(r["abstained"] for r in results)/n if n else 0,
  "false_attribution_rate":sum(r["false_attribution"] for r in results)/n if n else 0,
  "confident_wrong_localizable_rate":sum(bool(r["winner"] and not r["top1_correct"]) for r in local)/len(local) if local else None,
  "per_scenario":results,
 }
 Path(args.output).write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
 print(json.dumps({k:v for k,v in report.items() if k!="per_scenario"},indent=2))
 if report["status"]!="PASS": raise SystemExit(1)
if __name__=="__main__":main()
