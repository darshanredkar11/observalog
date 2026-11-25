#!/usr/bin/env python3
"""Generate raw, heterogeneous production-style log streams for the ObservaLog incident lab.

No third-party packages are required. Raw logs and hidden evaluation truth are written
to separate directories. Only raw/*.log should be mounted into Fluent Bit.
"""
from __future__ import annotations

import argparse
import copy
import json
import random
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

BASE = datetime(2026, 11, 25, 12, 0, 0, tzinfo=timezone.utc)
LEVELS = {"DEBUG": 10, "INFO": 20, "WARN": 30, "WARNING": 30, "ERROR": 40, "FATAL": 50}
OPTIONAL_FIELDS = ("trace_id", "span_id", "parent_span_id", "request_id", "version", "duration_ms")
CASES: dict[str, Any] = {}


def event(events: list[dict[str, Any]], service: str, ms: int, message: str,
          *, fmt: str = "java_json", level: str = "INFO", trace: str | None = None,
          span: str | None = None, parent: str | None = None, request: str | None = None,
          entity: str | None = None, instance: str | None = None,
          version: str = "1.4.0", outcome: str | None = None,
          duration: int | None = None, event_name: str | None = None,
          error_code: str | None = None, attrs: dict[str, Any] | None = None,
          tenant: str | None = None, region: str = "ap-south-1",
          event_id: str | None = None) -> None:
    idx = len(events) + 1
    events.append({
        "event_id": event_id or f"evt-{len(events)+1:04d}",
        "timestamp": (BASE + timedelta(milliseconds=ms)).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
        "offset_ms": ms, "service": service, "instance": instance or f"{service}-01",
        "version": version, "region": region, "tenant": tenant, "format": fmt,
        "level": level, "message": message, "trace_id": trace, "span_id": span,
        "parent_span_id": parent, "request_id": request, "entity_id": entity,
        "outcome": outcome, "duration_ms": duration, "event": event_name,
        "error_code": error_code, "attrs": attrs or {},
    })


def truth(category: str, root_cause: str, primary_entity: str,
          *, symptoms: list[str] | None = None, amplifiers: list[str] | None = None,
          expected: list[str] | None = None, must_not: list[str] | None = None,
          confidence: str = "high", notes: str = "") -> dict[str, Any]:
    return {
        "category": category, "primary_cause": root_cause, "primary_entity": primary_entity,
        "symptom_services": symptoms or [], "amplifiers": amplifiers or [],
        "expected_observations": expected or [], "must_not_correlate": must_not or [],
        "expected_confidence": confidence, "notes": notes,
    }


def make_healthy_single() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "edge-gateway", 0, "Request accepted", trace="trc-h001", span="s001", fmt="pino",
          request="req-h001", entity="ord-h001", event_name="http.request.received")
    event(e, "edge-gateway", 25, "Request completed with status 200", trace="trc-h001", span="s001",
          request="req-h001", entity="ord-h001", outcome="success", duration=25, fmt="pino")
    return e, truth("healthy", "no incident", "edge-gateway",
                    expected=["normal request lifecycle", "no failure hypothesis"])


def make_healthy_fanout() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    t = "trc-h002"; r = "req-h002"; o = "ord-h002"
    event(e, "gateway", 0, "Checkout request received", trace=t, span="g001", request=r, entity=o, fmt="java_json")
    event(e, "identity", 12, "Customer identity verified", trace=t, span="a001", parent="g001", request=r, entity=o, fmt="otel")
    event(e, "orders", 35, "Order draft created", trace=t, span="o001", parent="g001", request=r, entity=o, fmt="node_pino")
    event(e, "stock", 58, "Inventory reserved", trace=t, span="i001", parent="o001", request=r, entity=o, outcome="success", duration=18, fmt="python_text")
    event(e, "payments", 90, "Payment authorized", trace=t, span="p001", parent="o001", request=r, entity=o, outcome="success", duration=32, fmt="otel")
    event(e, "orders", 112, "Order confirmed", trace=t, span="o002", parent="o001", request=r, entity=o, outcome="success", duration=14)
    event(e, "notify", 130, "Receipt queued", trace=t, span="n001", parent="o002", request=r, entity=o, fmt="spring_text")
    return e, truth("healthy", "no incident", "checkout-flow",
                    expected=["six-service fanout remains one business flow", "parent-child edges retained"])


