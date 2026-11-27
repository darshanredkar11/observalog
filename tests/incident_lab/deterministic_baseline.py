#!/usr/bin/env python3
"""Leakage-safe deterministic baseline evaluator for ObservaLog incident corpus.
Raw rows are scored without labels. Ground truth is joined only after predictions.
Duplicate records are required to be exact repeated events and are collapsed for event-level metrics.
"""
import argparse, json
from collections import Counter
from pathlib import Path

HEALTHY_CLASSES = {"healthy_baseline"}

def detect(e):
    attrs = e.get("attributes") or {}
    level = str(e.get("level", e.get("severity", ""))).upper()
    outcome = str(e.get("outcome", "")).lower()
    code = e.get("error_code")
    msg = str(e.get("message", "")).lower()
    signals = []
    if level in {"ERROR", "FATAL", "CRITICAL"}: signals.append("severity")
    if outcome in {"failure", "failed", "error"}: signals.append("outcome")
    if code: signals.append("error_code")
    status = attrs.get("status_code")
    if isinstance(status, (int, float)) and status >= 500: signals.append("http_5xx")
    dur = e.get("duration_ms")
    if isinstance(dur, (int, float)) and dur >= 1000: signals.append("slow_1s")
    retries = attrs.get("retry_count")
    if isinstance(retries, (int, float)) and retries >= 3: signals.append("retry_burst")
    qdepth = attrs.get("queue_depth")
    if isinstance(qdepth, (int, float)) and qdepth >= 1000: signals.append("queue_backlog")
    cpu, heap = attrs.get("cpu_pct"), attrs.get("heap_pct")
    if isinstance(cpu, (int, float)) and cpu >= 90: signals.append("cpu_pressure")
    if isinstance(heap, (int, float)) and heap >= 90: signals.append("heap_pressure")
    if "breaker open" in msg: signals.append("circuit_open")
    return bool(signals), signals

def safe_raw(e):
    allowed = {"event_id","timestamp","service","instance","version","region","tenant",
               "format","level","message","trace_id","request_id","entity_id","outcome",
               "duration_ms","error_code","attributes"}
    return {k:v for k,v in e.items() if k in allowed}

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--raw", required=True)
    ap.add_argument("--labels", required=True)
    ap.add_argument("--output", default="baseline-report.json")
    a=ap.parse_args()
    raw_rows=[]
    with open(a.raw, encoding="utf-8") as f:
        raw_rows.extend(json.loads(line) for line in f if line.strip())
    gt=json.load(open(a.labels, encoding="utf-8"))
    labels={x["event_id"]:x["scenario"] for x in gt["labels"]}
    scored={}
    raw_by_id={}
    for original in raw_rows:
        e=safe_raw(original)
        eid=e.get("event_id")
        if eid in raw_by_id:
            assert raw_by_id[eid] == e, f"Duplicate event_id {eid} has conflicting raw payload"
        else:
            raw_by_id[eid]=e
            scored[eid]=detect(e)
    assert set(scored)==set(labels), "Raw event IDs and evaluator labels must match exactly"
    classes=sorted(set(labels.values()))
    assert HEALTHY_CLASSES.issubset(set(classes)), "Healthy taxonomy missing expected healthy class"
    unknown_healthy=HEALTHY_CLASSES-set(classes)
    assert not unknown_healthy, f"Healthy classes missing from corpus: {unknown_healthy}"
    incident_classes=set(classes)-HEALTHY_CLASSES
    per=Counter(); tp=Counter(); fp=Counter(); fn=Counter(); tn=Counter()
    for eid, truth in labels.items():
        pred, _=scored[eid]
        per[truth]+=1
        if truth in HEALTHY_CLASSES:
            if pred: fp[truth]+=1
            else: tn[truth]+=1
        else:
            if pred: tp[truth]+=1
            else: fn[truth]+=1
    assert sum(per.values())==len(labels), "Metrics must use unique ground-truth event IDs"
    incident_total=sum(per[c] for c in incident_classes)
    tp_total=sum(tp.values()); fp_total=sum(fp.values()); fn_total=sum(fn.values())
    precision=tp_total/(tp_total+fp_total) if tp_total+fp_total else 0.0
    recall=tp_total/(tp_total+fn_total) if tp_total+fn_total else 0.0
    report={"method":"deterministic-first heuristic baseline (unique-event level; diagnostic only)",
      "raw_records":len(raw_rows),"unique_event_ids":len(scored),"duplicates_in_raw":len(raw_rows)-len(scored),
      "incident_event_count":incident_total,"tp":tp_total,"fp":fp_total,"fn":fn_total,
      "precision":precision,"recall":recall,"f1":(2*precision*recall/(precision+recall) if precision+recall else 0.0),
      "healthy_classes":sorted(HEALTHY_CLASSES),
      "per_scenario":{c:{"events":per[c],"detected":tp[c],"missed":fn[c],"healthy_false_positives":fp[c],"healthy_true_negatives":tn[c]} for c in classes},
      "signal_counts":dict(Counter(s for _,ss in scored.values() for s in ss)),
      "limitations":["Event-level only; does not measure incident grouping/root-cause ranking/detection delay.",
       "Heuristic thresholds are provisional and deliberately simple.",
       "Scenario labels and collector metadata are excluded from raw prediction features.",
       "Synthetic corpus results are not real-world validation."]}
    Path(a.output).write_text(json.dumps(report,indent=2),encoding="utf-8")
    print(json.dumps(report,indent=2))
if __name__=="__main__": main()
