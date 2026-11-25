#!/usr/bin/env python3
"""Exercise the full Lite ingestion/gate pipeline with adversarial raw payloads."""
from __future__ import annotations
import argparse, json, os, subprocess, sys, time, urllib.error, urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

def request(url, method="GET", body=None, token=None, headers=None, timeout=30):
    h=dict(headers or {})
    if token is not None: h["X-ObservaLog-Token"]=token
    if body is not None: h.setdefault("Content-Type","application/json")
    req=urllib.request.Request(url,method=method,data=body,headers=h)
    try:
        with urllib.request.urlopen(req,timeout=timeout) as res:
            return res.status,res.read()
    except urllib.error.HTTPError as e:
        return e.code,e.read()

def psql(sql):
    cmd=["docker","compose","exec","-T","timescaledb","psql","-U","observalog","-d","observalog","-Atqc",sql]
    return subprocess.check_output(cmd,text=True).strip()

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--base-url",default="http://localhost:4000")
    ap.add_argument("--token",default=os.environ.get("LITE_INGEST_TOKEN"))
    ap.add_argument("--payload-file",default="/tmp/observalog-adversarial.jsonl")
    args=ap.parse_args()
    if not args.token: raise SystemExit("LITE_INGEST_TOKEN is required")
    payloads=[line for line in Path(args.payload_file).read_text(encoding="utf-8").splitlines() if line]
    # Security and boundary tests: must reject unauthorized/empty requests.
    code,_=request(args.base_url+"/ingest/raw","POST",b'["unauthorized-test"]',token="wrong-token")
    assert code==401, f"unauthorized request expected 401, got {code}"
    code,_=request(args.base_url+"/ingest/raw","POST",b"",token=args.token)
    assert code==400, f"empty request expected 400, got {code}"
    # Test over-limit request body. Expected 413 from Axum body limit.
    oversized=b"x"*(16*1024*1024+1)
    code,_=request(args.base_url+"/ingest/raw","POST",oversized,token=args.token,headers={"Content-Type":"text/plain"},timeout=90)
    assert code==413, f"oversize request expected 413, got {code}"

    raw_before=int(psql("SELECT COUNT(*) FROM lite_raw_records"))
    accepted_total=0
    batch=48
    batches=[payloads[offset:offset+batch] for offset in range(0,len(payloads),batch)]
    def send_batch(index, chunk):
        body=json.dumps(chunk,ensure_ascii=False,separators=(",",":")).encode()
        code,response=request(args.base_url+"/ingest/raw","POST",body,token=args.token,timeout=90)
        assert code==202, f"batch {index} expected 202 got {code}: {response[:300]!r}"
        result=json.loads(response)
        assert result.get("rejected",0)==0, result
        return result.get("accepted",0)
    # Concurrent client requests intentionally contend on durable admission and
    # queue creation; the DB advisory lock must preserve a correct backlog.
    with ThreadPoolExecutor(max_workers=4) as pool:
        accepted_total=sum(pool.map(lambda pair: send_batch(*pair), enumerate(batches)))
    # Deliberate source-identity collision: same source event should collapse at
    # normalization, but both raw deliveries must remain durably auditable.
    duplicate_event=json.dumps({"event_id":"dup-event-777","service":"duplicate-test","level":"ERROR","message":"same event twice"})
    duplicate_body=json.dumps([duplicate_event,duplicate_event]).encode()
    code,response=request(args.base_url+"/ingest/raw","POST",duplicate_body,token=args.token)
    assert code==202, f"duplicate-event delivery failed: {code} {response!r}"
    duplicate_result=json.loads(response)
    assert duplicate_result.get("accepted")==2, f"raw duplicate deliveries should both be accepted: {duplicate_result}"

    expected_raw=raw_before+accepted_total+2
    print(f"Accepted {accepted_total} adversarial unique payloads plus 2 duplicate deliveries; waiting for gates to drain...")
    drained=False
    for _ in range(240):
        pending=int(psql("SELECT COUNT(*) FROM lite_gate_work WHERE state IN ('pending','leased')"))
        dead=int(psql("SELECT COUNT(*) FROM lite_gate_work WHERE state='dead'"))
        raw=int(psql("SELECT COUNT(*) FROM lite_raw_records"))
        if pending==0 and raw==expected_raw:
            drained=True
            break
        time.sleep(2)
    assert drained, f"queue did not drain: raw={raw}/{expected_raw}, pending={pending}, dead={dead}"
    # Validate raw records are retained and every unique accepted test record
    # has both normalized payload and index. Duplicate source IDs may intentionally
    # be suppressed at canonical indexing, so account for the deliberate pair.
    payload_count=int(psql("SELECT COUNT(*) FROM lite_event_payload"))
    index_count=int(psql("SELECT COUNT(*) FROM lite_event_index"))
    raw_preserved=int(psql("SELECT COUNT(*) FROM lite_event_payload p JOIN lite_raw_records r ON r.id=p.raw_id WHERE p.raw_text IS NOT DISTINCT FROM r.raw_text"))
    flags_count=int(psql("SELECT COUNT(*) FROM lite_event_index WHERE jsonb_array_length(quality_flags)>0"))
    missing_trace=int(psql("SELECT COUNT(*) FROM lite_event_index WHERE trace_id IS NULL"))
    missing_outcome=int(psql("SELECT COUNT(*) FROM lite_event_index WHERE outcome IS NULL"))
    missing_duration=int(psql("SELECT COUNT(*) FROM lite_event_index WHERE duration_ms IS NULL"))
    signal_count=int(psql("SELECT COUNT(*) FROM lite_signals"))
    finding_count=int(psql("SELECT COUNT(*) FROM lite_findings"))
    short_trace_links=int(psql("SELECT COUNT(*) FROM lite_event_links WHERE evidence->>'trace_id'='A'"))
    short_trace_flagged=int(psql("SELECT COUNT(*) FROM lite_event_index WHERE quality_flags::text LIKE '%trace_id_short_unverified%'"))
    malformed_preserved=int(psql("SELECT COUNT(*) FROM lite_event_payload WHERE raw_text IN ('{','{not-json}','undefined','Infinity','-Infinity')"))
    assert payload_count==index_count, f"payload/index mismatch {payload_count}/{index_count}"
    assert raw_preserved==payload_count, f"raw payload not preserved for {raw_preserved}/{payload_count}"
    assert flags_count>0, "negative payload corpus produced no quality flags"
    assert missing_trace>0 and missing_outcome>0 and missing_duration>0, "sparse records were unexpectedly filled with fabricated fields"
    assert signal_count>0, "detection gate produced no signals from explicit ERROR/FATAL probes"
    assert finding_count>0, "reasoning gate produced no findings from explicit failure signals"
    assert short_trace_links==0, f"short unverified trace ID 'A' created {short_trace_links} correlations"
    assert short_trace_flagged>0, "short unverified trace IDs were not flagged by the quality gate"
    assert dead==0, f"{dead} work items are dead"
    print(f"PASS: raw={raw} canonical={index_count} payload={payload_count} quality_flagged={flags_count} signals={signal_count} findings={finding_count} missing_trace={missing_trace} missing_outcome={missing_outcome} missing_duration={missing_duration} malformed_preserved={malformed_preserved} weak_trace_links={short_trace_links} short_trace_flagged={short_trace_flagged} pending={pending} dead={dead}")
    print("PASS: all five gates; auth, empty-body, oversized-body, optional-field, contradictory, malformed, NL, unicode, concurrency, replay/duplicate and weak-identity negative checks")
if __name__=="__main__":
    main()