def make_auth_cascade() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    t = "trc-f003"; r = "req-f003"; o = "ord-f003"
    event(e, "gateway", 0, "Checkout request received", trace=t, span="g003", request=r, entity=o)
    event(e, "identity", 8, "JWT audience mismatch: expected orders-api", trace=t, span="a003",
          parent="g003", request=r, entity=o, fmt="spring_text", level="ERROR",
          error_code="JWT_AUDIENCE_MISMATCH", outcome="failure",
          attrs={"expected_audience": "orders-api", "actual_audience": "legacy-api", "issuer": "id.example"})
    event(e, "orders", 14, "Request rejected because identity context is unavailable", trace=t,
          span="o003", parent="g003", request=r, entity=o, level="WARN", outcome="failure",
          error_code="AUTH_CONTEXT_MISSING", fmt="python_text")
    event(e, "gateway", 20, "Checkout returned 401", trace=t, span="g003", request=r,
          entity=o, level="WARN", attrs={"http_status": 401})
    return e, truth("configuration_or_authentication", "identity service token audience validation mismatch",
                    "identity", symptoms=["orders", "gateway"],
                    expected=["identity rejection precedes dependent symptoms", "payment is not invoked"],
                    confidence="high")


def make_provider_retry() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    t = "trc-f004"; r = "req-f004"; o = "ord-f004"
    event(e, "orders", 0, "Order submitted", trace=t, span="o004", request=r, entity=o)
    event(e, "stock", 20, "Inventory reservation succeeded", trace=t, span="i004", parent="o004",
          request=r, entity=o, outcome="success", fmt="node_pino")
    for n, ms in enumerate((45, 245, 650, 1350), start=1):
        event(e, "payments", ms, f"Provider attempt {n} returned HTTP 429", trace=t, span=f"p04{n}",
              parent="o004", request=r, entity=o, level="ERROR" if n == 4 else "WARN",
              error_code="PROVIDER_RATE_LIMIT", outcome="failure", fmt="otel",
              attrs={"http_status": 429, "retry_number": n, "retry_delay_ms": max(0, ms-45),
                     "provider": "pay.example"})
    event(e, "payments", 1700, "Connection pool wait reached 1800ms", trace=t, span="pool04",
          parent="o004", request=r, entity=o, level="WARN", fmt="python_text",
          attrs={"pool_wait_ms": 1800, "active_connections": 50, "max_connections": 50})
    event(e, "orders", 1900, "Checkout deadline exceeded", trace=t, span="o004", request=r,
          entity=o, level="ERROR", outcome="failure", error_code="PAYMENT_DEADLINE_EXCEEDED")
    return e, truth("external_dependency", "payment provider rate limiting",
                    "pay.example", symptoms=["orders", "payments"], amplifiers=["aggressive immediate retry policy", "payment connection pool saturation"],
                    expected=["provider 429 responses precede pool exhaustion", "retry amplification distinguished from initiating cause"],
                    confidence="high")


def make_config_drift() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    for n, ms in enumerate((0, 70, 140, 210), start=1):
        inst = f"identity-{n:02d}"
        bad = n == 3
        event(e, "identity", ms, "JWT validation failed" if bad else "JWT validation succeeded",
              trace=f"trc-cfg{n}", span=f"a{n:03d}", request=f"req-cfg{n}", instance=inst,
              version="2.2.1", level="ERROR" if bad else "INFO",
              outcome="failure" if bad else "success", error_code="JWT_ISSUER_MISMATCH" if bad else None,
              fmt="java_json", attrs={"issuer": "https://old-id.example" if bad else "https://id.example",
                                      "config_fingerprint": "cfg-old" if bad else "cfg-current",
                                      "pod": inst})
    return e, truth("configuration", "one identity replica has stale issuer configuration",
                    "identity-03", symptoms=["requests routed to identity-03"],
                    expected=["same version across replicas", "failure follows config fingerprint, not release version"],
                    must_not=["healthy identity replicas", "all identity traffic"])


def make_release_regression() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    for n, (ver, inst, ms, bad) in enumerate([
        ("4.7.2", "orders-old-1", 0, False), ("4.7.2", "orders-old-2", 30, False),
        ("4.8.0", "orders-new-1", 60, True), ("4.8.0", "orders-new-2", 90, True)], start=1):
        event(e, "orders", ms, "Coupon evaluation dereferenced missing rules object" if bad else "Order validation completed",
              trace=f"trc-rel{n}", span=f"o{n:03d}", request=f"req-rel{n}", entity=f"ord-rel{n}",
              instance=inst, version=ver, fmt="node_pino", level="ERROR" if bad else "INFO",
              outcome="failure" if bad else "success", error_code="NULL_RULES_REFERENCE" if bad else None,
              attrs={"coupon_mode": "auto"})
    return e, truth("code_regression", "orders release 4.8.0 introduced a null dereference in coupon evaluation",
                    "orders@4.8.0", symptoms=["orders-new-1", "orders-new-2"],
                    expected=["old version succeeds under comparable requests", "new version fails on same path"],
                    must_not=["orders@4.7.2"])


