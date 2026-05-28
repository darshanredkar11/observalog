# ObservaLog

**A structured log contract system for distributed microservices — in Go, Java, and Node.js.**

ObservaLog enforces *what* every log must contain, *how* it travels on the wire, and *what* your AI triage engine does when something fails — all three as first-class, versioned contracts across every service language.

→ [Live site](https://darshanredkar11.github.io/observalog) · [Whitepaper](docs/whitepaper.md) · [Wire format](docs/wire-format.md)

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
1|trc_7f2a1b9e4d01|spn_004|spn_001|04|2|3|2|1748268153812
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
| [`logscanner`](logscanner/) | Rust | CI static analyser. Enforces log contracts at merge time for Go, Java, and Node.js. |
| [`observalog-brain`](observalog-brain/) | Rust | AI triage engine. Kafka → TimescaleDB → LLM → WebSocket. |

---

## Quick start

### 0. Start the infrastructure

```bash
cp .env.example .env          # add your ANTHROPIC_API_KEY
docker compose up -d
```

Starts: TimescaleDB · Kafka (KRaft, no Zookeeper) · Valkey · Fluent Bit · observalog-brain.

WebSocket API available at `ws://localhost:4000/ws` once the brain is healthy.

---

## Library usage

### Go

**Install**

```bash
go get github.com/darshanredkar11/observalog/observalog-go@latest
```

No registry setup required — Go fetches directly from GitHub.

**Setup**

```go
import (
    log "github.com/darshanredkar11/observalog/observalog-go"
    "github.com/darshanredkar11/observalog/observalog-go/middleware"
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
log.Info(ctx, "doc.document.created", "Document written to storage", log.F{
    "doc_id": docID,
    "bytes":  n,
})

// Decision point — outcome + duration required (scanner enforces this)
log.Info(ctx, "auth.permission.checked", "Permission granted", log.F{
    "doc_id":      docID,
    "outcome":     log.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})

// Error — structured Err, never a plain string (scanner blocks plain strings)
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
handler := middleware.KafkaConsumerMiddleware(func(ctx context.Context, msg *kafka.Message) error {
    log.Info(ctx, "doc.event.received", "Processing document event", log.F{"doc_id": docID})
    return processMessage(ctx, msg)
})
```

---

### Java

**Install (Maven)**

Add the GitHub Packages repository to your `pom.xml`:

```xml
<repositories>
    <repository>
        <id>github</id>
        <url>https://maven.pkg.github.com/darshanredkar11/observalog</url>
    </repository>
</repositories>

<dependency>
    <groupId>com.observalog</groupId>
    <artifactId>observalog-java</artifactId>
    <version>1.0.0</version>
</dependency>
```

Add your GitHub credentials to `~/.m2/settings.xml`:

```xml
<servers>
    <server>
        <id>github</id>
        <username>YOUR_GITHUB_USERNAME</username>
        <password>YOUR_GITHUB_TOKEN</password>  <!-- needs read:packages scope -->
    </server>
</servers>
```

**Install (Gradle)**

```kotlin
repositories {
    maven {
        url = uri("https://maven.pkg.github.com/darshanredkar11/observalog")
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
ObservaLog.info("doc.document.created", "Document written to storage", Map.of(
    "doc_id", docId,
    "bytes", bytes
));

// Decision point
ObservaLog.info("auth.permission.checked", "Permission granted", Map.of(
    "doc_id",      docId,
    "outcome",     Outcome.SUCCESS,
    "duration_ms", duration
));

// Error — typed Err, not a string (scanner blocks: Map.of("error", "some string"))
ObservaLog.error("provider.send.rejected", "Provider rejected document send", Map.of(
    "doc_id",      docId,
    "outcome",     Outcome.FAILURE,
    "duration_ms", duration,
    "error",       new Err("RateLimitExceeded", "PROVIDER_QUOTA_EXCEEDED", ex.getMessage(), true)
));
```

**Kafka consumer**

```java
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
# Tell npm to use GitHub Packages for the @darshanredkar11 scope
echo "@darshanredkar11:registry=https://npm.pkg.github.com" >> .npmrc

npm install @darshanredkar11/observalog-node
```

Set `NODE_AUTH_TOKEN` to a GitHub token with `read:packages` scope (in CI: `secrets.GITHUB_TOKEN`).

**Setup — Express**

```typescript
import { init, shutdown } from '@darshanredkar11/observalog-node';
import { httpMiddleware } from '@darshanredkar11/observalog-node/middleware';

init({ serviceCode: 2, version: process.env.SERVICE_VERSION! });
app.use(httpMiddleware());   // injects traceId, spanId, journeyStage per request
process.on('SIGTERM', shutdown);
```

**Emit logs**

```typescript
import { info, warn, error } from '@darshanredkar11/observalog-node';
import type { Err } from '@darshanredkar11/observalog-node';

// Info
info('doc.document.created', 'Document written to storage', {
    doc_id: docId,
    bytes:  n,
});

// Decision point
info('auth.permission.checked', 'Permission granted', {
    doc_id:      docId,
    outcome:     'success',
    duration_ms: Date.now() - start,
});

// Error — typed Err object, not a string
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

**Kafka consumer**

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

The scanner enforces five rules across Go, Java, and Node.js/TypeScript:

| Rule | What it catches |
|------|-----------------|
| `UNDECLARED_EVENT` | event string doesn't match `domain.object.action` grammar |
| `UNSTRUCTURED_ERROR` | `error` field is a plain string, not a typed `Err` struct |
| `MISSING_DURATION` | `outcome` present without `duration_ms` |
| `RAW_PII_IN_LOG` | `email`, `phone`, `password`, `token`, `ssn` in log context |
| `UNDECLARED_ABBREVIATION` | context key not in the 18-entry wire dictionary |

```yaml
- name: Install logscanner
  run: cargo install --git https://github.com/darshanredkar11/observalog logscanner --locked

- name: Scan changed files
  run: |
    FILES=$(git diff --name-only origin/main...HEAD | grep -E '\.(go|java|ts|js)$')
    [ -n "$FILES" ] && logscanner --format text --files $FILES
```

Exit code 1 = errors (block merge). Exit code 2 = warnings only.

---

## Publish new releases

Tag prefixes trigger the publish workflows:

```bash
git tag java/v1.0.1 && git push origin java/v1.0.1   # → publishes to GitHub Packages Maven
git tag node/v1.0.1 && git push origin node/v1.0.1   # → publishes to GitHub Packages npm
git tag go/v1.0.1   && git push origin go/v1.0.1     # → creates GitHub Release (Go proxy picks up automatically)
```

---

## Log contract

| Field | Required | Source |
|-------|----------|--------|
| `event` | always | Developer — `domain.object.action` grammar |
| `message` | always | Developer — human-readable description |
| `outcome` | decision points | Developer — `success/failure/partial/pending` |
| `duration_ms` | when outcome present | Developer — wall clock of the operation |
| `error` | level=Error or outcome=failure | Developer — typed `Err` struct, never a string |
| `trace_id` | auto | Library — from context or `sys_<uuid>` |
| `span_id` | auto | Library — generated at service entry |
| `seq` | auto | Library — request-scoped atomic counter |
| `ts_ms` | auto | Library — UTC milliseconds at emit time |

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
| Two-table I/O reduction | ~78% vs single-table JSONB |

---

## Build

```bash
# Go library
cd observalog-go && go build ./... && go test ./...

# Java library
cd observalog-java && mvn verify

# Node.js library
cd observalog-node && npm ci && npm test

# Rust scanner (all three languages)
cd logscanner && cargo build --release && cargo test

# Rust brain
cd observalog-brain && cargo build --release && cargo test
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [docs/whitepaper.md](docs/whitepaper.md) | Architecture decisions, multi-language design, gap analysis |
| [docs/wire-format.md](docs/wire-format.md) | Part A byte positions, Part B abbreviation dictionary |
| [docs/event-grammar.md](docs/event-grammar.md) | Event naming contract and scanner rules |
| [docs/deployment-guide.md](docs/deployment-guide.md) | Infrastructure setup and configuration |

---

## License

MIT
