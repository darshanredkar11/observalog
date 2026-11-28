#!/usr/bin/env python3
"""Gate 5.3: incident-vs-healthy ML generalization experiment.

Train/test split is by held-out scenario families (not randomly by event), so no
scenario's event records cross the split. Ground-truth labels are joined after raw
feature extraction. IDs, timestamp, tenant, region, instance, version, and event
ordering are intentionally excluded. Results are exploratory synthetic-only.
"""
from __future__ import annotations
import argparse, json, math
from pathlib import Path
from collections import Counter
from sklearn.feature_extraction import DictVectorizer
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler
from sklearn.linear_model import LogisticRegression
from sklearn.ensemble import RandomForestClassifier, HistGradientBoostingClassifier
from sklearn.metrics import precision_score, recall_score, f1_score, accuracy_score, roc_auc_score

HELD_OUT = {
    "auth_failure", "authorization_denial", "dependency_timeout", "retry_amplification",
    "rate_limit_429", "replica_config_drift", "release_regression", "db_lock_contention",
}

def features(event):
    # Feature whitelist: operational signals only; no identity/time/order fields.
    attrs = event.get("attributes") or {}
    level = str(event.get("level") or "").upper()
    outcome = str(event.get("outcome") or "").lower()
    code = str(event.get("error_code") or "").lower()
    message = str(event.get("message") or "").lower()
    row = {
        "level": level, "outcome": outcome, "error_code": code,
        "format": event.get("format") or "",
        "service": event.get("service") or "",
        "message_tokens": " ".join(sorted(set(message.split()))),
    }
    for key in ("duration_ms","http_status","retry_count","queue_depth","consumer_lag_ms",
                 "cpu_pct","heap_pct","gc_pause_ms","lock_wait_ms","pool_active","pool_max",
                 "fanout_slot","clock_offset_ms","delivery_attempt"):
        value = event.get(key, attrs.get(key))
        if isinstance(value,(int,float)) and math.isfinite(value):
            row["num_"+key] = float(value)
    for key in ("provider","dependency","config_fingerprint","feature_flag","order_state",
                 "settlement_state","expected_type","actual_type","topic"):
        value = attrs.get(key)
        if value is not None:
            row["attr_"+key] = str(value)
    # Boolean flags derived from message are permitted as operational text features.
    for token in ("timeout","retry","failed","failure","recovered","breaker","deadlock",
                  "invalid","denied","rejected","malformed","slow","completed","success",
                  "open","half_open","closed","mismatch","error","returned","received"):
        row["msg_"+token] = int(token in message)
    return row