def make_db_deadlock() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "orders", 0, "Transaction started for order update", trace="trc-db01", span="o-db1",
          request="req-db1", entity="ord-db1", fmt="spring_text")
    event(e, "billing", 4, "Transaction started for invoice update", trace="trc-db02", span="b-db1",
          request="req-db2", entity="inv-db1", fmt="python_text")
    event(e, "postgres", 30, "ERROR: deadlock detected; Process 418 waits for ShareLock on transaction 990",
          fmt="postgres_text", level="ERROR", error_code="40P01",
          attrs={"pid": 418, "application_name": "orders", "wait_event": "transactionid"})
    event(e, "orders", 34, "Transaction rolled back after deadlock", trace="trc-db01", span="o-db1",
          request="req-db1", entity="ord-db1", fmt="java_json", level="ERROR", outcome="failure",
          error_code="DB_DEADLOCK")
    event(e, "billing", 38, "Invoice update transaction retried", trace="trc-db02", span="b-db1",
          request="req-db2", entity="inv-db1", fmt="node_pino", level="WARN",
          attrs={"retry_count": 1})
    return e, truth("database_concurrency", "database transaction deadlock caused by conflicting lock order",
                    "postgres/transaction locks", symptoms=["orders", "billing"],
                    expected=["database deadlock record is a common causal signal", "two separate traces share database contention"],
                    confidence="medium_high")


def make_duplicate_delivery() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "orders", 0, "Order event published", trace="trc-q01", span="o-q1", request="req-q1",
          entity="ord-q1", event_name="order.created", fmt="node_pino",
          attrs={"message_id": "msg-duplicate-01", "topic": "orders"})
    event(e, "stock-consumer", 70, "Message received", trace="trc-q01", span="c-q1", parent="o-q1",
          request="req-q1", entity="ord-q1", fmt="python_text",
          attrs={"message_id": "msg-duplicate-01", "delivery_attempt": 1})
    event(e, "stock", 95, "Inventory reservation created", trace="trc-q01", span="s-q1",
          parent="c-q1", request="req-q1", entity="ord-q1", outcome="success",
          attrs={"reservation_id": "res-001", "message_id": "msg-duplicate-01"})
    event(e, "stock-consumer", 150, "Message received after redelivery", trace="trc-q01", span="c-q2",
          parent="o-q1", request="req-q1", entity="ord-q1", fmt="otel",
          attrs={"message_id": "msg-duplicate-01", "delivery_attempt": 2, "ack_state": "unknown"})
    event(e, "stock", 180, "Second inventory reservation created", trace="trc-q01", span="s-q2",
          parent="c-q2", request="req-q1", entity="ord-q1", level="ERROR", outcome="failure",
          error_code="DUPLICATE_RESERVATION", attrs={"reservation_id": "res-002",
          "message_id": "msg-duplicate-01", "idempotency_key": None})
    return e, truth("async_delivery_and_idempotency", "consumer side effect is not idempotent across message redelivery",
                    "stock-consumer/message msg-duplicate-01", symptoms=["stock"],
                    amplifiers=["missing or ineffective idempotency key"],
                    expected=["same message ID delivered twice", "two distinct reservation IDs for one order"])


def make_business_invariant() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    t = "trc-biz1"; r = "req-biz1"; o = "ord-biz1"
    event(e, "gateway", 0, "POST /checkout returned 200", trace=t, span="g-biz", request=r,
          entity=o, fmt="nginx_json", attrs={"status": 200, "request_time": 0.125})
    event(e, "payments", 25, "Payment captured successfully", trace=t, span="p-biz",
          parent="g-biz", request=r, entity=o, outcome="success", fmt="otel",
          attrs={"payment_id": "pay-biz1", "amount_minor": 129900})
    event(e, "orders", 50, "Order accepted; response emitted", trace=t, span="o-biz",
          parent="g-biz", request=r, entity=o, outcome="success",
          attrs={"persisted_status": "PENDING", "expected_status": "PAID"})
    event(e, "reconciliation", 5000, "Payment exists but order remains pending", trace=t,
          span="r-biz", request=r, entity=o, fmt="python_text", level="ERROR",
          error_code="ORDER_PAYMENT_STATE_MISMATCH",
          attrs={"payment_status": "CAPTURED", "order_status": "PENDING"})
    return e, truth("business_logic_consistency", "order state transition failed after payment capture",
                    "order ord-biz1", symptoms=["orders", "reconciliation"],
                    expected=["HTTP 200 is not treated as business success", "payment captured conflicts with order pending"])


