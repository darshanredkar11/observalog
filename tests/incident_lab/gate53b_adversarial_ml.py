#!/usr/bin/env python3
"""Gate 5.3-B: RF adversarial validation, feature ablation, prevalence stress, and Rust graph-ranker input impact."""
import argparse,json,math,random,subprocess
from pathlib import Path
from sklearn.feature_extraction import DictVectorizer
from sklearn.ensemble import RandomForestClassifier
from sklearn.metrics import accuracy_score,precision_score,recall_score,f1_score
SEEDS=[20261128,20261129,20261130]
ABLATIONS=["full","no_message","no_error_code","no_service","no_message_error_code_service"]
def features(e,ab):
 a=e.get("attributes") or {};msg=str(e.get("message") or "").lower()
 r={"level":str(e.get("level") or "").upper(),"outcome":str(e.get("outcome") or "").lower(),"format":e.get("format") or ""}
 if "service" not in ab:r["service"]=e.get("service") or ""
 if "error_code" not in ab:r["error_code"]=str(e.get("error_code") or "").lower()
 if "message" not in ab:
  r["message_tokens"]=" ".join(sorted(set(msg.split())))
  for t in ("timeout","retry","failed","failure","recovered","breaker","deadlock","invalid","denied","rejected","malformed","slow","completed","success","open","half_open","closed","mismatch","error","returned","received"):r["msg_"+t]=int(t in msg)
 for k in ("duration_ms","http_status","retry_count","queue_depth","consumer_lag_ms","cpu_pct","heap_pct","gc_pause_ms","lock_wait_ms","pool_active","pool_max","fanout_slot","clock_offset_ms","delivery_attempt"):
  v=e.get(k,a.get(k))
  if isinstance(v,(int,float)) and math.isfinite(v):r["num_"+k]=float(v)
 for k in ("provider","dependency","config_fingerprint","feature_flag","order_state","settlement_state","expected_type","actual_type","topic"):
  v=a.get(k)
  if v is not None:r["attr_"+k]=str(v)
 return r
def met(y,p,t,hn):
 g=[int(x>=t) for x in p];fp=sum(1 for a,b in zip(y,g) if not a and b)
 return {"threshold":float(t),"accuracy":accuracy_score(y,g),"precision":precision_score(y,g,zero_division=0),"recall":recall_score(y,g,zero_division=0),"f1":f1_score(y,g,zero_division=0),"false_positive_count":fp,"false_alerts_per_1000_healthy":fp/hn*1000 if hn else None,"healthy_count":hn}
def choose(y,p):
 ts=[0,.05,.1,.2,.3,.4,.5,.6,.7,.8,.9,.95,.99,1]+sorted(set(map(float,p)))
 return max(ts,key=lambda t:(f1_score(y,[int(x>=t) for x in p],zero_division=0),t))
def split(labels,seed):
 f=list(sorted(set(c for c in labels.values() if c!="healthy_baseline")));random.Random(seed).shuffle(f)
 return f[8:],f[:4],f[4:8]
def hsplit(ids,seed):
 ids=list(ids);random.Random(seed).shuffle(ids);n=len(ids);return ids[:int(.7*n)],ids[int(.7*n):int(.85*n)],ids[int(.85*n):]
def rust(binary,events):
 try:
  p=subprocess.run([binary],input=json.dumps(events),text=True,capture_output=True,timeout=20)
  return json.loads(p.stdout) if p.returncode==0 else {"error":p.stderr[-200:]}
 except Exception as e:return {"error":str(e)}
