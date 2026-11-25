# ObservaLog Lite — Discovery and Design

This directory records the discovery phase for the lite branch of the parent `observalog` repository. It is deliberately grounded in the existing in-repository brain, Fluent Bit configuration and TimescaleDB schema. No independent brain repository is used.

## Work completed in this phase

1. Audited the existing brain and the actual SDK → Fluent Bit → Kafka → brain → TimescaleDB path.
2. Recorded current limitations and specific mismatches in [current-brain-audit.md](current-brain-audit.md).
3. Defined isolated streaming responsibilities and resilience invariants in [gate-responsibilities.md](gate-responsibilities.md).
4. Defined the main evidence-led business reasoning gate in [reasoning-model.md](reasoning-model.md).
5. Added a runtime incident generator with 22 heterogeneous scenarios, source-format variation, 1-to-12 service cases, arrival permutations, duplicate delivery and missing optional fields under [tests/incident_lab](../../tests/incident_lab/README.md).

## Current branch state

Branch: `lite/streaming-adapter`

The branch is a safe discovery baseline forked from `main`. The old SDKs, Kafka/Valkey dependencies and current brain implementation have not yet been removed. They remain present until their replacement path is implemented and validated; pruning them now would leave the branch without a working ingestion path.

The next implementation stage is to replace the legacy two-line parser/collector boundary with raw-log adapters, define the canonical optional-field contract, and make TimescaleDB a durable replay source for independent gates. Then the branch can be physically reduced to the required Fluent Bit, brain, TimescaleDB, adapter and incident-lab files.

## Non-negotiable constraints

- Do not alter `main` or merge the existing documentation PR.
- Do not use the independent `observalog-brain` repository.
- Do not require every source to emit a complete ObservaLog contract.
- Never fabricate trace IDs, parent spans, outcomes or durations to satisfy schema constraints.
- Preserve raw log evidence and field provenance.
- Keep gate queues bounded and gate communication asynchronous.
- Do not put LLM calls on the ingestion hot path.
- Keep scenario ground truth outside Fluent Bit's watched paths and outside the brain's inputs.