def make_unrelated_concurrent() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "identity", 0, "Login rejected: invalid password", trace="trc-unrel-a",
          span="a-unrel", request="req-login-10", level="WARN", outcome="failure",
          error_code="INVALID_PASSWORD", fmt="spring_text")
    event(e, "media", 3, "Object storage upload timed out", trace="trc-unrel-b",
          span="m-unrel", request="req-media-20", level="ERROR", outcome="failure",
          error_code="OBJECT_STORE_TIMEOUT", fmt="node_pino")
    event(e, "identity", 14, "Login rejected: invalid password", trace="trc-unrel-c",
          span="a-unrel2", request="req-login-11", level="WARN", outcome="failure",
          error_code="INVALID_PASSWORD")
    return e, truth("multiple_independent_events", "two unrelated failures occurred in overlapping time windows",
                    "identity login attempts and media upload are separate incidents",
                    symptoms=["identity", "media"], expected=["two incident candidates", "no causal edge from identity to media"],
                    must_not=["identity→media causal relationship"])


def make_missing_trace_business_id() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "gateway", 0, "Checkout started", span=None, request=None, entity="ord-miss1",
          fmt="unknown_json", attrs={"http_method": "POST", "path": "/checkout"})
    event(e, "orders", 40, "Order record created", span=None, request=None, entity="ord-miss1",
          trace=None, fmt="python_text", attrs={"order_id": "ord-miss1"})
    event(e, "payments", 100, "Gateway timeout", span=None, request=None, entity="ord-miss1",
          trace=None, fmt="java_json", level="ERROR", error_code="GATEWAY_TIMEOUT",
          attrs={"order_id": "ord-miss1"})
    return e, truth("partial_correlation", "payment timeout is associated with the same order, but trace reconstruction is unavailable",
                    "ord-miss1", symptoms=["payments"],
                    expected=["business entity links the three events with limited confidence", "missing trace is surfaced as telemetry quality issue"],
                    confidence="medium")


def make_clock_skew() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    t = "trc-clock1"
    event(e, "gateway", 0, "Request dispatched", trace=t, span="g-clock", request="req-clock1")
    # The payment host clock is 90 seconds slow; its causal parent relation remains explicit.
    event(e, "payments", -90000, "Payment authorization started", trace=t, span="p-clock",
          parent="g-clock", request="req-clock1", fmt="otel",
          attrs={"host_clock_offset_ms": -90000})
    event(e, "payments", -89920, "Payment authorization succeeded", trace=t, span="p-clock",
          parent="g-clock", request="req-clock1", outcome="success", duration=80,
          fmt="otel", attrs={"host_clock_offset_ms": -90000})
    event(e, 140, "Order confirmed", trace=t, span="o-clock", parent="g-clock",
          request="req-clock1", outcome="success")
    return e, truth("telemetry_clock_skew", "payment host clock is approximately 90 seconds behind",
                    "payments host", expected=["span relationship is preserved despite timestamp inversion",
                    "event time and observed/collector time remain distinct"],
                    confidence="high")


def make_queue_lag() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "orders", 0, "Order event published", trace="trc-lag1", span="o-lag",
          request="req-lag1", entity="ord-lag1", fmt="node_pino",
          attrs={"message_id": "msg-lag1", "queue": "orders.created"})
    event(e, "queue-monitor", 1500, "Consumer lag rising", fmt="otel", level="WARN",
          attrs={"queue": "orders.created", "lag_messages": 22000, "oldest_age_ms": 145000})
    event(e, "fulfilment", 150000, "Message processing started", trace="trc-lag1",
          span="f-lag", parent="o-lag", request="req-lag1", entity="ord-lag1",
          attrs={"message_id": "msg-lag1", "queue_age_ms": 149500})
    event(e, "fulfilment", 150300, "Fulfilment completed", trace="trc-lag1",
          span="f-lag", parent="o-lag", request="req-lag1", entity="ord-lag1",
          outcome="success", duration=300)
    return e, truth("queue_backlog", "asynchronous consumer throughput fell behind incoming event volume",
                    "orders.created consumer group", symptoms=["fulfilment"],
                    expected=["producer success is separated from delayed consumer completion", "lag precedes delayed processing"])


def make_resource_pressure() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    for n, (ms, heap, gc, level) in enumerate([
        (0, 62, 4, "INFO"), (10000, 74, 15, "INFO"), (20000, 88, 120, "WARN"),
        (30000, 96, 900, "WARN"), (35000, 99, 2400, "ERROR")], start=1):
        event(e, "catalog", ms, "JVM health sample", fmt="java_json", level=level,
              instance="catalog-03", version="7.1.0",
              attrs={"heap_used_pct": heap, "gc_pause_ms": gc, "active_threads": 420+n*4,
                     "request_p99_ms": 80 if n < 3 else 900+n*300})
    event(e, "catalog", 36000, "OutOfMemoryError: Java heap space", fmt="spring_text",
          level="FATAL", error_code="JAVA_HEAP_OOM", instance="catalog-03", version="7.1.0")
    return e, truth("resource_exhaustion", "catalog instance experiences progressive heap exhaustion",
                    "catalog-03", symptoms=["catalog requests"],
                    expected=["heap and GC degradation precede fatal OOM", "single instance scope retained"])


