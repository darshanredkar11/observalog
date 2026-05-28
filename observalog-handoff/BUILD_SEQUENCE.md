# ObservaLog — Build Sequence

Build in this exact order. Each file depends on the ones above it.
Do not skip ahead. Test after each file compiles.

---

## Phase 1 — observalog-go

### Step 1: Project scaffold
```bash
mkdir observalog-go && cd observalog-go
go mod init github.com/yourorg/observalog-go
mkdir -p middleware internal
```

### Step 2: Terminal files (no internal imports)
Build these first — everything else depends on them.

1. `fields.go`
   - log.F map type
   - log.Err struct { Kind, Code, Message string; Retryable bool }
   - outcomes constants: Success, Failure, Partial, Pending
   - level constants: LevelDebug, LevelInfo, LevelWarn, LevelError
   - Exports: F, Err, Level, Outcome types and constants

2. `context.go`
   - Unexported ctx key types (struct{} — never strings)
   - WithTraceID(), WithSpanID(), WithParentSpan(), WithUserID()
   - WithTenantID(), WithJourneyStage(), WithSeq()
   - TraceIDFromCtx(), SpanIDFromCtx(), SeqFromCtx() etc.
   - Exports: With* and *FromCtx functions only

3. `dict.go`
   - KeyToAbbrev map — every ctx key → short abbreviation
   - AbbrevToKey map — reverse lookup for brain decoder
   - ValidateNoDictCollisions() — called at Init(), panics on collision
   - DictVersion constant
   - Exports: KeyToAbbrev, AbbrevToKey, DictVersion

### Step 3: Wire contract (read-only reference)
4. `wire_contract.go`
   - const PartASchemaVersion = 1
   - const PartAByteLen = 55
   - Byte offset constants: TraceIDOffset, SpanIDOffset, ParentSpanOffset,
     SeqOffset, ServiceCodeOffset, LevelCodeOffset, OutcomeCodeOffset, TsMsOffset
   - Service code constants: ServiceAuth=1, ServiceDoc=2, ServiceProvider=3, ServiceSystem=0
   - Level code constants: LevelCodeDebug=0, LevelCodeInfo=1, LevelCodeWarn=2, LevelCodeError=3
   - Outcome code constants: OutcomeCodeNone=0, OutcomeCodeSuccess=1, OutcomeCodeFailure=2, OutcomeCodePartial=3
   - NO functions. Constants only.
   - Exports: all constants

### Step 4: Core encoding files
5. `wire.go`
   - EncodePartA(entry LogEntry) []byte — writes fixed-position header
   - EncodePartB(entry LogEntry) []byte — writes abbreviated JSON using dict.go
   - Imports: wire_contract.go (constants), dict.go (abbreviations)
   - Exports: EncodePartA, EncodePartB

6. `fingerprint.go`
   - ComputeFingerprint(serviceCode uint8, event, errorCode, ctxPrimaryKey string) int64
   - Uses xxhash.Sum64String()
   - Pure function. Zero side effects.
   - Imports: github.com/cespare/xxhash/v2
   - Exports: ComputeFingerprint

### Step 5: Infrastructure
7. `async.go`
   - var logCh chan []byte
   - StartDrainGoroutine(w io.Writer, bufSize int)
   - SendToChannel(line []byte) — non-blocking, increments drop counter on full
   - FlushAndClose() — blocks until channel drained
   - var dropCount atomic.Uint64 — exported as DroppedLogCount()
   - Exports: StartDrainGoroutine, SendToChannel, FlushAndClose, DroppedLogCount

8. `degrade.go`
   - ApplyGracefulDegradation(ctx context.Context) context.Context
   - If trace_id missing: inject sys_<uuidv4>
   - If user_id missing: inject "system"
   - If journey_stage missing: inject "system.background.untraced"
   - Exports: ApplyGracefulDegradation

### Step 6: Emit contract (read-only reference)
9. `emit_contract.go`
   - Compile-time assertions using blank identifier tricks
   - Assert: seq is accessed as *atomic.Uint32 (not value copy)
   - Assert: PartAByteLen matches actual encoded length
   - Documents: fingerprint must include ctxPrimaryKey — comment only
   - NO functions. Assertions and comments only.

