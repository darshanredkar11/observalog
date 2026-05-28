# ObservaLog — Scanner Rules

These are the rules logscanner enforces. One file per rule in `src/go/rules/`.
Hard block = exit code 1, merge blocked. Advisory = exit code 2, comment posted.

---

## Hard blocks (severity: error)

### MISSING_EXIT_LOG
Every return path in a function must have a preceding log call in its scope chain.
- Applies to: all function classes
- Detection: AST walk, check every return_statement, panic(), os.Exit()
- Boundary and external functions: ALSO require an entry log

### MISSING_OUTCOME
A decision point (if/else/switch with different return paths) must have outcome field.
- Applies to: all function classes
- Detection: AST branch analysis, identify if-else with different return paths

### UNSTRUCTURED_ERROR
Any log call with level=Error must contain error field as log.Err struct.
Any log call with outcome=failure must contain error field.
- Detection: check field map for "error" key, verify value is log.Err not string

### MISSING_DURATION
Any log call carrying outcome field must also carry duration_ms field.
- Detection: field co-occurrence check

### UNDECLARED_EVENT
Event string must satisfy grammar: three segments, known domain, known action verb.
- Detection: parse event string, check against DOMAINS and ACTIONS in grammar.rs
- Note: validate structure, not against a lookup table of specific events

---

## Advisory (severity: warn)

### RAW_PII_IN_LOG
Field names email, phone, password, token, ssn passed directly into ctx.
- Detection: check ctx map keys against PII_FIELD_NAMES list
- Advisory only — warn but do not block

### UNDECLARED_ABBREVIATION
Ctx key used that is not registered in dict.go abbreviation dictionary.
- Detection: check ctx keys against known abbreviations
- Advisory — new keys are expected during development

### AUTO_DERIVED_JOURNEY_STAGE
Handler function whose journey_stage resolves to auto-derived format.
- Detection: static analysis on HTTP handler functions
- Advisory — prompt developer to add explicit route_stages.yaml mapping

---

## Function classification (used to determine which rules apply)

### boundary
- HTTP handler: takes (http.ResponseWriter, *http.Request) parameters
- Kafka consumer: takes (context.Context, kafka.Message) parameters
- Rules: entry log AND exit log both mandatory

### external
- Contains http.Do(), http.Get(), http.Post() calls
- Contains sql.Query(), sql.Exec(), db.Query() calls
- Contains kafka produce calls
- Rules: entry log AND exit log mandatory. Exit must have duration_ms.

### internal
- Everything else
- Rules: exit log mandatory on every return path. Entry log not required.

---

## Scanner output format

```json
{
  "rule": "MISSING_EXIT_LOG",
  "severity": "error",
  "file": "internal/doc/handler.go",
  "function": "SendDocument",
  "line": 84,
  "col": 5,
  "message": "Function has 3 return paths. None emit an exit log with outcome.",
  "uncovered_returns": [
    { "line": 84, "col": 5, "kind": "ExplicitReturn", "context": "return nil, err" },
    { "line": 91, "col": 5, "kind": "ExplicitReturn", "context": "return result, nil" }
  ],
  "suggested_event": "doc.document.published"
}
```

## CLI interface

```bash
# Scan specific files (CI mode — diff-aware)
logscanner --files internal/doc/handler.go internal/auth/middleware.go

# Scan directory
logscanner --dir ./services/auth-service/internal

# Output format
logscanner --format json --files changed_files...

# Exit codes
# 0 = no findings
# 1 = severity:error findings (block merge)
# 2 = severity:warn only (allow merge)
```
