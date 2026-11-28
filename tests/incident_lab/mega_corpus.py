#!/usr/bin/env python3
"""Seeded synthetic ObservaLog Lite corpus generator. Ingest raw/ only; ground-truth is evaluator-only."""
import argparse,json,random
from pathlib import Path
from datetime import datetime,timedelta,timezone
CASES=["healthy_baseline","auth_failure","authorization_denial","dependency_timeout","retry_amplification","rate_limit_429","replica_config_drift","release_regression","db_lock_contention","duplicate_delivery","business_invariant_violation","independent_incidents","missing_trace","clock_skew","queue_backlog","resource_pressure","tenant_config_drift","shared_dns_failure","schema_drift","poison_record","mixed_format","overlapping_incidents","trace_id_reuse","large_fanout","deadline_timeout","slow_success","partial_fields","contradictory_aliases","tenant_isolation","circuit_breaker","recovery"]
SERVICES=["gateway","identity","orders","payments","inventory","billing","shipping","notifications","search","catalog","profile","analytics"]
FORMATS=["java_json","node_pino","otel_nested","python_text","spring_text","nginx_json","postgres_text","key_value"]
BASE=datetime(2026,11,25,12,0,tzinfo=timezone.utc)
def opaque_id(r, prefix):
 return f"{prefix}-{r.getrandbits(64):016x}"

def event(r,i,c,g,ts,trace_reuse_pool):
 s=r.choice(SERVICES); instance=f"{s}-{r.randint(1,4):02d}"; version=r.choice(["1.8.2","1.8.3","1.9.0"])
 trace=opaque_id(r,"trc"); req=opaque_id(r,"req"); entity=opaque_id(r,"biz"); level="INFO"; outcome="success"; code=None; dur=r.randint(5,200); msg="Operation completed"; attrs={}; tenant=f"tenant-{r.randint(1,12)}"
 if c in ("auth_failure","authorization_denial"): level,outcome,code="ERROR","failure","JWT_INVALID" if c=="auth_failure" else "ACCESS_DENIED"; msg="Authentication or authorization rejected"
 elif c in ("dependency_timeout","retry_amplification","rate_limit_429"): level,outcome,code="ERROR","failure","HTTP_429" if c=="rate_limit_429" else "UPSTREAM_TIMEOUT"; attrs.update(provider="external-api",retry_count=r.randint(0,7),http_status=429 if c=="rate_limit_429" else 504)
 elif c=="replica_config_drift": attrs["config_fingerprint"]="drifted" if r.random()<.25 else "baseline"
 elif c=="release_regression": version="2.2.0" if r.random()<.5 else "2.1.4"; level="ERROR" if version=="2.2.0" and r.random()<.7 else "INFO"; outcome="failure" if level=="ERROR" else "success"; code="REGRESSION" if level=="ERROR" else None
 elif c=="db_lock_contention": level="WARN"; dur=r.randint(800,10000); attrs.update(lock_wait_ms=dur,pool_active=40,pool_max=40)
 elif c=="duplicate_delivery": attrs.update(message_id=f"msg-{g}-{i%17}",delivery_attempt=r.randint(1,5),idempotency_key=f"idem-{i%17}")
 elif c=="business_invariant_violation": level,outcome,code="ERROR","failure","STATE_MISMATCH"; attrs.update(order_state="paid",settlement_state="failed")
 elif c=="independent_incidents": level,outcome,code="ERROR","failure",r.choice(["DNS_FAILURE","DB_TIMEOUT","AUTH_REJECT"]); trace=f"independent-{i}"
 elif c=="missing_trace": trace=None; attrs["business_key"]=entity
 elif c=="clock_skew": attrs.update(parent_span_id=opaque_id(r,"span"),clock_offset_ms=r.choice([-120000,-30000,30000,120000]))
 elif c=="queue_backlog": level="WARN"; attrs.update(queue_depth=r.randint(200,20000),consumer_lag_ms=r.randint(1000,120000))
 elif c=="resource_pressure": level="WARN"; attrs.update(cpu_pct=r.randint(75,100),heap_pct=r.randint(80,99),gc_pause_ms=r.randint(50,5000))
 elif c=="tenant_config_drift": tenant="affected" if r.random()<.2 else "control"; attrs["feature_flag"]="new-flow" if tenant=="affected" else "stable-flow"
 elif c=="shared_dns_failure": level,outcome,code="ERROR","failure","DNS_LOOKUP_FAILED"; attrs["dependency"]="shared-dns"
 elif c=="schema_drift": level,outcome,code="ERROR","failure","SCHEMA_CONVERSION"; attrs.update(expected_type="integer",actual_type="string")
 elif c=="poison_record": level,outcome,code=("ERROR","failure","MALFORMED_PAYLOAD") if i%9==0 else ("INFO","success",None); msg="Malformed record" if level=="ERROR" else "Adjacent valid record"
 elif c=="mixed_format": msg=r.choice(["request completed","WARN upstream slow","cache hit","ERROR invalid response","status=200 duration=42ms"])
 elif c=="overlapping_incidents": level,outcome="ERROR","failure"; code="AUTH_FAILURE" if i%2==0 else "DB_TIMEOUT"; trace=f"overlap-{g}-{i%2}"
 elif c=="trace_id_reuse": trace=r.choice(trace_reuse_pool); attrs["business_key"]=entity
 elif c=="large_fanout": s=r.choice(SERVICES); attrs.update(fanout_slot=r.randrange(12),parent_span_id=opaque_id(r,"span"))
 elif c=="deadline_timeout": level,outcome,code="ERROR","failure","DEADLINE_EXCEEDED"; dur=r.randint(2000,20000)
 elif c=="slow_success": level="WARN"; dur=r.randint(1200,12000)
 elif c=="partial_fields": trace=None if r.random()<.7 else trace; req=None if r.random()<.4 else req; instance=None if r.random()<.2 else instance
 elif c=="contradictory_aliases": attrs.update(severity="ERROR",level_alias="INFO",status_code=500,outcome_alias="success")
 elif c=="tenant_isolation": tenant=f"tenant-{r.randint(1,40)}"; attrs["shared_trace_probe"]=opaque_id(r,"probe")
 elif c=="circuit_breaker": msg=r.choice(["breaker OPEN","breaker HALF_OPEN","breaker CLOSED"]); level="WARN"
 elif c=="recovery": msg="Dependency recovered; readiness check passed"
 if r.random()<.03: attrs["noise_marker"]="benign"
 if r.random()<.02: attrs["unknown_field"]="unanticipated"
 return {"event_id":opaque_id(r,"evt"),"timestamp":ts.isoformat(timespec="milliseconds").replace("+00:00","Z"),"service":s,"instance":instance,"version":version,"region":"ap-south-1","tenant":tenant,"format":r.choice(FORMATS),"level":level,"message":msg,"trace_id":trace,"request_id":req,"entity_id":entity,"outcome":outcome,"duration_ms":dur,"error_code":code,"attributes":attrs}