def make_tenant_flag() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    for n, tenant in enumerate(("tenant-green", "tenant-blue", "tenant-green", "tenant-blue"), start=1):
        bad = tenant == "tenant-blue"
        event(e, "pricing", n*30, "Price calculation failed" if bad else "Price calculation completed",
              trace=f"trc-tenant{n}", span=f"pr{n}", request=f"req-tenant{n}",
              tenant=tenant, level="ERROR" if bad else "INFO",
              outcome="failure" if bad else "success",
              error_code="RULESET_NOT_FOUND" if bad else None,
              attrs={"feature_flag": "dynamic-pricing", "flag_version": "v12" if bad else "v11",
                     "ruleset": "blue-v12" if bad else "green-v11"}, fmt="node_pino")
    return e, truth("configuration_feature_flag", "dynamic-pricing v12 references a missing ruleset for tenant-blue",
                    "tenant-blue/dynamic-pricing", symptoms=["tenant-blue pricing requests"],
                    expected=["tenant cohort comparison reveals scope", "tenant-green is a healthy control"],
                    must_not=["tenant-green"])


def make_shared_dns() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    for n, svc in enumerate(("orders", "identity", "notifications"), start=1):
        event(e, svc, n*8, "Name resolution failed for cache.internal",
              trace=f"trc-dns{n}", span=f"dns{n}", request=f"req-dns{n}",
              level="ERROR", error_code="EAI_AGAIN",
              attrs={"dependency_host": "cache.internal", "resolver": "10.0.0.53"},
              fmt="python_text")
    event(e, "node", 40, "CoreDNS SERVFAIL for cache.internal", fmt="postgres_text",
          level="ERROR", error_code="DNS_SERVFAIL",
          attrs={"qname": "cache.internal", "resolver": "10.0.0.53"})
    return e, truth("infrastructure_network", "shared DNS resolver fails to resolve cache.internal",
                    "resolver 10.0.0.53", symptoms=["orders", "identity", "notifications"],
                    expected=["multiple independent traces share one failed dependency name/resolver",
                    "DNS evidence precedes service symptoms"])


def make_schema_drift() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "checkout", 0, "Order event published", trace="trc-sch1", span="co-sch",
          entity="ord-sch1", request="req-sch1", attrs={"schema_version": 2, "amount": "1299.00",
          "producer_version": "5.0.0"}, fmt="otel")
    event(e, "fulfilment", 30, "Cannot deserialize amount: expected number, got string",
          trace="trc-sch1", span="fu-sch", parent="co-sch", entity="ord-sch1",
          request="req-sch1", level="ERROR", error_code="JSON_TYPE_MISMATCH",
          attrs={"field": "amount", "expected_type": "number", "actual_type": "string",
                 "consumer_version": "4.2.1"}, fmt="java_json")
    event(e, "fulfilment", 60, "Message moved to retry queue", trace="trc-sch1",
          span="fu-sch", parent="co-sch", entity="ord-sch1", request="req-sch1",
          level="WARN", attrs={"retry_count": 1})
    return e, truth("data_contract_schema", "producer changed amount from numeric to string without consumer compatibility",
                    "checkout→fulfilment event contract", symptoms=["fulfilment"],
                    expected=["producer and consumer versions/field types are visible",
                    "schema incompatibility precedes retry behavior"])


def make_poison_record() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "gateway", 0, "First valid record", trace="trc-poison1", span="g-po1", fmt="java_json")
    event(e, "orders", 10, "Order created", trace="trc-poison1", span="o-po1", parent="g-po1", fmt="node_pino")
    event(e, "gateway", 20, "Last valid record", trace="trc-poison1", span="g-po1", fmt="python_text")
    return e, truth("ingestion_resilience", "one malformed source record must be quarantined without blocking adjacent valid records",
                    "adapter/parser boundary", expected=["valid records before and after malformed record are persisted",
                    "malformed input is visible in quarantine and does not poison the stream"],
                    confidence="high")


def make_mixed_sparse() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "legacy-reporting", 0, "Started export job", fmt="python_text",
          trace=None, span=None, request="job-901", attrs={"exportId": "exp-901"})
    event(e, "reporting-worker", 500, "CSV written to object storage", fmt="unknown_json",
          trace=None, span=None, request="job-901", attrs={"app_name": "reporting-worker",
          "severity": "INFO", "export_id": "exp-901", "object_key": "exports/901.csv"})
    event(e, "object-store", 800, "PUT /exports/901.csv 201", fmt="nginx_json",
          trace=None, span=None, request="job-901", attrs={"status": 201, "request_time": 0.08})
    return e, truth("normalization_and_partial_correlation", "no incident; heterogeneous sparse records describe one export job",
                    "export job exp-901", expected=["formats normalize into common fields",
                    "export_id/job ID supports bounded association", "missing trace fields remain unknown"],
                    confidence="medium")


