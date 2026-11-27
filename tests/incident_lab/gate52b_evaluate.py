#!/usr/bin/env python3
"""Gate 5.2-B report evaluator with explicit service-target and abstention semantics."""
import argparse, importlib.util, json, random, re
from pathlib import Path

def load_generator(path):
 spec=importlib.util.spec_from_file_location("incident_generator",path); g=importlib.util.module_from_spec(spec); spec.loader.exec_module(g); return g

def is_failure(e):
 return e.get("outcome")=="failure" or str(e.get("level","")).upper() in ("ERROR","FATAL") or bool(e.get("error_code"))

def score_case(events):
 # deduplicate repeated event IDs (collector duplicate resilience)
 dedup={e.get("event_id"):e for e in events}; events=list(dedup.values())
 by_span={}
 scores={}
 for e in events:
  s=e.get("service","unknown"); c=scores.setdefault(s,{"service":s,"score":0.0,"direct_failures":0,"upstream_failure_links":0,"downstream_failure_links":0,"healthy_peer_events":0,"reasons":[]})
  e["_failed"]=is_failure(e)
  if e.get("span_id"): by_span.setdefault(e["span_id"],[]).append(e)
  if e["_failed"]:
   c["direct_failures"]+=1;c["score"]+=1;c["reasons"].append("direct failure")
   if e.get("error_code") or e.get("dependency"):c["score"]+=.5;c["reasons"].append("error/dependency detail")
  else:c["healthy_peer_events"]+=1
 for child in events:
  pid=child.get("parent_span_id")
  if not pid or not child["_failed"]:continue
  for parent in by_span.get(pid,[]):
   if parent.get("service")==child.get("service") or not parent["_failed"]:continue
   p=scores[parent["service"]];p["upstream_failure_links"]+=1;p["score"]+=1.5;p["reasons"].append("failed explicit parent span")
   c=scores[child["service"]];c["downstream_failure_links"]+=1;c["score"]-=.75;c["reasons"].append("failed child with explicit failed parent")
   if parent.get("offset_ms",0)<=child.get("offset_ms",0):p["score"]+=.25
 for c in scores.values():
  versions={e.get("version") for e in events if e["service"]==c["service"] and e.get("version")}
  if versions:
   healthy=sum(1 for e in events if e["service"]==c["service"] and e.get("version") in versions and not e["_failed"])
   c["healthy_peer_events"]=healthy;pen=min(healthy*.25,1.0);c["score"]-=pen
   if pen:c["reasons"].append(f"{healthy} healthy same-service/version controls")
 ranked=sorted(scores.values(),key=lambda x:(-x["score"],x["service"]))
 winner=ranked[0]["service"] if ranked and ranked[0]["score"]>=2 and (ranked[0]["direct_failures"]>0 or ranked[0]["upstream_failure_links"]>0) else None
 return winner,ranked,events

def service_targets(truth, events):
 # Only service targets explicitly present as service name/version/instance in truth.
 # Composite labels and external entities are not coerced into a service root.
 if truth.get("category") in ("multiple_independent_events","identity_collision"): return None
 label=str(truth.get("primary_entity","")).strip().lower()
 services={e.get("service") for e in events if e.get("service")}
 matches=set()
 for service in services:
  s=str(service).lower()
  if re.search(r"(?<![a-z0-9_-])"+re.escape(s)+r"(?![a-z0-9_-])",label):
   matches.add(service)
  for e in events:
   if e.get("service")!=service:continue
   for value in (e.get("instance"),e.get("version")):
    if value and str(value).lower()==label:matches.add(service)
 # explicit service@version form
 for service in services:
  if label.startswith(str(service).lower()+"@"):matches.add(service)
 return matches if len(matches)==1 else None

def main():
 p=argparse.ArgumentParser();p.add_argument("--generator",default="tests/incident_lab/generate.py");p.add_argument("--output",default="gate52b-evaluation.json");p.add_argument("--permutations",type=int,default=3);p.add_argument("--seed",type=int,default=52026);p.add_argument("--duplicate-rate",type=float,default=.10);p.add_argument("--drop-optional-rate",type=float,default=.15);a=p.parse_args()
 g=load_generator(a.generator);rows=[];total=top1_correct=top3_correct=abstentions=false_attribution=expected_abstain_cases=expected_abstain_correct=0;service_labeled=service_cases_abstain=service_cases=0
 for name,factory in sorted(g.CASES.items()):
  base,truth=factory()
  for perm in range(a.permutations):
   rng=random.Random(a.seed+perm*1009);ev=[dict(x) for x in base]
   for e in ev:
    for field in g.OPTIONAL_FIELDS:
     if e.get(field) is not None and rng.random()<a.drop_optional_rate:e[field]=None
   for e in list(ev):
    if rng.random()<a.duplicate_rate:ev.append(dict(e))
   rng.shuffle(ev);winner,ranked,canonical=score_case(ev);targets=service_targets(truth,canonical);
   total+=1;abstentions+=winner is None
   expected_abstain=targets is None
   if expected_abstain:expected_abstain_cases+=1;expected_abstain_correct+=int(winner is None)
   if targets is not None:
    service_cases+=1;service_cases_abstain+=int(winner is None)
    top1_correct+=int(winner in targets)
    top3_correct+=int(any(c["service"] in targets for c in ranked[:3]))
    if winner is not None and winner not in targets:false_attribution+=1
   rows.append({"scenario":name,"permutation":perm,"primary_entity":truth.get("primary_entity"),"service_target":sorted(targets) if targets is not None else None,"winner":winner,"top3":[c["service"] for c in ranked[:3]],"top1_correct":bool(winner in targets) if targets is not None else None,"top3_correct":bool(any(c["service"] in targets for c in ranked[:3])) if targets is not None else None,"expected_abstain":expected_abstain,"abstained":winner is None,"ranked":ranked})
 summary={"gate":"5.2-B","status":"REPORT_ONLY","evaluator":"Python mirror of prototype ranking; explicit service-level truth mapping; non-service/composite labels excluded from service accuracy","scenario_count":len(g.CASES),"permutations_per_scenario":a.permutations,"total_cases":total,"service_labeled_cases":service_cases,"top1_correct":top1_correct,"top1_accuracy_pct":round(100*top1_correct/service_cases,2) if service_cases else None,"top3_correct":top3_correct,"top3_accuracy_pct":round(100*top3_correct/service_cases,2) if service_cases else None,"false_attributions":false_attribution,"false_attribution_rate_pct":round(100*false_attribution/service_cases,2) if service_cases else None,"abstentions":abstentions,"abstention_rate_pct":round(100*abstentions/total,2) if total else 0,"expected_abstain_cases":expected_abstain_cases,"correct_expected_abstentions":expected_abstain_correct,"expected_abstention_accuracy_pct":round(100*expected_abstain_correct/expected_abstain_cases,2) if expected_abstain_cases else None,"service_cases_abstained":service_cases_abstain,"limitations":["prototype-only evaluation; not runtime ingestion","truth primary_entity sometimes names an external dependency, business object, host, or composite incident; these are not treated as service targets","Python mirror must remain aligned with Rust implementation","score is not probability/calibrated confidence"],"cases":rows}
 Path(a.output).write_text(json.dumps(summary,indent=2),encoding="utf-8");print(json.dumps({k:v for k,v in summary.items() if k!="cases"},indent=2))
 print("\nPer-scenario:")
 for r in rows:print(f'{r["scenario"]} p{r["permutation"]}: winner={r["winner"] or "ABSTAIN"} target={r["service_target"]} top3={r["top3"]} top3_correct={r["top3_correct"]}')
if __name__=="__main__":main()
