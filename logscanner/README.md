# logscanner

**CI static analyser for ObservaLog log contracts.**

logscanner parses Go source files and enforces structural rules on every `log.Info`, `log.Warn`, `log.Error`, and `log.Debug` call. A PR that violates a rule exits with code 1, blocking the merge.

---

## Install

```bash
cargo build --release
cp target/release/logscanner /usr/local/bin/
```

---

## Usage

### Scan changed files (CI recommended)

```bash
logscanner --files $(git diff --name-only origin/main...HEAD | grep '\.go$')
```

### Scan a directory

```bash
logscanner --dir ./services/auth-service
```

### JSON output (for tooling)

```bash
logscanner --dir ./services --format json
```

---

## Exit codes

| Code | Meaning |
|------|---------|
| `0`  | No findings — merge allowed |
| `1`  | ERROR findings — merge blocked |
| `2`  | WARN findings only — merge allowed, notify |

---

## Rules

### UNDECLARED_EVENT (ERROR)

Every log call's `event` argument must follow the `domain.object.action` grammar.

- Exactly three segments separated by `.`
- Domain must be one of: `auth`, `doc`, `provider`, `infra`
- Action must be one of the 15 declared verbs: `received`, `validated`, `rejected`, `published`, `failed`, `exhausted`, `expired`, `attempted`, `succeeded`, `created`, `updated`, `deleted`, `queried`, `connected`, `disconnected`
- All lowercase

```go
// Blocked: domain "payments" not declared
log.Info(ctx, "payments.invoice.created", ...)

// Blocked: action "processed" not in 15-verb set
log.Error(ctx, "doc.document.processed", ...)

// Allowed
log.Error(ctx, "provider.send.rejected", ...)
```

### UNSTRUCTURED_ERROR (ERROR)

`log.Error()` calls and any log with `outcome=failure` must use `log.Err{}` struct — never a plain string for the error field.

```go
// Blocked: string error field
log.Error(ctx, "provider.send.failed", "Send failed", log.F{
    "error": err.Error(),  // <-- blocked
})

// Allowed: typed Err struct
log.Error(ctx, "provider.send.failed", "Send failed", log.F{
    "error": &log.Err{
        Kind:      "NetworkError",
        Code:      "SEND_FAILED",
        Retryable: true,
    },
})
```

### MISSING_DURATION (ERROR)

Any log call with an `outcome` field must also include `duration_ms`. The brain uses this field for timeout detection; a missing duration produces an incomplete triage.

```go
// Blocked: outcome without duration_ms
log.Info(ctx, "doc.storage.saved", "Document saved", log.F{
    "outcome": log.Success,  // missing duration_ms
})

// Allowed
log.Info(ctx, "doc.storage.saved", "Document saved", log.F{
    "outcome":     log.Success,
    "duration_ms": time.Since(start).Milliseconds(),
})
```

### RAW_PII_IN_LOG (WARN)

Field names matching known PII patterns are flagged. These fields should be excluded or hashed before logging.

Detected patterns: `email`, `phone`, `ssn`, `credit_card`, `password`, `secret`, `token` (as standalone field names).

```go
// Flagged (WARN)
log.Info(ctx, "auth.session.created", "Session created", log.F{
    "email": userEmail,  // PII
})
```

### UNDECLARED_ABBREVIATION (WARN)

Field names that are not in the abbreviation dictionary and are not nested under `ctx` are flagged. They will be emitted unabbreviated, increasing wire size.

---

## CI integration

### GitHub Actions

```yaml
name: Log contracts

on:
  pull_request:
    paths:
      - '**/*.go'

jobs:
  logscanner:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install logscanner
        run: |
          cd logscanner
          cargo build --release
          cp target/release/logscanner /usr/local/bin/

      - name: Scan changed Go files
        run: |
          CHANGED=$(git diff --name-only origin/${{ github.base_ref }}...HEAD | grep '\.go$' || true)
          if [ -n "$CHANGED" ]; then
            logscanner --files $CHANGED
          fi
```

### Full directory scan

```yaml
      - name: Scan all Go files
        run: logscanner --dir ./services
```

---

## Output format

### Text (default)

```
ERROR [UNSTRUCTURED_ERROR] services/doc-service/handler.go:42
  log.Error call uses string "error" field — use log.Err{} struct

WARN [MISSING_DURATION] services/auth-service/middleware.go:89
  outcome field present but duration_ms absent
```

### JSON

```json
[
  {
    "severity": "ERROR",
    "rule": "UNSTRUCTURED_ERROR",
    "file": "services/doc-service/handler.go",
    "line": 42,
    "message": "log.Error call uses string \"error\" field — use log.Err{} struct"
  }
]
```

---

## Architecture

```
src/
├── main.rs              # CLI entry point (clap)
├── scanner.rs           # scan_file, scan_directory, scan_files, exit_code
├── finding.rs           # Finding, Severity, Location types
├── grammar.rs           # Event grammar validation (domains, actions, regex)
└── go/
    ├── walker.rs        # Go file parser, function body extractor
    ├── classify.rs      # Function class: Boundary / External / Internal
    └── rules/
        └── mod.rs       # All rule implementations
```

The scanner uses regex-based Go parsing — no full AST. It identifies `log.Info(...)`, `log.Error(...)`, etc. calls and inspects their arguments. Two rules (`MISSING_EXIT_LOG`, `MISSING_OUTCOME`) require full AST branch analysis and are currently stubs pending tree-sitter or `go/ast` integration.

---

## Build

```bash
cargo build --release   # production binary (~2MB)
cargo test              # run all rule tests
```
