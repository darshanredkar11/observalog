# observalog-go

**Structured log emission library for Go microservices.**

Import this library into any Go service that participates in the ObservaLog contract. It handles all structural fields automatically, enforces typed errors, and emits the two-line NDJSON wire format expected by the observalog-brain.

---

## Install

```bash
go get github.com/darshanredkar11/observalog-go
```

---

## Quick start

```go
import log "github.com/darshanredkar11/observalog-go"

func main() {
    log.Init(log.ConfigFromEnv("v1.2.3-abc123def"))
    defer log.Shutdown()

    http.ListenAndServe(":8080", middleware.Middleware(yourRouter))
}
```

---

## Initialization

### ConfigFromEnv

```go
cfg := log.ConfigFromEnv(version string) Config
```

Reads from environment:

| Variable          | Required | Description |
|------------------|----------|-------------|
| `SERVICE_NAME`   | yes      | Must match a registered service code (`auth`, `doc`, `provider`, `infra`) |
| `ENV`            | yes      | `production`, `staging`, or `development` |
| `LOG_LEVEL`      | no       | Minimum level to emit. Default: `info` |
| `LOG_BUFFER_SIZE`| no       | Async channel buffer size. Default: `10000` |

### Init

```go
err := log.Init(cfg)
```

- Validates configuration
- Sets minimum log level
- Calls `ValidateNoDictCollisions()` — panics if the abbreviation dictionary has conflicts
- Starts the async drain goroutine that writes to stdout

### Shutdown

```go
log.Shutdown()
```

Flushes the async channel and closes the drain goroutine. Call with `defer` at program exit.

---

## Emit functions

All four functions share the same signature:

```go
log.Debug(ctx context.Context, event string, message string, fields log.F)
log.Info(ctx  context.Context, event string, message string, fields log.F)
log.Warn(ctx  context.Context, event string, message string, fields log.F)
log.Error(ctx context.Context, event string, message string, fields log.F)
```

- `event` — `domain.object.action` grammar (validated by logscanner at CI time)
- `message` — human-readable description, past tense, minimum 10 characters
- `fields` — `log.F` (alias for `map[string]interface{}`)

---

## Field types

### log.F

```go
type F map[string]interface{}
```

Arbitrary key-value context. Fields with known names (see abbreviation dictionary) are abbreviated on the wire.

### log.Err

Typed error struct. Required for `log.Error()` calls and any `outcome=failure` log. Never use a string for the error field.

```go
type Err struct {
    Kind      string // Error kind/type name (e.g. "RateLimitExceeded")
    Code      string // Machine-readable code (e.g. "PROVIDER_QUOTA_EXCEEDED")
    Message   string // Human-readable description
    Retryable bool   // Whether a retry may succeed
}
```

Usage:

```go
log.Error(ctx, "provider.send.rejected", "Provider rejected document send", log.F{
    "error": &log.Err{
        Kind:      "RateLimitExceeded",
        Code:      "PROVIDER_QUOTA_EXCEEDED",
        Retryable: true,
    },
    "outcome":     log.Failure,
    "duration_ms": elapsed.Milliseconds(),
})
```

### Outcome constants

```go
log.Success  // "success"
log.Failure  // "failure"
log.Partial  // "partial"
log.Pending  // "pending"
```

---

## Three emit shapes

### 1. Informational (no outcome)

For internal steps, background jobs, or events where there is no decision point:

```go
log.Info(ctx, "doc.storage.saved", "Document written to storage", log.F{
    "doc_id":  docID,
    "bytes":   n,
    "backend": "postgres",
})
```

### 2. Decision point (outcome required)

For operations where success/failure matters for triage. `outcome` and `duration_ms` are both required:

```go
log.Info(ctx, "auth.jwt.validated", "JWT validation succeeded", log.F{
    "outcome":     log.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})
```

### 3. Error (typed error required)

For errors. `log.Err{}` is required — a string `"error"` field blocks the merge:

```go
log.Error(ctx, "provider.send.rejected", "Provider rejected document send", log.F{
    "doc_id":      docID,
    "outcome":     log.Failure,
    "duration_ms": time.Since(start).Milliseconds(),
    "error": &log.Err{
        Kind:      "RateLimitExceeded",
        Code:      "PROVIDER_QUOTA_EXCEEDED",
        Message:   err.Error(),
        Retryable: true,
    },
})
```

