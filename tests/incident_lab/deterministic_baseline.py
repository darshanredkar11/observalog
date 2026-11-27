#!/usr/bin/env python3
"""Leakage-safe deterministic baseline evaluator for ObservaLog incident corpus.

Reads raw JSONL and evaluator-only labels separately. The 'scenario' field in raw
records is stripped before feature extraction. Ground truth is joined only after
scoring, by event_id. This is a first-pass diagnostic, not an incident-quality claim.
"""
import argparse, json, math
from collections import Counter
from pathlib import Path

def detect(e):
    """Small deterministic signal set. Scores/labels are intentionally not ML."""
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
    cpu = attrs.get("cpu_pct")
    heap = attrs.get("heap_pct")
    if isinstance(cpu, (int, float)) and cpu >= 90: signals.append("cpu_pressure")
    if isinstance(heap, (int, float)) and heap >= 90: signals.append("heap_pressure")
    if "breaker open" in msg: signals.append("circuit_open")
    return bool(signals), signals

def safe_raw(e):
    # Explicit allowlist. Scenario labels, noise markers, collector metadata, etc. are excluded.
    allowed = {"event_id","timestamp","service","instance","version","region","tenant",
               "format","level","message","trace_id","request_id","entity_id","outcome",
               "duration_ms","error_code","attributes"}
    return {k:v for k,v in e.items() if k in allowed}

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--raw", required=True, help="raw/corpus.jsonl")
    ap.add_argument("--labels", required=True, help="ground-truth/labels.json (evaluator-only)")
    ap.add_argument("--output", default="baseline-report.json")
    a=ap.parse_args()
    raw_rows=[]
    with open(a.raw, encoding="utf-8") as f:
        for line in f:
            if line.strip():
                raw_rows.append(json.loads(line))
    gt=json.load(open(a.labels, encoding="utf-8"))
    labels={x["event_id"]:x["scenario"] for x in gt["labels"]}
    scored={}
    for original in raw_rows:
        e=safe_raw(original)
        pred, signals=detect(e)
        eid=e.get("event_id")
        scored[eid]=(pred, signals)
    # Ground truth joins happen after raw-only scoring; event IDs may repeat as duplicates.
    classes=sorted(set(labels.values()))
    per=Counter(); tp=Counter(); fp=Counter(); fn=Counter(); tn=Counter()
    duplicates=len(raw_rows)-len({e.get("event_id") for e in raw_rows})
    for eid, truth in labels.items():
        pred, _=scored.get(eid,(False,[]))
        if truth not in {"healthy_baseline","healthy_single","healthy_six_service"}:
            per[truth]+=1
            if pred: tp[truth]+=1
            else: fn[truth]+=1
        else:
            per[truth]+=1
            if pred: fp[truth]+=1
            else: tn[truth]+=1
    incident_total=sum(per[c] for c in per if not c.startswith("healthy"))
    tp_total=sum(tp.values()); fp_total=sum(fp.values()); fn_total=sum(fn.values())
    precision=tp_total/(tp_total+fp_total) if tp_total+fp_total else 0.0
    recall=tp_total/(tp_total+fn_total) if tp_total+fn_total else 0.0
    report={"method":"deterministic-first heuristic baseline (event-level; diagnostic only)",
      "raw_records":len(raw_rows),"unique_event_ids":len(scored),"duplicates_in_raw":duplicates,
      "incident_event_count":incident_total,"tp":tp_total,"fp":fp_total,"fn":fn_total,
      "precision":precision,"recall":recall,"f1":(2*precision*recall/(precision+recall) if precision+recall else 0.0),
      "per_scenario":{c:{"events":per[c],"detected":tp[c],"missed":fn[c],"healthy_false_positives":fp[c],"healthy_true_negatives":tn[c]} for c in classes},
      "signal_counts":dict(Counter(s for _,ss in scored.values() for s in ss)),
      "limitations":["Event-level only; does not measure incident grouping/root-cause ranking/detection delay.",
       "Heuristic thresholds are provisional and deliberately simple.",
       "Raw scenario field is excluded; evaluator labels joined by event_id only after prediction.",
       "Healthy/incident taxonomy must be reviewed before interpreting aggregate precision/recall."]}
    Path(a.output).write_text(json.dumps(report,indent=2),encoding="utf-8")
    print(json.dumps(report,indent=2))
if __name__=="__main__": main()