def main():
 p=argparse.ArgumentParser();p.add_argument("--raw",required=True);p.add_argument("--labels",required=True);p.add_argument("--output",required=True);p.add_argument("--rust-binary",default="");a=p.parse_args()
 raw=[json.loads(x) for x in Path(a.raw).read_text().splitlines() if x.strip()];doc=json.loads(Path(a.labels).read_text());labels={x["event_id"]:x["scenario"] for x in doc["labels"]};u={}
 for e in raw:
  if e["event_id"] in u and u[e["event_id"]]!=e:raise ValueError("duplicate payload mismatch")
  u.setdefault(e["event_id"],e)
 if set(u)!=set(labels):raise ValueError("raw/truth IDs mismatch")
 healthy=[eid for eid,c in labels.items() if c=="healthy_baseline"];rows=[];local=[]
 for seed in SEEDS:
  train_s,val_s,test_s=split(labels,seed);ht,hv,he=hsplit(healthy,seed)
  train=[eid for eid,c in labels.items() if c in train_s]+ht;val=[eid for eid,c in labels.items() if c in val_s]+hv
  faults=[eid for eid,c in labels.items() if c in test_s];test=faults+he;ytr=[int(labels[x]!="healthy_baseline") for x in train];yv=[int(labels[x]!="healthy_baseline") for x in val];yt=[int(labels[x]!="healthy_baseline") for x in test];hn=sum(v==0 for v in yt)
  for ab in ABLATIONS:
   vec=DictVectorizer();xt=vec.fit_transform([features(u[x],ab) for x in train]);xv=vec.transform([features(u[x],ab) for x in val]);xs=vec.transform([features(u[x],ab) for x in test])
   m=RandomForestClassifier(n_estimators=180,min_samples_leaf=3,class_weight="balanced",max_features="sqrt",random_state=seed,n_jobs=-1);m.fit(xt,ytr);pv=m.predict_proba(xv)[:,1];ps=m.predict_proba(xs)[:,1];t=choose(yv,pv)
   def stats(y,p,t,healthy_n):
    pred=[int(x>=t) for x in p];fp=sum(1 for a,b in zip(y,pred) if a==0 and b==1)
    return {"threshold":t,"accuracy":accuracy_score(y,pred),"precision":precision_score(y,pred,zero_division=0),"recall":recall_score(y,pred,zero_division=0),"f1":f1_score(y,pred,zero_division=0),"false_positive_count":fp,"false_alerts_per_1000_healthy":fp/healthy_n*1000 if healthy_n else None}
   r={"seed":seed,"ablation":ab,"test_at_0_5":stats(yt,ps,.5,hn),"validation_calibrated_threshold":t,"test_at_validation_threshold":stats(yt,ps,t,hn),"prevalence_stress":{}}
   for prevalence in (.5,.1,.01):
    rng=random.Random(seed+int(prevalence*1000));fi=list(faults);hi=list(he);rng.shuffle(fi);rng.shuffle(hi);nh=min(len(hi),int(len(fi)*(1-prevalence)/prevalence));ids=fi+hi[:nh];yy=[1]*len(fi)+[0]*nh
    probs=[float(m.predict_proba(vec.transform([features(u[x],ab)]))[:,1][0]) for x in ids];pairs=list(zip(yy,probs));rng.shuffle(pairs);sy=[x[0] for x in pairs];sp=[x[1] for x in pairs]
    r["prevalence_stress"][str(prevalence)]={"realized_prevalence":sum(sy)/len(sy) if sy else 0,"at_0_5":stats(sy,sp,.5,nh),"at_validation_threshold":stats(sy,sp,t,nh)}
   rows.append(r)
   if a.rust_binary and ab=="full":
    full=[u[x] for x in test];filtered=[u[x] for x,pv in zip(test,ps) if pv>=t];before=rust(a.rust_binary,full);after=rust(a.rust_binary,filtered)
    local.append({"seed":seed,"calibrated_threshold":t,"full_events":len(full),"ml_filtered_events":len(filtered),"full_abstained":before.get("abstained"),"filtered_abstained":after.get("abstained"),"full_top3":[x.get("service") for x in before.get("candidates",[])[:3]],"filtered_top3":[x.get("service") for x in after.get("candidates",[])[:3]],"full_winner":before.get("likely_initiator"),"filtered_winner":after.get("likely_initiator"),"errors":[x for x in (before,after) if "error" in x]})
 aggregate={}
 for ab in ABLATIONS:
  rr=[x for x in rows if x["ablation"]==ab]
  aggregate[ab]={"mean_f1_0_5":sum(x["test_at_0_5"]["f1"] for x in rr)/len(rr),"mean_recall_0_5":sum(x["test_at_0_5"]["recall"] for x in rr)/len(rr),"mean_false_alerts_1k_0_5":sum(x["test_at_0_5"]["false_alerts_per_1000_healthy"] for x in rr)/len(rr),"mean_f1_calibrated":sum(x["test_at_validation_threshold"]["f1"] for x in rr)/len(rr),"mean_false_alerts_1k_calibrated":sum(x["test_at_validation_threshold"]["false_alerts_per_1000_healthy"] for x in rr)/len(rr)}
 report={"gate":"5.3-B","status":"PASS","seed_splits":SEEDS,"ablation":ABLATIONS,"method":"scenario-family-disjoint train/validation/test splits; F1 threshold selected on validation families only","prevalence_stress_incident_rates":[.5,.1,.01],"aggregate":aggregate,"runs":rows,"rust_graph_ranker_filter_impact":local,"limitations":["Synthetic corpus only; not production validation.","The generator shares latent distributions across families.","Rust ranker stability/abstention is measured, not causal localization accuracy; generated mega-corpus has no trusted root-cause labels or span-edge truth."]}
 Path(a.output).write_text(json.dumps(report,indent=2)+"\n");print(json.dumps(report,indent=2))
if __name__=="__main__":main()
