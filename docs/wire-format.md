# ObservaLog Wire Format Specification

**Schema Version 1 — frozen**

---

## Overview

Every `observalog.Info()` / `observalog.Error()` call emits exactly two NDJSON lines to stdout:

```
A:{schema_v}|{trace_id}|{span_id}|{parent_span}|{seq}|{svc}|{lvl}|{out}|{ts_ms}
B:{abbreviated JSON payload}
```

Two lines always arrive together, in order. The brain parses them as a pair. The Fluent Bit tail plugin forwards both to Kafka. The Kafka consumer in the brain reads A then B.

---

## Part A — Fixed-Position Header

Part A is exactly **55 bytes** (excluding the trailing newline), starting with the literal prefix `A:`.

All byte offsets are 0-indexed relative to the start of the content **after** the `A:` prefix.

### Byte layout

```
Offset  Length  Field           Description
──────  ──────  ─────────────── ──────────────────────────────────────────
0       1       schema_version  Integer character. Currently '1'. Unknown versions fall back to full JSON.
1       1       |               Separator
2       14      trace_id        14-char string. Format: trc_<10 hex chars>. sys_ prefix for missing context.
16      1       |               Separator
17      7       span_id         7-char string. Format: spn_<3 hex chars>.
24      1       |               Separator
25      7       parent_span     7-char string. Parent span_id, or "-------" (7 dashes) if this is the root span.
32      1       |               Separator
33      2       seq             Zero-padded 2-char hex (uint8). Per-service atomic counter, resets per request.
35      1       |               Separator
36      1       svc             1-char service code (see Service Codes below).
37      1       |               Separator
38      1       lvl             1-char level code (see Level Codes below).
39      1       |               Separator
40      1       out             1-char outcome code (see Outcome Codes below).
41      1       |               Separator
42      13      ts_ms           13-char Unix timestamp in milliseconds. Zero-padded.
55      1       \n              Newline (not counted in PartAByteLen)
```

### Example

```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
```

Decoded:

| Field         | Raw bytes | Value              |
|---------------|-----------|--------------------|
| schema_version | `1`      | v1 (current)       |
| trace_id       | `trc_7f2a1b9e4d` | trace ID    |
| span_id        | `spn_004` | span 4             |
| parent_span    | `spn_001` | parent is span 1   |
| seq            | `04`      | 4th emit this request |
| svc            | `2`       | doc-service        |
| lvl            | `1`       | INFO               |
| out            | `1`       | success            |
| ts_ms          | `1748268153812` | Unix ms timestamp |

### Service Codes

| Code | Service        |
|------|---------------|
| `0`  | system/infra  |
| `1`  | auth-service  |
| `2`  | doc-service   |
| `3`  | provider-service |

### Level Codes

| Code | Level |
|------|-------|
| `0`  | DEBUG |
| `1`  | INFO  |
| `2`  | WARN  |
| `3`  | ERROR |

### Outcome Codes

| Code | Outcome  |
|------|----------|
| `0`  | none (not a decision point) |
| `1`  | success  |
| `2`  | failure  |
| `3`  | partial  |
| `4`  | pending  |

### Reading Part A in Rust (brain)

The brain reads Part A at hard-coded byte offsets — zero JSON parsing:

```rust
let trace_id = &line[2..16];          // bytes 2–15
let span_id  = &line[17..24];         // bytes 17–23
let parent   = &line[25..32];         // bytes 25–31
let seq      = u8::from_str_radix(&line[33..35], 16)?;
let service  = line.as_bytes()[36] - b'0';
let level    = line.as_bytes()[38] - b'0';
let outcome  = line.as_bytes()[40] - b'0';
let ts_ms    = line[42..55].parse::<i64>()?;
```

---

## Part B — Abbreviated JSON Payload

Part B is a single JSON object where every field name is replaced with a one- or two-character abbreviation defined in `dict.go` / `parser.rs`.

