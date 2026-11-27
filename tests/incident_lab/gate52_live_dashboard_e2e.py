#!/usr/bin/env python3
"""Gate 5.2 live board integration smoke: authenticated ingestion and dashboard reads."""
import json
import os
import time
import urllib.error
import urllib.request

BASE = os.environ.get("LITE_E2E_BASE", "http://127.0.0.1:4000").rstrip("/")
TOKEN = os.environ.get("LITE_INGEST_TOKEN", "gate52-live-e2e-token-0123456789")
HEADERS = {"x-observalog-token": TOKEN, "content-type": "application/json"}

def request(path, headers=None, data=None):
    req = urllib.request.Request(BASE + path, headers=headers or {}, data=data, method="POST" if data is not None else "GET")
    try:
        with urllib.request.urlopen(req, timeout=10) as res:
            return res.status, json.loads(res.read().decode()) if res.headers.get("content-type","").startswith("application/json") else res.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")

def wait_health():
    for _ in range(60):
        try:
            code, _ = request("/health")
            if code == 200:
                return
        except Exception:
            pass
        time.sleep(2)
    raise AssertionError("Brain health did not become ready within 120s")

def main():
    wait_health()
    code, _ = request("/api/dashboard/summary")
    assert code == 401, f"dashboard endpoint should require auth, got {code}"
    payload=[]
    for i in range(10):
        service=f"gate52-e2e-service-{i:02d}"
        log={
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "service": service,
            "instance": f"{service}-1",
            "version": "e2e",
            "environment": "ci",
            "severity": "INFO" if i != 3 else "ERROR",
            "event_id": f"gate52-e2e-{i}",
            "trace_id": f"gate52-e2e-trace-{i:08d}",
            "span_id": f"gate52-e2e-span-{i:08d}",
            "message": f"Gate 5.2 dashboard smoke event from {service}",
            "outcome": "success" if i != 3 else "failure",
        }
        payload.append({"log":json.dumps(log),"source_path":f"/tmp/gate52/{service}.log","source_offset":i+1,"tag":service})
    code, result=request("/ingest/raw",HEADERS,json.dumps(payload).encode())
    assert code == 202, f"ingest failed: {code} {result}"
    assert result.get("accepted",0) == 10, f"expected 10 accepted records, got {result}"
    last_summary=None
    for _ in range(40):
        code,last_summary=request("/api/dashboard/summary",HEADERS)
        assert code == 200, f"summary endpoint failed: {code} {last_summary}"
        if last_summary.get("service_count",0)>=10 and last_summary.get("event_count",0)>=10:
            break
        time.sleep(1)
    assert last_summary["service_count"]>=10, f"expected 10 canonical services, summary={last_summary}"
    assert last_summary["event_count"]>=10, f"expected >=10 canonical events, summary={last_summary}"
    code,logs=request("/api/logs?limit=100",HEADERS)
    assert code==200 and len(logs.get("events",[]))>=10, f"logs API did not expose events: {code} {logs}"
    code,findings=request("/api/findings",HEADERS)
    assert code==200 and isinstance(findings.get("findings"),list), f"findings API shape invalid: {code} {findings}"
    print(json.dumps({"gate":"5.2 live-dashboard E2E","status":"PASS","accepted":result["accepted"],"services":last_summary["service_count"],"events_last_60m":last_summary["event_count"],"errors_last_60m":last_summary["error_count"],"queue":last_summary["queue"],"log_records_returned":len(logs["events"]),"findings_endpoint":"PASS"},indent=2))

if __name__ == "__main__":
    main()