### Step 7: Hot path
10. `emit.go`
    - Info(ctx, event, message string, fields F)
    - Warn(ctx, event, message string, fields F)
    - Error(ctx, event, message string, fields F)
    - Debug(ctx, event, message string, fields F)
    - internal emitLine(ctx, level, event, message string, fields F)
    - Hot path sequence:
      1. Level check — return if below threshold
      2. ApplyGracefulDegradation(ctx)
      3. Extract ambient fields from ctx
      4. Increment seq: atomic.AddUint32
      5. Compute fingerprint if error field present
      6. EncodePartA → EncodePartB
      7. SendToChannel (non-blocking)
    - Imports: context.go, wire.go, fingerprint.go, async.go, degrade.go, fields.go
    - Exports: Info, Warn, Error, Debug

### Step 8: Config and lifecycle
11. `config.go`
    - Config struct: ServiceName, Version, Env, Level, BufferSize string/int
    - ConfigFromEnv(version string) Config — reads env vars
    - Init(cfg Config) error — validates, starts drain goroutine, sets globals
    - Shutdown() — calls FlushAndClose(), blocks until buffer empty
    - Exports: Config, ConfigFromEnv, Init, Shutdown

### Step 9: Middleware
12. `middleware/http.go`
    - Middleware(next http.Handler) http.Handler
    - deriveJourneyStage(r *http.Request) string — route config lookup + auto-derive
    - LoadRouteStages(path string) error — loads route_stages.yaml
    - Exports: Middleware, LoadRouteStages

13. `middleware/kafka_consumer.go`
    - KafkaMiddleware(handler func(ctx, msg) error) func(ctx, msg) error
    - extractFromHeaders(msg) — reads X-Trace-Id, X-Span-Id, X-User-Id, X-Journey-Stage
    - Exports: KafkaMiddleware

14. `middleware/kafka_producer.go`
    - NewTracedMessage(ctx, topic string, value []byte) KafkaMessage
    - injectHeaders(ctx, msg) — writes X-Trace-Id, X-Span-Id, X-User-Id,
      X-Service, X-Service-Seq, X-Schema-Version, X-Journey-Stage
    - Exports: NewTracedMessage

### Step 10: Tests
15. `emit_test.go` — test all three shapes, verify ambient field injection
16. `wire_test.go` — test Part A byte positions match wire_contract.go constants
17. `fingerprint_test.go` — test collision prevention with same event/error but different ctx key
18. `degrade_test.go` — test sys_ prefix on missing context
19. `middleware/http_test.go` — test trace_id propagation and journey_stage derivation

---

## Phase 2 — logscanner (after observalog-go is complete and tested)

Sequence: finding.rs → grammar.rs → go/classify.rs → go/rules/* → go/walker.rs → scanner.rs → main.rs

---

## Phase 3 — observalog-brain (after logscanner is complete)

Sequence: db/schema.rs → db/queries.rs → ingest/wire_contract.rs → ingest/parser.rs →
          ingest/writer_contract.rs → ingest/writer.rs → ingest/kafka.rs →
          triage/repair.rs → triage/chain.rs → triage/gap.rs → triage/classify.rs →
          triage/dedup.rs → triage/environment.rs → triage/context.rs →
          triage/llm.rs → ws.rs → main.rs

---

## Environment variables required for observalog-go

| Variable | Required | Example | Notes |
|----------|----------|---------|-------|
| SERVICE_NAME | yes | auth-service | Must match registered service name |
| ENV | yes | production | production / staging / dev only |
| LOG_LEVEL | yes | info | debug / info / warn / error |
| LOG_BUFFER_SIZE | no | 10000 | Default: 10000 |
| HOST | no | pod-auth-9f3d | Falls back to os.Hostname() |

VERSION is injected at build time via ldflags:
```bash
go build -ldflags "-X main.version=$(git describe --tags --always)-$(git rev-parse --short HEAD)"
```

## Go module dependencies (observalog-go go.mod)

```
require (
    github.com/cespare/xxhash/v2 v2.3.0
    github.com/google/uuid v1.6.0
    go.uber.org/atomic v1.11.0
)
```

zerolog is used as the JSON serialisation engine under the hood.
```
require (
    github.com/rs/zerolog v1.33.0
)
```