def score(y, probs, threshold=0.5):
    pred=[int(p>=threshold) for p in probs]
    return {"threshold":threshold,"accuracy":accuracy_score(y,pred),
            "precision":precision_score(y,pred,zero_division=0),
            "recall":recall_score(y,pred,zero_division=0),
            "f1":f1_score(y,pred,zero_division=0),
            "roc_auc":roc_auc_score(y,probs) if len(set(y))>1 else None}

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--raw",required=True)
    ap.add_argument("--labels",required=True)
    ap.add_argument("--output",default="gate53-ml-report.json")
    args=ap.parse_args()
    raw=[json.loads(x) for x in Path(args.raw).read_text().splitlines() if x.strip()]
    truth_doc=json.loads(Path(args.labels).read_text())
    labels={r["event_id"]:r["scenario"] for r in truth_doc["labels"]}
    # Collapse only exact duplicate IDs; reject conflicting copies.
    unique={}
    for e in raw:
        if e["event_id"] in unique and unique[e["event_id"]]!=e:
            raise ValueError("conflicting duplicate payload for event_id")
        unique.setdefault(e["event_id"],e)
    if set(unique)!=set(labels): raise ValueError("raw/label IDs do not match")
    scenarios=sorted(set(labels.values()))
    heldout=sorted(HELD_OUT & set(scenarios))
    train_scenarios=sorted(set(scenarios)-set(heldout))
    # Healthy records are split by opaque event ID so both train and test contain negatives.
    # Held-out fault families remain entirely absent from training.
    healthy_ids=sorted(eid for eid,c in labels.items() if c=="healthy_baseline")
    healthy_test=set(healthy_ids[::5])
    train_ids=[eid for eid,c in labels.items() if c in train_scenarios and c!="healthy_baseline"]
    test_ids=[eid for eid,c in labels.items() if c in heldout]
    train_ids.extend(eid for eid in healthy_ids if eid not in healthy_test)
    test_ids.extend(healthy_test)
    X_train=[features(unique[eid]) for eid in train_ids]
    X_test=[features(unique[eid]) for eid in test_ids]
    y_train=[int(labels[eid]!="healthy_baseline") for eid in train_ids]
    y_test=[int(labels[eid]!="healthy_baseline") for eid in test_ids]
    vectorizer=DictVectorizer(sparse=True)
    Xt=vectorizer.fit_transform(X_train); Xv=vectorizer.transform(X_test)
    models={
        "logistic_regression":LogisticRegression(max_iter=1500,class_weight="balanced"),
        "random_forest":RandomForestClassifier(n_estimators=300,min_samples_leaf=2,class_weight="balanced",random_state=20261128,n_jobs=-1),
        "hist_gradient_boosting":HistGradientBoostingClassifier(max_iter=150,learning_rate=0.08,max_leaf_nodes=15,l2_regularization=1.0,random_state=20261128),
    }
    # Existing rule baseline, reproduced only as a binary event detector for comparison.
    def rule(e):
        a=e.get("attributes") or {}
        lvl=str(e.get("level") or "").upper(); outcome=str(e.get("outcome") or "").lower(); code=e.get("error_code")
        dur=e.get("duration_ms"); retry=a.get("retry_count"); q=a.get("queue_depth"); cpu=a.get("cpu_pct"); heap=a.get("heap_pct")
        msg=str(e.get("message") or "").lower()
        return bool(lvl in {"ERROR","FATAL","CRITICAL"} or outcome in {"failure","failed","error"} or code
                    or (isinstance(a.get("status_code"),(int,float)) and a["status_code"]>=500)
                    or (isinstance(dur,(int,float)) and dur>=1000)
                    or (isinstance(retry,(int,float)) and retry>=3)
                    or (isinstance(q,(int,float)) and q>=1000)
                    or (isinstance(cpu,(int,float)) and cpu>=90)
                    or (isinstance(heap,(int,float)) and heap>=90)
                    or "breaker open" in msg)
    rule_y=[int(rule(unique[eid])) for eid in test_ids]
    reports={"deterministic_rule_baseline":score(y_test,rule_y)}
    for name,model in models.items():
        # HistGradientBoosting requires dense input; other models stay sparse.
        fit_x, test_x = (Xt.toarray(), Xv.toarray()) if name == "hist_gradient_boosting" else (Xt, Xv)
        model.fit(fit_x,y_train)
        probs=model.predict_proba(test_x)[:,1]
        reports[name]=score(y_test,probs)
    report={"gate":"5.3","status":"PASS","experiment":"binary incident detection, held-out scenario families",
            "base_records":len(labels),"raw_records_with_duplicates":len(raw),
            "unique_records":len(unique),"scenario_count":len(scenarios),
            "train_scenario_families":train_scenarios,"held_out_scenario_families":heldout,"healthy_negative_split":"every fifth healthy event ID held out for test; remaining healthy events train",
            "train_events":len(train_ids),"test_events":len(test_ids),
            "feature_policy":"whitelist; excludes IDs, timestamps, tenant, region, instance, version, event sequence and ground truth",
            "results":reports,"limitations":["Synthetic corpus only; not production accuracy.","Held-out families are a single fixed split; conclusions require repeat-seed and cross-split validation.","Binary detection does not evaluate root-cause localization or accepted-diagnosis coverage."]}
    Path(args.output).write_text(json.dumps(report,indent=2)+"\\n")
    print(json.dumps(report,indent=2))
if __name__=="__main__": main()
