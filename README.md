# ObservaLog

**A structured log contract system for distributed microservices — in Go, Java, and Node.js.**

ObservaLog enforces *what* every log must contain, *how* it travels on the wire, and *what* your AI triage engine does when something fails — all three as first-class, versioned contracts across every service language.

---

## The problem

Distributed systems fail silently. A JWT expires in `auth-service` (Go), cascades through `doc-service` (Java), and triggers a provider retry loop in `notification-service` (Node.js). By the time an engineer investigates, they are staring at 40,000 lines of unstructured text across three Kibana tabs in three different formats, manually correlating timestamps.

The root cause is not a missing alert. It is missing *structure*. Logs that were never designed to be read by a machine cannot be triaged by one — regardless of the language that emitted them.

ObservaLog fixes this at the source, in all three languages.

---

## How it works

```
Go / Java / Node.js service       Fluent Bit         Kafka          observalog-brain
──────────────────────────────    ──────────────     ──────         ────────────────────────────
log.Info(ctx, "event", ...)    →  GZIP compress  →  topic    →     parse Part A (fixed offsets)
log.Error(ctx, "event", ...)   →  forward        →           →     parse Part B (abbreviated JSON)
                                                                          ↓
                                                              TimescaleDB
                                                              log_index  (hot — 60 bytes/row)
                                                              log_payload (cold — full JSONB)
                                                                          ↓
                                                          fingerprint dedup (xxHash64)
                                                          gap detection (seq counter)
                                                          failure classification
                                                          LLM triage → RepairId
                                                          WebSocket → dashboard
```

Every log call, in every language, emits exactly two lines:

```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
{"e":"provider.send.rejected","m":"Provider rejected document send","er":{"ek":"RateLimitExceeded","ec":"PROVIDER_QUOTA_EXCEEDED","rt":true},"c":{"di":"doc123","hs":429},"o":"failure","ms":87}
```

**Part A** — 55-byte fixed-position header. The Rust brain reads `trace_id`, `span_id`, `service`, `level`, `outcome`, and `ts_ms` at hard-coded byte offsets — zero JSON parsing for the fields it needs 95% of the time.

**Part B** — abbreviated JSON. `event` → `e`, `duration_ms` → `ms`, `error.kind` → `ek`. After GZIP the size difference vs. binary formats is under 10 bytes per entry.

---

## Components

| Component | Language | Role |
|-----------|----------|------|
| [`observalog-go`](observalog-go/) | Go | Library for Go services. Emits two-line NDJSON to stdout. |
| [`observalog-java`](observalog-java/) | Java | Library for Java/Spring services. ThreadLocal context, Servlet filter. |
| [`observalog-node`](observalog-node/) | Node.js / TypeScript | Library for Node.js services. AsyncLocalStorage context, Express middleware. |
| [`logscanner`](logscanner/) | Rust | CI static analyser. Enforces log contracts at merge time. Exit 1 = block. |
| [`observalog-brain`](observalog-brain/) | Rust | AI triage engine. Kafka → TimescaleDB → LLM → WebSocket. |

---

## Quick start

### 0. Start the infrastructure

```bash
cp .env.example .env          # add your ANTHROPIC_API_KEY
docker compose up -d
```

Starts: TimescaleDB · Kafka (KRaft, no Zookeeper) · Valkey · Fluent Bit · observalog-brain.

The brain applies its own schema at startup. Fluent Bit tails all container stdout and forwards ObservaLog lines to Kafka automatically.

WebSocket API available at `ws://localhost:4000/ws` once the brain is healthy.

---

## Library usage

### Go

**Install**

```bash
go get github.com/darshanredkar11/observalog-go@latest
```

**Setup**

```go
import (
    log "github.com/darshanredkar11/observalog-go"
    "github.com/darshanredkar11/observalog-go/middleware"
)

func main() {
    log.Init(log.ConfigFromEnv("v1.2.3-abc123def"))
    defer log.Shutdown()

    // Wrap your HTTP router — injects trace_id, span_id, journey_stage automatically
    http.ListenAndServe(":8080", middleware.Middleware(yourRouter))
}
```