def make_two_incidents() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "identity", 0, "OIDC issuer mismatch", trace="trc-two-a", span="a-two",
          request="req-two-a", level="ERROR", error_code="OIDC_ISSUER_INVALID",
          attrs={"issuer": "https://wrong.example"}, fmt="spring_text")
    event(e, "payments", 5, "Provider returned 503", trace="trc-two-b", span="p-two",
          request="req-two-b", level="ERROR", error_code="PROVIDER_503",
          attrs={"provider": "pay.example", "http_status": 503}, fmt="node_pino")
    event(e, "identity", 20, "OIDC issuer mismatch", trace="trc-two-c", span="a-two2",
          request="req-two-c", level="ERROR", error_code="OIDC_ISSUER_INVALID")
    event(e, "payments", 25, "Provider returned 503", trace="trc-two-d", span="p-two2",
          request="req-two-d", level="ERROR", error_code="PROVIDER_503",
          attrs={"provider": "pay.example", "http_status": 503})
    return e, truth("multiple_incidents", "two independent concurrent causes: OIDC configuration and payment provider outage",
                    "identity config and pay.example", symptoms=["identity", "payments"],
                    expected=["two distinct incident groups", "separate hypotheses and evidence"],
                    must_not=["identity→payments causal relationship"])


def make_trace_collision() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e: list[dict[str, Any]] = []
    event(e, "gateway", 0, "Request A received", trace="trc-reused", span="g-reuse-a",
          request="req-A", entity="ord-A", fmt="java_json")
    event(e, "gateway", 2, "Request B received", trace="trc-reused", span="g-reuse-b",
          request="req-B", entity="ord-B", fmt="node_pino")
    event(e, "payments", 20, "Payment A failed", trace="trc-reused", span="p-reuse-a",
          parent="g-reuse-a", request="req-A", entity="ord-A", level="ERROR",
          error_code="CARD_DECLINED")
    event(e, "stock", 22, "Inventory B reservation succeeded", trace="trc-reused",
          span="s-reuse-b", parent="g-reuse-b", request="req-B", entity="ord-B",
          outcome="success")
    return e, truth("identity_collision", "trace ID is reused across two distinct requests",
                    "trc-reused", expected=["trace collision is surfaced", "request/business IDs partition the two flows"],
                    must_not=["Payment A failure correlated to Order B"])


def make_clock_and_retry() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    e, base_truth = make_provider_retry()
    # Introduce arrival disorder and a noisy duplicate at generation time.
    base_truth["notes"] = "Provider retry storm with default event-time ordering; use --shuffle-arrival and --duplicate-rate to stress delivery."
    return e, base_truth


CASES.update({
    "healthy-single": make_healthy_single,
    "healthy-six-service": make_healthy_fanout,
    "auth-cascade": make_auth_cascade,
    "provider-rate-limit-retry": make_provider_retry,
    "replica-config-drift": make_config_drift,
    "version-regression": make_release_regression,
    "database-deadlock": make_db_deadlock,
    "duplicate-message-side-effect": make_duplicate_delivery,
    "business-state-invariant": make_business_invariant,
    "unrelated-concurrent-failures": make_unrelated_concurrent,
    "missing-trace-business-link": make_missing_trace_business_id,
    "clock-skew-parent-span": make_clock_skew,
    "async-queue-backlog": make_queue_lag,
    "resource-pressure-oom": make_resource_pressure,
    "tenant-feature-flag-drift": make_tenant_flag,
    "shared-dns-failure": make_shared_dns,
    "producer-consumer-schema-drift": make_schema_drift,
    "poison-record-isolation": make_poison_record,
    "mixed-format-sparse-logs": make_mixed_sparse,
    "two-incidents-one-window": make_two_incidents,
    "trace-id-reuse": make_trace_collision,
})


def compact_fields(record: dict[str, Any]) -> dict[str, Any]:
    return {k: v for k, v in record.items() if v is not None}