def main():
 p=argparse.ArgumentParser(); p.add_argument("--count",type=int,default=25000); p.add_argument("--seed",type=int,default=20261126); p.add_argument("--output",default="/tmp/observalog-mega-corpus"); p.add_argument("--duplicate-rate",type=float,default=.04); p.add_argument("--list",action="store_true"); a=p.parse_args()
 if a.count<1 or not 0<=a.duplicate_rate<=1: p.error("count must be positive and duplicate-rate 0..1")
 if a.list: print("\n".join(f"{n}. {c}" for n,c in enumerate(CASES,1))); return
 r=random.Random(a.seed); root=Path(a.output); raw=root/"raw"; gt=root/"ground-truth"; raw.mkdir(parents=True,exist_ok=True); gt.mkdir(parents=True,exist_ok=True); rows=[]; labels=[]
 trace_reuse_pool=[opaque_id(r,"trc") for _ in range(5)]
 case_order=[]
 for group in range((a.count + len(CASES) - 1)//len(CASES)):
  group_cases=list(CASES); r.shuffle(group_cases); case_order.extend(group_cases)
 for i in range(a.count):
  c=case_order[i]; g=i//len(CASES); ts=BASE+timedelta(milliseconds=i*17+r.randint(-500,500)); e=event(r,i,c,g,ts,trace_reuse_pool); rows.append(e); labels.append({"event_id":e["event_id"],"scenario":c,"group":g,"expected_class":c})
 n=int(a.count*a.duplicate_rate)
 for e in r.sample(rows,n): rows.append(dict(e))
 r.shuffle(rows)
 with (raw/"corpus.jsonl").open("w",encoding="utf-8") as f:
  for e in rows: f.write(json.dumps(e,separators=(",",":"))+"\n")
 with (gt/"labels.json").open("w",encoding="utf-8") as f: json.dump({"seed":a.seed,"base_count":a.count,"duplicates":n,"functional_classes":CASES,"labels":labels},f,indent=2)
 (root/"README.txt").write_text(f"Seed: {a.seed}\nBase: {a.count}\nDuplicates: {n}\nTotal: {len(rows)}\nClasses: {len(CASES)}\nInput raw/corpus.jsonl only; ground-truth/ is evaluator-only. NEVER ingest truth.\n")
 print(f"Generated {len(rows):,} records across {len(CASES)} functional classes -> {root}")
if __name__=="__main__": main()