### Abbreviation Dictionary (DictVersion 1)

| Full field name  | Abbreviation | Type        | Notes |
|-----------------|--------------|-------------|-------|
| `event`         | `e`          | string      | Required. `domain.object.action` grammar. |
| `message`       | `m`          | string      | Required. Min 10 chars, past tense. |
| `duration_ms`   | `ms`         | integer     | Required when `outcome` is present. |
| `ctx`           | `c`          | object      | Developer context fields. |
| `outcome`       | `o`          | string      | `success`/`failure`/`partial`/`pending` |
| `error`         | `er`         | object      | Required when level=ERROR or outcome=failure. |
| `error.kind`    | `ek`         | string      | Error kind/type name. |
| `error.code`    | `ec`         | string      | Machine-readable error code. |
| `error.message` | `em`         | string      | Human-readable error message. |
| `error.retryable` | `rt`       | boolean     | Whether a retry may succeed. |
| `doc_id`        | `di`         | string      | Document identifier. |
| `topic`         | `tp`         | string      | Kafka topic. |
| `partition`     | `pt`         | integer     | Kafka partition. |
| `offset`        | `of`         | integer     | Kafka offset. |
| `provider`      | `pr`         | string      | External provider name. |
| `http_status`   | `hs`         | integer     | HTTP response status code. |
| `user_id`       | `ui`         | string      | User identifier. |
| `journey_stage` | `js`         | string      | Three-segment journey path. |

### Example Part B

Full (unabbreviated) log call:

```go
log.Error(ctx, "provider.send.rejected", "Provider rejected document send", log.F{
    "doc_id":      "doc123",
    "outcome":     log.Failure,
    "duration_ms": 87,
    "error": &log.Err{
        Kind:      "RateLimitExceeded",
        Code:      "PROVIDER_QUOTA_EXCEEDED",
        Retryable: true,
    },
})
```

Emitted Part B:

```json
{"e":"provider.send.rejected","m":"Provider rejected document send","er":{"ek":"RateLimitExceeded","ec":"PROVIDER_QUOTA_EXCEEDED","rt":true},"c":{"di":"doc123"},"o":"failure","ms":87}
```

### Dictionary contract rules

1. Every abbreviation must be unique across the dictionary — `ValidateNoDictCollisions()` panics at library Init if violated.
2. The dictionary is versioned (`DictVersion = 1`). `DictVersion` must increment on any change.
3. Both `dict.go` (Go library) and `parser.rs` (Rust brain) must contain identical mappings. They are a cross-language contract.
4. Any field name not in the dictionary is emitted unabbreviated under the `c` (ctx) key.

---

## Compression

The Go library writes uncompressed NDJSON to stdout. Fluent Bit applies GZIP before forwarding to Kafka. The application has no involvement in compression.

After GZIP, the size difference between this abbreviated NDJSON format and a binary protocol (e.g. MessagePack) is under 10 bytes per entry. NDJSON was chosen precisely because universal tooling compatibility (`jq`, `grep`, Kibana, any log viewer) matters more than the marginal size difference.

---

## Schema Versioning

Byte 0 of Part A is the schema version character. When the brain encounters an unknown schema version, it falls back to full JSON parsing for both parts. This allows rolling deploys: a new brain version that introduces a schema change can coexist with old producers still emitting v1 format.

Current schema version: **1** (frozen).

---

## Fingerprint

For log entries containing an error, a fingerprint is computed at emit time:

```
fingerprint = xxHash64(service_byte | event | error_code | ctx_primary_key)
```

- `service_byte` is the raw byte value of the service code (not its ASCII digit representation)
- Fields are joined with `|` separators
- Result is stored as `BIGINT` in `log_index`
- This exact algorithm must be reproduced identically in Go (`fingerprint.go`) and Rust (`parser.rs`)

---

*Wire format contracts are frozen at schema version 1. Changes require a new schema version byte and a corresponding brain fallback path.*
