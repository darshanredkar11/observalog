#!/usr/bin/env python3
"""Deterministic adversarial payload corpus for ObservaLog Lite.

Creates payloads with sparse, malformed, contradictory, weirdly typed, noisy,
natural-language, Unicode and extreme values. Never embeds evaluator labels.
"""
from __future__ import annotations
import argparse, json, random, string
from pathlib import Path

BASE_TS = "2026-11-25T12:00:00Z"

def build(seed: int = 20261125, repetitions: int = 10) -> list[str]:
    rng = random.Random(seed)
    cases: list[object] = [
        None, True, False, 0, 1, -1, 3.14159, "", " ", "hello",
        "ERROR payment failed; request timeout", "WARN retrying upstream",
        "The request seems to have succeeded but the order is still pending.",
        "Customer says: 'everything is broken'!!! 😵‍💫🔥",
        "שלום log message; 日本語; हिन्दी; मराठी; العربية",
        "line one\nline two\nline three", "\u0000binary-ish\u0001control",
        {}, [], {"message": ""}, {"msg": None}, {"body": 123},
        {"message": ["not", "a", "string"]}, {"level": {}},
        {"level": []}, {"severity_number": "HIGH"}, {"severity_number": 999},
        {"level": "TRACE"}, {"level": "FATAL"}, {"level": "verbose"},
        {"time": "yesterday"}, {"timestamp": "2026-99-99T88:77:66Z"},
        {"timestamp": 0}, {"timestamp": -999999999999999999},
        {"timestamp": "1970-01-01T00:00:00Z"}, {"timestamp": "2099-12-31T23:59:59Z"},
        {"duration_ms": -1}, {"duration_ms": "forever"},
        {"duration": "NaN"}, {"latency_ms": 1e300}, {"latency": 0},
        {"trace_id": ""}, {"trace_id": 123}, {"traceId": "A"},
        {"span_id": []}, {"parent_span_id": {"id": "x"}},
        {"request_id": False}, {"request.id": "req.dotted"},
        {"order_id": {"unexpected": "object"}}, {"tenant": ["tenant-a", "tenant-b"]},
        {"service": ""}, {"service": 7}, {"service": {"name": "nested"}},
        {"service": "alpha", "serviceName": "beta"},
        {"level": "ERROR", "severity": "INFO"},
        {"outcome": "success", "result": "failed"},
        {"status": 200, "outcome": "failed", "message": "contradictory outcome"},
        {"error": "not an object"}, {"error": {"message": "", "code": 42}},
        {"exception": {"type": "NullPointerException", "stacktrace": "x" * 10000},
         "message": "stacktrace heavy"},
        {"message": "unknown fields should not break us", "unmapped": {"deep": {"array": [1,2,3]}}},
        {"resource": {"service": {"name": "nested-service"}, "attributes": {"host.name": "node-1"}}},
        {"attributes": {"event.id": "e-dotted", "request.id": "r-dotted", "order.id": "o-dotted"}},
        {"message": "short", "extra": "x" * 250000},
        {"message": "wide object", **{f"field_{i}": i for i in range(300)}},
        {"message": "deep nested", "a": {"b": {"c": {"d": {"e": "f"}}}}},
        {"message": "array payload", "items": [None, True, 0, "", {"x": "y"}]},
        {"message": "large number", "numeric": 10**100},
        {"message": "negative zero", "numeric": -0.0},
        {"message": "emoji and accents", "value": "🚀🌍 café naïve"},
        {"message": "pathological whitespace", "value": " \t\n  "},
    ]
    payloads=[]
    for rep in range(repetitions):
        for index, case in enumerate(cases):
            if isinstance(case, dict):
                event=dict(case)
                event.setdefault("event_id", f"adv-{rep:02d}-{index:04d}")
                event.setdefault("source_name", "adversarial-corpus")
                event["test_rep"]=rep
                event["test_case"]=index
            else:
                event={
                    "event_id": f"adv-{rep:02d}-{index:04d}",
                    "source_name": "adversarial-corpus",
                    "message": str(case),
                    "test_payload": case,
                    "test_rep": rep,
                    "test_case": index,
                }
            payloads.append(json.dumps(event, ensure_ascii=False, separators=(",", ":")))
    # Intentionally malformed JSON and unstructured prose: these remain raw text
    # records rather than being wrapped into a valid JSON envelope.
    malformed = [
        "{", "{not-json}", "{\"message\":\"unterminated}", "{\"timestamp\":NaN}",
        "[1,2,3", "null trailing", "undefined", "Infinity", "-Infinity",
        "2026-11-25 ERROR [payments] timeout trace_id=trace-x request_id=req-y",
        "a=b c=d ERROR message with key=value inside prose",
    ]
    payloads.extend(malformed * repetitions)
    rng.shuffle(payloads)
    return payloads

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=20261125)
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--output", type=Path, default=Path("/tmp/observalog-adversarial.jsonl"))
    args=parser.parse_args()
    payloads=build(args.seed,args.repetitions)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text("\n".join(payloads)+"\n",encoding="utf-8")
    print(f"generated {len(payloads)} adversarial records at {args.output}")
if __name__=="__main__":
    main()
