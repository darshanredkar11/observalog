# ObservaLog — Contract Reference

This file is the ground truth for what every log must contain.
The Go library enforces structural fields automatically.
The scanner enforces positional fields at CI time.

---

## Ambient fields — library auto-injects, developer writes zero

| Field | Go type | Source | Contract |
|-------|---------|--------|----------|
| trace_id | string CHAR(14) | ctx | trc_ + 10 chars. Born at gateway. Fallback: sys_<uuid> |
| span_id | string CHAR(7) | ctx | spn_ + 3 chars. Generated at service entry. |
| parent_span_id | string CHAR(7) | ctx | null at origin. Propagated from calling service. |
| seq | uint32 | library | Service-local. atomic.AddUint32. Starts at 1. |
| user_id | string | ctx | From JWT sub claim. Fallback: "system" |
| tenant_id | string | ctx | Optional. Set by JWT middleware if multi-tenant. |
| journey_stage | string | ctx | Set by boundary middleware. Immutable. |
| service | string | init | From SERVICE_NAME env var. |
| version | string | init | semver-gitsha. From ldflags. |
| host | string | init | From HOST env var or os.Hostname(). |
| env | string | init | production / staging / dev |
| ts | time.Time | library | RFC3339 + millis, UTC. Generated at Emit(). |

---

## Positional fields — developer supplies

| Field | Required when | Contract |
|-------|--------------|----------|
| event | always | domain.object.action grammar. Scanner blocks violations. |
| message | always | Past tense. Human-readable. Min 10 chars. Not identical to event. |
| level | always | Debug/Info/Warn/Error enum. Error = actionable always. |
| outcome | decision points, exits | success/failure/partial/pending enum. |
| duration_ms | when outcome present | Wall clock of this operation. Scanner blocks outcome without duration_ms. |
| error | level=Error or outcome=failure | log.Err struct. Never string. |
| ctx | always | Domain keys. 1 level deep max. Keys from dict.go. |

---

## Event grammar

Format: `domain.object.action`

Domains (finite):
- auth
- doc
- provider
- infra

Action vocabulary (15 verbs only):
received, validated, rejected, published, failed,
exhausted, expired, attempted, succeeded, created,
updated, deleted, queried, connected, disconnected

Valid examples:
- auth.jwt.validated
- doc.document.received
- provider.send.rejected
- infra.db.query_slow

---

## log.Err struct

```go
type Err struct {
    Kind      string // error class: "RateLimitExceeded", "NetworkError", "ValidationError"
    Code      string // machine-readable constant: "PROVIDER_QUOTA_EXCEEDED"
    Message   string // err.Error() — human readable
    Retryable bool   // determines brain fix strategy
}
```

Always use typed Err struct. Never pass error as string.

---

## Three log call shapes (complete reference)

### Shape 1 — Informational
```go
observalog.Info(ctx, "doc.storage.saved", "Document written to storage", log.F{
    "doc_id":  docID,
    "bytes":   n,
    "backend": "postgres",
})
```

### Shape 2 — Decision or exit point
```go
observalog.Info(ctx, "auth.permission.checked", "Permission granted for doc:send", log.F{
    "doc_id":      docID,
    "permission":  "doc:send",
    "outcome":     outcomes.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})
```

### Shape 3 — Failure
```go
observalog.Error(ctx, "provider.send.rejected", "Provider rejected document send", log.F{
    "doc_id":      docID,
    "provider":    "sendgrid",
    "http_status": 429,
    "outcome":     outcomes.Failure,
    "duration_ms": time.Since(start).Milliseconds(),
    "error": log.Err{
        Kind:      "RateLimitExceeded",
        Code:      "PROVIDER_QUOTA_EXCEEDED",
        Message:   err.Error(),
        Retryable: true,
    },
})
```

---

## Part A wire format (compact header)

```
A:{schema_v}|{trace_id}|{span_id}|{parent_span}|{seq}|{svc}|{lvl}|{out}|{ts_ms}
```

Example:
```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
```

Byte positions (0-indexed after "A:"):
- [0] schema_version — single digit
- [2..16] trace_id — 14 chars
- [17] pipe separator
- [18..24] span_id — 7 chars
- [25] pipe
- [26..32] parent_span — 7 chars or "-------"
- [33] pipe
- [34..35] seq — 2 chars zero-padded uint8
- [36] pipe
- [37] svc — 1 char: 0=system,1=auth,2=doc,3=provider
- [38] pipe
- [39] lvl — 1 char: 0=debug,1=info,2=warn,3=error
- [40] pipe
- [41] out — 1 char: 0=none,1=success,2=failure,3=partial
- [42] pipe
- [43..55] ts_ms — 13 chars unix millis

## Part B abbreviation dictionary

```
e  → event          m  → message        ms → duration_ms
c  → ctx            o  → outcome
er → error          ek → error.kind      ec → error.code
em → error.message  rt → error.retryable
di → doc_id         tp → topic           pt → partition
of → offset         pr → provider        hs → http_status
ui → user_id        js → journey_stage
```

Never add an abbreviation without updating dict.go AND checking for collisions.