def render(record: dict[str, Any]) -> str:
    ts = record["timestamp"]
    service = record["service"]
    level = record["level"]
    msg = record["message"]
    attrs = dict(record["attrs"])
    base = {
        "event_id": record["event_id"], "service": service, "service_version": record["version"],
        "host": record["instance"], "region": record["region"], "tenant_id": record["tenant"],
        "trace_id": record["trace_id"], "span_id": record["span_id"],
        "parent_span_id": record["parent_span_id"], "request_id": record["request_id"],
        "order_id": record["entity_id"], "event": record["event"],
        "outcome": record["outcome"], "duration_ms": record["duration_ms"],
        "error_code": record["error_code"],
    }
    if record["format"] == "java_json":
        obj = {"@timestamp": ts, "level": level, "logger": f"{service}.Handler",
               "message": msg, "service": service, "serviceVersion": record["version"],
               "host": record["instance"], "region": record["region"],
               "eventId": record["event_id"], "traceId": record["trace_id"],
               "spanId": record["span_id"], "parentSpanId": record["parent_span_id"],
               "requestId": record["request_id"], "orderId": record["entity_id"],
               "eventName": record["event"], "outcome": record["outcome"],
               "durationMs": record["duration_ms"], "errorCode": record["error_code"],
               "tenant": record["tenant"], **attrs}
        return json.dumps(compact_fields(obj), separators=(",", ":"), ensure_ascii=False)
    if record["format"] == "node_pino":
        obj = {"time": int((BASE + timedelta(milliseconds=record["offset_ms"])).timestamp()*1000),
               "level": LEVELS[level], "msg": msg, "service.name": service,
               "service.version": record["version"], "host.name": record["instance"],
               "event_id": record["event_id"], "trace_id": record["trace_id"],
               "span_id": record["span_id"], "parent_span_id": record["parent_span_id"],
               "correlationId": record["request_id"], "order_id": record["entity_id"],
               "event": record["event"], "outcome": record["outcome"],
               "duration_ms": record["duration_ms"], "err_code": record["error_code"],
               "tenant_id": record["tenant"], **attrs}
        return json.dumps(compact_fields(obj), separators=(",", ":"), ensure_ascii=False)
    if record["format"] == "otel":
        obj = {"timestamp": ts, "severity_text": level, "body": msg,
               "resource": {"service.name": service, "service.version": record["version"],
                            "host.name": record["instance"], "cloud.region": record["region"]},
               "trace_id": record["trace_id"], "span_id": record["span_id"],
               "parent_span_id": record["parent_span_id"],
               "attributes": compact_fields({"event.id": record["event_id"],
                  "request.id": record["request_id"], "order.id": record["entity_id"],
                  "event.name": record["event"], "outcome": record["outcome"],
                  "duration_ms": record["duration_ms"], "error.code": record["error_code"],
                  "tenant.id": record["tenant"], **attrs})}
        return json.dumps(obj, separators=(",", ":"), ensure_ascii=False)
    if record["format"] == "unknown_json":
        obj = {"ts": ts, "severity": level, "app_name": service, "log": msg,
               "node": record["instance"], "build": record["version"],
               "correlationID": record["request_id"], "trace": record["trace_id"],
               "span": record["span_id"], "order": record["entity_id"],
               "eventId": record["event_id"], "fields": attrs}
        return json.dumps(compact_fields(obj), separators=(",", ":"), ensure_ascii=False)
    if record["format"] == "nginx_json":
        obj = {"time_iso8601": ts, "service": service, "host": record["instance"],
               "request": attrs.pop("request", "POST /checkout"),
               "status": attrs.pop("status", 200),
               "request_time": attrs.pop("request_time", record["duration_ms"] / 1000 if record["duration_ms"] is not None else 0.1),
               "request_id": record["request_id"], "trace_id": record["trace_id"],
               "order_id": record["entity_id"], "message": msg, **attrs}
        return json.dumps(compact_fields(obj), separators=(",", ":"), ensure_ascii=False)
    if record["format"] == "postgres_text":
        suffix = " ".join(f"{k}={v}" for k, v in attrs.items())
        return f'{ts} [{attrs.get("pid", 1001)}] {level}: {msg} app={service} host={record["instance"]} {suffix}'.strip()
    if record["format"] == "spring_text":
        meta = []
        for key, value in (("traceId", record["trace_id"]), ("spanId", record["span_id"]),
                           ("parentSpanId", record["parent_span_id"]), ("requestId", record["request_id"]),
                           ("orderId", record["entity_id"]), ("version", record["version"])):
            if value is not None:
                meta.append(f"{key}={value}")
        meta.extend(f"{k}={v}" for k, v in attrs.items())
        return f'{ts} {level:<5} [{service},{record["instance"]}] {service}.Handler : {msg} {" ".join(meta)}'.strip()
    # Python logging-style text.
    meta = compact_fields({"trace_id": record["trace_id"], "span_id": record["span_id"],
                           "parent_span_id": record["parent_span_id"], "request_id": record["request_id"],
                           "order_id": record["entity_id"], "version": record["version"],
                           "outcome": record["outcome"], "duration_ms": record["duration_ms"],
                           "error_code": record["error_code"], **attrs})
    suffix = " ".join(f"{k}={json.dumps(v, ensure_ascii=False) if isinstance(v, (dict,list,str)) else v}"
                      for k, v in meta.items())
    return f'{ts} {level} {service} [{record["instance"]}] {msg} {suffix}'.strip()


