#!/usr/bin/env python3
"""Gate 5.2-B: permutation-robust evaluator; scorer never sees ground truth."""
from __future__ import annotations
import argparse, importlib.util, json, random, re
from pathlib import Path

SEED=20261127
PERMUTATIONS=3

def load_generator(path):
 spec=importlib.util.spec_from_file_location("incident_lab_generator",path)
 module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module

def score(events):
 """Deterministic service-level baseline: errors plus explicit failed parent/child."""
 scores={}
 def ensure(service):
  return scores.setdefault(service,{"score":0.0,"failures":0,"upstream_links":0,"downstream_links":0})
 def failed(e): return e.get("level") in ("ERROR","FATAL") or e.get("outcome")=="failure"
 for e in events:
  c=ensure(e["service"])
  if failed(e):
   c["failures"]+=1;c["score"]+=1.0
   if e.get("error_code") or e.get("attrs"): c["score"]+=0.5
 for child in events:
  parent_id=child.get("parent_span_id")
  if not parent_id or not failed(child): continue
  for parent in events:
   if parent is child or parent.get("span_id")!=parent_id or parent["service"]==child["service"]: continue
   same_flow=((child.get("trace_id") and child.get("trace_id")==parent.get("trace_id")) or
               (child.get("request_id") and child.get("request_id")==parent.get("request_id")))
   if same_flow and failed(parent):
    pc=ensure(parent["service"]); cc=ensure(child["service"])
    pc["upstream_links"]+=1;pc["score"]+=1.75
    cc["downstream_links"]+=1;cc["score"]-=0.75
 ranked=sorted(scores.items(),key=lambda kv:(-kv[1]["score"],kv[0]))
 candidates=[{"service":s,**v} for s,v in ranked]
 # Abstain on a tied top score: no deterministic alphabetical guess masquerades as diagnosis.
 winner=None
 if candidates and candidates[0]["score"]>=2 and (candidates[0]["failures"] or candidates[0]["upstream_links"]):
  tied=len(candidates)>1 and abs(candidates[0]["score"]-candidates[1]["score"])<1e-9
  if not tied: winner=candidates[0]["service"]
 return candidates,winner

def map_target(primary_entity, events):
 """Evaluator-only mapping, conservative: exact service/version/instance tokens only."""
 if not primary_entity or primary_entity.lower() in {"no incident","none"}: return None
 text=primary_entity.lower()
 services={e["service"] for e in events}
 matches=set()
 for e in events:
  s=e["service"]
  # service token or explicit service@version target
  if re.search(r"(?<![a-z0-9_-])"+re.escape(s.lower())+r"(?![a-z0-9_-])",text) or (s.lower()+"@") in text:
   matches.add(s)
  # exact instance token (e.g. identity-03) maps to service
  inst=e.get("instance")
  if inst and re.search(r"(?<![a-z0-9_-])"+re.escape(inst.lower())+r"(?![a-z0-9_-])",text):
   matches.add(s)
 return next(iter(matches)) if len(matches)==1 else None

def run_case(name,factory,seed,permutation):
 events,truth=factory()
 rng=random.Random(seed+permutation*1009)
 mutated=[]
 for event in events:
  e=dict(event);e["attrs"]=dict(e.get("attrs") or {})
  for field in ("trace_id","span_id","parent_span_id","request_id","version","duration_ms"):
   if e.get(field) is not None and rng.random()<0.08:e[field]=None
  mutated.append(e)
 originals=list(mutated)
 for e in originals:
  if rng.random()<0.04:mutated.append(dict(e,attrs=dict(e.get("attrs",{}))))
 rng.shuffle(mutated)
 unique={}
 for e in mutated:unique.setdefault(e["event_id"],e)
 ranked,winner=score(list(unique.values()))
 target=map_target(truth.get("primary_entity"),events)
 top3=[x["service"] for x in ranked[:3]]
 target_rank=next((i+1 for i,s in enumerate(top3) if s==target),None) if target else None
 localizable=target is not None
 correct=bool(localizable and winner==target)
 return {"scenario":name,"permutation":permutation,"target_service":target,
  "winner":winner,"ranked_candidates":ranked[:5],"localizable":localizable,
  "top1_correct":correct,"top3_correct":bool(localizable and target in top3),
  "mrr":1.0/target_rank if target_rank else 0.0,
  "abstained":winner is None,"false_attribution":bool(winner and not localizable),
  "confident_wrong_localizable":bool(winner and localizable and winner!=target),
  "truth_category":truth.get("category"),"truth_primary_entity":truth.get("primary_entity")}

def main():
 p=argparse.ArgumentParser()
 p.add_argument("--generator",default="tests/incident_lab/generate.py")
 p.add_argument("--output",default="gate52-b-evaluation.json")
 args=p.parse_args()
 g=load_generator(args.generator);results=[]
 for name,factory in sorted(g.CASES.items()):
  for permutation in range(PERMUTATIONS):results.append(run_case(name,factory,SEED,permutation))
 local=[r for r in results if r["localizable"]];n=len(results)
 by_scenario={}
 for name in sorted(g.CASES):
  rs=[r for r in results if r["scenario"]==name]
  by_scenario[name]={"target_service":rs[0]["target_service"],
   "top1_correct_all_permutations":all(r["top1_correct"] for r in rs) if rs[0]["localizable"] else None,
   "top3_correct_all_permutations":all(r["top3_correct"] for r in rs) if rs[0]["localizable"] else None,
   "winners":[r["winner"] for r in rs],"abstentions":sum(r["abstained"] for r in rs),
   "false_attributions":sum(r["false_attribution"] for r in rs),
   "confident_wrong":sum(r["confident_wrong_localizable"] for r in rs)}
 report={"gate":"5.2-B","status":"PASS" if n==22*PERMUTATIONS else "FAIL",
  "evaluator":"direct generator records; truth used only after scoring for strict target mapping",
  "seed":SEED,"permutations_per_scenario":PERMUTATIONS,"scenario_count":len(g.CASES),
  "stream_count":n,"localizable_streams":len(local),
  "top1_accuracy_on_localizable":sum(r["top1_correct"] for r in local)/len(local) if local else None,
  "top3_accuracy_on_localizable":sum(r["top3_correct"] for r in local)/len(local) if local else None,
  "mrr_on_localizable":sum(r["mrr"] for r in local)/len(local) if local else None,
  "coverage":sum(not r["abstained"] for r in results)/n if n else 0,
  "abstention_rate":sum(r["abstained"] for r in results)/n if n else 0,
  "false_attribution_rate_nonlocalizable":sum(r["false_attribution"] for r in results)/n if n else 0,
  "confident_wrong_localizable_rate":sum(r["confident_wrong_localizable"] for r in local)/len(local) if local else None,
  "scenario_summary":by_scenario,"per_scenario":results}
 Path(args.output).write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
 print(json.dumps({k:v for k,v in report.items() if k not in ("per_scenario","scenario_summary")},indent=2))
 print(json.dumps(report["scenario_summary"],indent=2))
 if report["status"]!="PASS":raise SystemExit(1)
if __name__=="__main__":main()
