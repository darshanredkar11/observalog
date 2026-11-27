#!/usr/bin/env python3
"""Gate 5.2-B evaluator: replay 22 generated cases × arrival permutations against deterministic graph ranking.
This evaluates the prototype heuristic against generator truth, not end-to-end production ingestion.
"""
import argparse, importlib.util, json, random
from pathlib import Path

def load_generator(path):
    spec=importlib.util.spec_from_file_location("incident_generator",path)
    g=importlib.util.module_from_spec(spec); spec.loader.exec_module(g); return g

def score_case(events):
    # Mirror graph_diagnosis.rs exactly for prototype evaluation.
    by_span={}
    dedup={}
    for e in events:
        dedup[e.get("event_id")] = e
    events=list(dedup.values())
    scores={}
    for e in events:
        s=e.get("service","unknown")
        c=scores.setdefault(s,{"service":s,"score":0.0,"direct_failures":0,"upstream_failure_links":0,
            "downstream_failure_links":0,"healthy_peer_events":0,"reasons":[]})
        failed=e.get("outcome")=="failure" or str(e.get("level","")).upper() in ("ERROR","FATAL"
        ) or bool(e.get("error_code"))
        e["_failed"]=failed
        if e.get("span_id"): by_span.setdefault(e["span_id"],[]).append(e)
        if failed:
            c["direct_failures"]+=1; c["score"]+=1
            c["reasons"].append(f"direct failure event {e.get('event_id')}")
            if e.get("error_code") or e.get("dependency"):
                c["score"]+=.5
        else: c["healthy_peer_events"]+=1
    # Resolve explicit parent-span relations only. Parent-child evidence must be in the event records.
    span_events={span:items for span,items in by_span.items()}
    for child in events:
        parent_id=child.get("parent_span_id")
        if not parent_id or not child["_failed"]: continue
        parents=span_events.get(parent_id,[])
        for parent in parents:
            if parent.get("service")==child.get("service"): continue
            if parent.get("_failed"):
                p=scores[parent["service"]]; p["upstream_failure_links"]+=1; p["score"]+=1.5
                p["reasons"].append("explicit failed parent span")
                c=scores[child["service"]]; c["downstream_failure_links"]+=1; c["score"]-=.75
                c["reasons"].append("failed child has explicit failed parent")
                # Causal direction remains structural; timestamp is only a tie/support check.
                if parent.get("offset_ms",0)<=child.get("offset_ms",0): p["score"]+=.25
    # Same-service/version healthy observations are bounded contrast.
    for c in scores.values():
        versions={e.get("version") for e in events if e["service"]==c["service"] and e.get("version")}
        if versions:
            healthy=sum(1 for e in events if e["service"]==c["service"] and e.get("version") in versions and not e["_failed"])
            c["healthy_peer_events"]=healthy
            penalty=min(healthy*.25,1.0); c["score"]-=penalty
            if penalty: c["reasons"].append(f"{healthy} healthy same-service/version controls")
    ranked=sorted(scores.values(),key=lambda x:(-x["score"],x["service"]))
    winner=ranked[0]["service"] if ranked and ranked[0]["score"]>=2 and (ranked[0]["direct_failures"]>0 or ranked[0]["upstream_failure_links"]>0) else None
    return winner, ranked, events

def truth_match(candidate, truth, events):
    if not candidate: return False
    target=str(truth.get("primary_entity","")).lower().strip()
    if candidate.lower()==target or candidate.lower() in target: return True
    # Ground-truth entity labels sometimes refer to an instance, version, dependency host,
    # tenant/flag, DB resource, or span-relationship concept rather than service name.
    related=[]
    for e in events:
        if candidate!=e.get("service"): continue
        related += [str(e.get("instance") or ""),str(e.get("version") or "")]
        related += [str(v) for v in (e.get("attrs") or {}).values()]
    return any(target and target in x.lower() for x in related if x)

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--generator",default="tests/incident_lab/generate.py")
    p.add_argument("--output",default="gate52b-evaluation.json")
    p.add_argument("--permutations",type=int,default=3)
    p.add_argument("--seed",type=int,default=52026)
    p.add_argument("--duplicate-rate",type=float,default=.10)
    p.add_argument("--drop-optional-rate",type=float,default=.15)
    a=p.parse_args()
    g=load_generator(a.generator)
    rows=[]; top1=top3=correct_nonabstain=false_attribution=abstentions=0; n=0
    for name,factory in sorted(g.CASES.items()):
        base,truth=factory()
        for perm in range(a.permutations):
            rng=random.Random(a.seed+perm*1009)
            records=[dict(e) for e in base]
            for e in records:
                for field in g.OPTIONAL_FIELDS:
                    if e.get(field) is not None and rng.random()<a.drop_optional_rate: e[field]=None
            for e in list(records):
                if rng.random()<a.duplicate_rate: records.append(dict(e))
            rng.shuffle(records) # arrival-order stress every permutation
            winner,ranked,canonical=score_case(records)
            n+=1
            matched=truth_match(winner,truth,canonical)
            if winner is None: abstentions+=1
            else:
                correct_nonabstain+=int(matched)
                false_attribution+=int(not matched)
                top1+=int(matched)
            # Top-3 evaluated only where root service/instance/dependency can be grounded in service candidates.
            matches=[c["service"] for c in ranked[:3] if truth_match(c["service"],truth,canonical)]
            top3+=int(bool(matches))
            rows.append({"scenario":name,"permutation":perm,"primary_entity":truth.get("primary_entity"),
                "winner":winner,"top3":[c["service"] for c in ranked[:3]],"match":matched if winner else None,
                "abstained":winner is None,"ranked":ranked})
    summary={"gate":"5.2-B","evaluator":"prototype heuristic mirrored in Python; synthetic truth matching heuristic",
        "status":"REPORT_ONLY","scenario_count":len(g.CASES),"permutations_per_scenario":a.permutations,
        "total_cases":n,"top1_correct":top1,"top1_accuracy_pct":round(100*top1/n,2) if n else 0,
        "top3_correct":top3,"top3_accuracy_pct":round(100*top3/n,2) if n else 0,
        "false_attributions":false_attribution,"false_attribution_rate_pct":round(100*false_attribution/n,2) if n else 0,
        "abstentions":abstentions,"abstention_rate_pct":round(100*abstentions/n,2) if n else 0,
        "nonabstaining_cases":n-abstentions,"correct_nonabstain":correct_nonabstain,
        "limitations":["heuristic truth mapping for primary_entity labels","evaluates prototype only; no ingestion/runtime integration",
            "scenarios without service-level truth may be inherently non-service-root-cause targets",
            "prototype scoring currently uses simplified canonical event model"],
        "cases":rows}
    Path(a.output).write_text(json.dumps(summary,indent=2),encoding="utf-8")
    print(json.dumps({k:v for k,v in summary.items() if k!="cases"},indent=2))
    print("\nScenario results:")
    for r in rows: print(f'{r["scenario"]} p{r["permutation"]}: winner={r["winner"] or "ABSTAIN"} top3={r["top3"]} match={r["match"]}')
if __name__=="__main__": main()
