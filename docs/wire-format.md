# A/B Wire Format — Legacy SDK Reference

> **Legacy reference only — not used by ObservaLog Lite.**

This document describes the former SDK / fixed wire-contract architecture. It is retained as historical migration context and is not a deployment or ingestion requirement for the `lite/streaming-adapter` branch.

The fixed-position Part A plus abbreviated Part B protocol belonged to the former SDK ingestion path. It is not the wire contract for ObservaLog Lite.

Lite input is the original JSON/text record sent by Fluent Bit to the brain HTTP endpoint. The adapter maps only fields it can reliably identify, retains the original raw record and provenance, and leaves unknown data absent. It does not require two paired stdout lines, a fixed service code or mandatory trace/outcome/duration values.

See [the Lite architecture](lite/README.md) and [the replay/recovery runbook](lite/replay-and-recovery.md).

The active runtime uses [ObservaLog Lite](lite/README.md): Fluent Bit sends original log records over HTTP to the brain, the brain normalizes them into an optional canonical event model, and TimescaleDB is the durable journal and gate-work store. Kafka, Valkey, mandatory SDKs and the old fixed A/B two-line contract are not required.