---

## Auto-injected fields

These fields are always present in every log entry. You do not supply them:

| Field          | Source |
|---------------|--------|
| `trace_id`    | HTTP/Kafka middleware (or `sys_<uuid>` if missing) |
| `span_id`     | Generated at service entry by HTTP middleware |
| `parent_span` | Propagated from upstream via `X-Parent-Span` header |
| `seq`         | Service-local atomic counter, increments per Emit call |
| `service`     | `SERVICE_NAME` env var |
| `version`     | Passed to `ConfigFromEnv` |
| `host`        | `os.Hostname()` |
| `env`         | `ENV` env var |
| `ts`          | `time.Now().UTC()` at emit time |

---

## Middleware

### HTTP

```go
import "github.com/darshanredkar11/observalog-go/middleware"

http.ListenAndServe(":8080", middleware.Middleware(yourRouter))
```

Wraps every request with:
- A new `trace_id` (or propagated `X-Trace-ID`)
- A new `span_id`
- `journey_stage` derived from the HTTP path
- A fresh per-request `seq` counter starting at 1

### Kafka consumer

```go
ctx := middleware.KafkaMiddleware(context.Background(), msg)
```

Extracts trace context from Kafka message headers (`X-Trace-ID`, `X-Span-ID`, etc.) and injects it into the context.

### Kafka producer

```go
msg := middleware.NewTracedMessage(ctx, topic, key, value)
```

Creates a Kafka message with trace headers pre-populated from the context. Downstream consumers can call `KafkaMiddleware` to continue the trace.

---

## Graceful degradation

If context is missing (background jobs, crons, unregistered routes), the library never panics:

| Missing field    | Substituted value               |
|-----------------|---------------------------------|
| `trace_id`      | `sys_<10 hex chars>` (14 chars) |
| `user_id`       | `"system"`                      |
| `journey_stage` | `"system.background.untraced"`  |

The `sys_` prefix signals the brain instantly — no special handling is needed in the emitting code.

---

## Buffer overflow

The drain goroutine writes to stdout asynchronously via a buffered channel (default: 10,000 entries). When the buffer is full, entries are dropped non-blocking.

```go
dropped := log.DroppedLogCount() // uint64, cumulative
```

Expose this as a metric and alert if it grows. If drops are frequent, increase `LOG_BUFFER_SIZE`.

---

## Wire format emitted

Each `Emit` call writes two lines to stdout:

```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
{"e":"provider.send.rejected","m":"Provider rejected document send","er":{"ek":"RateLimitExceeded","ec":"PROVIDER_QUOTA_EXCEEDED","rt":true},"c":{"di":"doc123"},"o":"failure","ms":87}
```

See [docs/wire-format.md](../docs/wire-format.md) for the complete byte-level specification.

---

## Testing

```bash
go test ./...
```

Tests cover:
- Three emit shapes (informational, decision point, error)
- Ambient context injection (no explicit fields needed for auto-injected values)
- Wire format byte positions (Part A offsets, Part B abbreviations)
- Fingerprint collision prevention (`ctx_primary_key` disambiguation)
- Graceful degradation (sys_ prefix, system defaults)
- HTTP middleware trace propagation and journey stage derivation

---

## Files

| File                    | Responsibility |
|------------------------|----------------|
| `fields.go`            | `F`, `Err`, `Outcome`, `Level` types |
| `context.go`           | Context key types and With/FromCtx functions |
| `dict.go`              | Abbreviation dictionary (DictVersion 1) |
| `wire_contract.go`     | Byte offset constants (must match brain's `wire_contract.rs`) |
| `wire.go`              | `EncodePartA`, `EncodePartB` |
| `fingerprint.go`       | xxHash64 fingerprint computation |
| `emit.go`              | `emitLine` and the four public Emit functions |
| `async.go`             | Non-blocking channel + drain goroutine |
| `degrade.go`           | Graceful degradation for missing context |
| `config.go`            | `Config`, `ConfigFromEnv`, `Init`, `Shutdown` |
| `middleware/http.go`   | HTTP trace injection middleware |
| `middleware/kafka_consumer.go` | Kafka consumer context extraction |
| `middleware/kafka_producer.go` | Kafka producer header injection |
