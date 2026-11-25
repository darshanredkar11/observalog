# Event Grammar — Legacy SDK Reference

> **Legacy reference only — not used by ObservaLog Lite.**

This document describes the former SDK / fixed wire-contract architecture. It is retained as historical migration context and is not a deployment or ingestion requirement for the `lite/streaming-adapter` branch.

This grammar constrained event names emitted by the former ObservaLog SDK/logscanner. ObservaLog Lite accepts arbitrary source event names and does not reject records because they fall outside this grammar.

For current adapter behavior and canonical fields, see [the lite runtime guide](lite/README.md).

The active runtime uses [ObservaLog Lite](lite/README.md): Fluent Bit sends original log records over HTTP to the brain, the brain normalizes them into an optional canonical event model, and TimescaleDB is the durable journal and gate-work store. Kafka, Valkey, mandatory SDKs and the old fixed A/B two-line contract are not required.