def build_case(name: str, seed: int, shuffle: bool, duplicate_rate: float,
               drop_optional_rate: float, permutation: int) -> tuple[list[str], dict[str, Any], dict[str, Any]]:
    events, expected = CASES[name]()
    rng = random.Random(seed + permutation * 1009)
    records = copy.deepcopy(events)

    dropped: list[dict[str, str]] = []
    for item in records:
        for field in OPTIONAL_FIELDS:
            if item.get(field) is not None and rng.random() < drop_optional_rate:
                item[field] = None
                dropped.append({"event_id": item["event_id"], "field": field})

    duplicate_ids: list[str] = []
    original = list(records)
    for item in original:
        if rng.random() < duplicate_rate:
            records.append(copy.deepcopy(item))
            duplicate_ids.append(item["event_id"])

    if shuffle:
        rng.shuffle(records)
    else:
        records.sort(key=lambda x: x["offset_ms"])

    raw = [render(x) for x in records]
    malformed_count = 0
    if name == "poison-record-isolation":
        raw.insert(min(2, len(raw)), '{"timestamp":"2026-11-25T12:00:00.015Z","level": ERROR, "message": "truncated JSON"')
        malformed_count = 1
    manifest = {
        "scenario": name, "seed": seed, "permutation": permutation,
        "records": len(raw), "malformed_records": malformed_count, "formats": sorted({x["format"] for x in events}),
        "arrival_shuffled": shuffle, "duplicated_event_ids": duplicate_ids,
        "dropped_optional_fields": dropped,
        "raw_is_the_only_directory_to_mount_into_fluent_bit": True,
    }
    return raw, expected, manifest


def write_one(name: str, output: Path, truth_dir: Path, seed: int, shuffle: bool,
              duplicate_rate: float, drop_optional_rate: float, permutation: int) -> dict[str, Any]:
    raw, expected, manifest = build_case(name, seed, shuffle, duplicate_rate,
                                         drop_optional_rate, permutation)
    stem = f"{name}__p{permutation:03d}"
    raw_dir = output / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True)
    truth_dir.mkdir(parents=True, exist_ok=True)
    (raw_dir / f"{stem}.log").write_text("\n".join(raw) + "\n", encoding="utf-8")
    (truth_dir / f"{stem}.json").write_text(
        json.dumps({"scenario": name, "ground_truth": expected}, indent=2) + "\n",
        encoding="utf-8")
    try:
        (truth_dir / f"{stem}.json").chmod(0o600)
    except OSError:
        pass
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate ObservaLog runtime incident streams.")
    parser.add_argument("--list", action="store_true", help="list available scenarios")
    parser.add_argument("--scenario", choices=sorted(CASES), help="generate one scenario")
    parser.add_argument("--all", action="store_true", help="generate every scenario")
    parser.add_argument("--permutations", type=int, default=1, help="arrival permutations per scenario")
    parser.add_argument("--seed", type=int, default=20261125, help="deterministic random seed")
    parser.add_argument("--shuffle-arrival", action="store_true", help="randomize arrival order without changing event timestamps")
    parser.add_argument("--duplicate-rate", type=float, default=0.0, help="duplicate probability per record (0..1)")
    parser.add_argument("--drop-optional-rate", type=float, default=0.0, help="randomly remove optional correlation/metadata fields (0..1)")
    parser.add_argument("--output", type=Path, default=Path("incident-output"), help="output root; raw and ground-truth are separated")
    args = parser.parse_args()

    if args.list:
        for name in sorted(CASES):
            print(name)
        return
    if not args.scenario and not args.all:
        parser.error("choose --scenario NAME, --all, or --list")
    if args.permutations < 1:
        parser.error("--permutations must be at least 1")
    if not 0 <= args.duplicate_rate <= 1 or not 0 <= args.drop_optional_rate <= 1:
        parser.error("mutation rates must be between 0 and 1")

    selected = sorted(CASES) if args.all else [args.scenario]
    manifests = []
    for name in selected:
        for permutation in range(args.permutations):
            manifests.append(write_one(name, args.output, args.output / "ground-truth",
                                       args.seed, args.shuffle_arrival, args.duplicate_rate,
                                       args.drop_optional_rate, permutation))
    (args.output / "manifest.json").write_text(json.dumps(manifests, indent=2) + "\n", encoding="utf-8")
    print(f"Generated {len(manifests)} streams under {args.output}")
    print(f"Mount only {args.output / 'raw'} into Fluent Bit; keep {args.output / 'ground-truth'} outside the collector.")


if __name__ == "__main__":
    main()