**Emit logs**

```go
// Informational
log.Info(ctx, "doc.storage.saved", "Document written to storage", log.F{
    "doc_id":  docID,
    "bytes":   n,
})

// Decision point — outcome + duration required
log.Info(ctx, "auth.permission.checked", "Permission granted", log.F{
    "doc_id":      docID,
    "outcome":     log.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})

// Error — structured Err, never a plain string
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

**Kafka consumer**

```go
// Kafka consumer middleware — restores trace context from message headers
handler := middleware.KafkaConsumerMiddleware(func(ctx context.Context, msg *kafka.Message) error {
    log.Info(ctx, "doc.event.received", "Processing document event", log.F{"doc_id": docID})
    return processMessage(ctx, msg)
})
```

---

### Java

**Install (Maven)**

Add to your `pom.xml`:

```xml
<dependency>
    <groupId>com.observalog</groupId>
    <artifactId>observalog-java</artifactId>
    <version>1.0.0</version>
</dependency>
```

Published to GitHub Packages. Add the repository:

```xml
<repositories>
    <repository>
        <id>github</id>
        <url>https://maven.pkg.github.com/darshanredkar11/observalog-java</url>
    </repository>
</repositories>
```

Add credentials to `~/.m2/settings.xml`:

```xml
<servers>
    <server>
        <id>github</id>
        <username>YOUR_GITHUB_USERNAME</username>
        <password>YOUR_GITHUB_TOKEN</password>
    </server>
</servers>
```

**Install (Gradle)**

```kotlin
repositories {
    maven {
        url = uri("https://maven.pkg.github.com/darshanredkar11/observalog-java")
        credentials {
            username = project.findProperty("gpr.user") as String? ?: System.getenv("GITHUB_ACTOR")
            password = project.findProperty("gpr.key") as String? ?: System.getenv("GITHUB_TOKEN")
        }
    }
}

dependencies {
    implementation("com.observalog:observalog-java:1.0.0")
}
```

**Setup — Spring Boot**

Register the servlet filter to inject `LogContext` on every HTTP request:

```java
@Bean
public FilterRegistrationBean<HttpFilter> observalogFilter() {
    var reg = new FilterRegistrationBean<>(new HttpFilter());
    reg.addUrlPatterns("/*");
    reg.setOrder(1);
    return reg;
}
```

**Emit logs**

```java
import com.observalog.ObservaLog;
import com.observalog.Err;
import com.observalog.Outcome;

// Info
ObservaLog.info("doc.storage.saved", "Document written to storage", Map.of(
    "doc_id", docId,
    "bytes", bytes
));

// Decision point
ObservaLog.info("auth.permission.checked", "Permission granted", Map.of(
    "doc_id",      docId,
    "outcome",     Outcome.SUCCESS,
    "duration_ms", duration
));

// Error
ObservaLog.error("provider.send.rejected", "Provider rejected document send", Map.of(
    "doc_id",      docId,
    "outcome",     Outcome.FAILURE,
    "duration_ms", duration,
    "error",       new Err("RateLimitExceeded", "PROVIDER_QUOTA_EXCEEDED", ex.getMessage(), true)
));
```

**Kafka consumer (Java)**

```java
// Wrap each Kafka record before processing
KafkaConsumerMiddleware.wrap(record);
try {
    processRecord(record);
} finally {
    LogContext.clear();
}
```

---

### Node.js / TypeScript

**Install**

```bash
npm install @darshanredkar11/observalog-node
```

**Setup — Express**

```typescript
import { init, shutdown } from '@darshanredkar11/observalog-node';
import { httpMiddleware } from '@darshanredkar11/observalog-node/middleware';

init({ serviceCode: 2, version: process.env.SERVICE_VERSION! });
app.use(httpMiddleware());          // injects traceId, spanId, journeyStage per request
process.on('SIGTERM', shutdown);
```

**Emit logs**

```typescript
import { info, warn, error } from '@darshanredkar11/observalog-node';
import type { Err } from '@darshanredkar11/observalog-node';

// Info
info('doc.storage.saved', 'Document written to storage', {
    doc_id: docId,
    bytes: n,
});

// Decision point
info('auth.permission.checked', 'Permission granted', {
    doc_id:      docId,
    outcome:     'success',
    duration_ms: Date.now() - start,
});

// Error
const err: Err = {
    kind:      'RateLimitExceeded',
    code:      'PROVIDER_QUOTA_EXCEEDED',
    message:   e.message,
    retryable: true,
};
error('provider.send.rejected', 'Provider rejected document send', {
    doc_id:      docId,
    outcome:     'failure',
    duration_ms: Date.now() - start,
    error:       err,
});
```

**Kafka consumer (Node.js)**

```typescript
import { kafkaConsumerMiddleware } from '@darshanredkar11/observalog-node/middleware';

consumer.run({
    eachMessage: kafkaConsumerMiddleware(async ({ message }) => {
        info('doc.event.received', 'Processing document event', { doc_id: docId });
        await processMessage(message);
    }),
});
```

---

## Add logscanner to CI

```yaml
- name: Scan logs
  run: logscanner --files $(git diff --name-only origin/main...HEAD | grep '\.go$')
```

Exits 1 and blocks merge on any violation: missing event grammar, string errors, outcome without duration, missing trace propagation in HTTP clients.

---

## Log contract

Every log call, in every language, must contain:

| Field | Required | Source |
|-------|----------|--------|
| `event` | always | Developer — `domain.object.action` grammar |
| `message` | always | Developer — past tense, min 10 chars |
| `outcome` | decision points | Developer — `success/failure/partial/pending` |
| `duration_ms` | when outcome present | Developer — wall clock of the operation |
| `error` | level=Error or outcome=failure | Developer — typed `Err` struct, never a string |
| `trace_id` | auto | Library — from context or `sys_<uuid>` |
| `span_id` | auto | Library — generated at service entry |
| `seq` | auto | Library — request-scoped atomic counter |
| `ts` | auto | Library — UTC at emit time |

The scanner enforces these at CI time. A PR that emits `Error` with a string `"error"` field never merges.

---

## Event grammar

```
domain.object.action
```

**Domains:** `auth` · `doc` · `provider` · `infra`

**Actions (15 verbs):** `received` · `validated` · `rejected` · `published` · `failed` · `exhausted` · `expired` · `attempted` · `succeeded` · `created` · `updated` · `deleted` · `queried` · `connected` · `disconnected`

Valid: `auth.jwt.validated` · `doc.document.created` · `provider.send.rejected`

---

## Performance

| Metric | Value |
|--------|-------|
| Emit latency (hot path, Go) | ~200ns per log call |
| Fingerprint computation | ~8ns (xxHash64, all languages) |
| Wire size reduction | ~70% vs unabbreviated JSON (before GZIP) |
| TimescaleDB point lookup | 0.46ms (vs 35–85ms ClickHouse) |
| LLM call elimination | ~95% via fingerprint dedup |
| log_index row size | ~60 bytes |

---

## Deployment

See [docs/deployment-guide.md](docs/deployment-guide.md) for full infrastructure setup.

**Required services:**
- Kafka (log transport, Fluent Bit → brain)
- TimescaleDB (log storage, hypertable with 1-day chunks)
- Valkey (gap detection grace window, 30s TTL)
- Anthropic API key (LLM triage)

---

## Documentation

| Document | Description |
|----------|-------------|
| [docs/whitepaper.md](docs/whitepaper.md) | Architecture decisions, multi-language design, gap analysis |
| [docs/wire-format.md](docs/wire-format.md) | Part A byte positions, Part B abbreviation dictionary |
| [docs/event-grammar.md](docs/event-grammar.md) | Event naming contract and scanner rules |
| [docs/deployment-guide.md](docs/deployment-guide.md) | Infrastructure setup and configuration |

---

## Build

```bash
# Go library
cd observalog-go && go build ./... && go test ./...

# Java library
cd observalog-java && mvn verify

# Node.js library
cd observalog-node && npm ci && npm test

# Rust scanner
cd logscanner && cargo build --release && cargo test

# Rust brain
cd observalog-brain && cargo build --release && cargo test
```

---

## License

MIT
