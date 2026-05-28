# ObservaLog Event Grammar

**Version 1 — enforced by logscanner at CI time**

---

## Grammar

Every event name must follow the three-segment pattern:

```
domain.object.action
```

All lowercase. Two literal dots. No spaces.

---

## Domains

| Domain     | Services         | Examples |
|------------|-----------------|---------|
| `auth`     | auth-service     | `auth.jwt.validated`, `auth.session.expired` |
| `doc`      | doc-service      | `doc.document.created`, `doc.storage.saved` |
| `provider` | provider-service | `provider.send.rejected`, `provider.api.failed` |
| `infra`    | infra layer      | `infra.kafka.connected`, `infra.db.exhausted` |

---

## Actions (15 verbs)

| Action          | Meaning |
|-----------------|---------|
| `received`      | Inbound message or request accepted |
| `validated`     | Input or token checked successfully |
| `rejected`      | Input or request denied by validation |
| `published`     | Message sent to a downstream queue or topic |
| `failed`        | Operation failed (generic catch-all) |
| `exhausted`     | Resource limit reached (quota, pool, rate) |
| `expired`       | TTL or timeout reached |
| `attempted`     | Operation started; outcome unknown yet |
| `succeeded`     | Operation completed successfully |
| `created`       | New entity written |
| `updated`       | Existing entity modified |
| `deleted`       | Entity removed |
| `queried`       | Read operation executed |
| `connected`     | Connection established |
| `disconnected`  | Connection closed |

---

## Naming rules

1. **Exactly three segments.** `auth.jwt.validated` — valid. `auth.jwt` — invalid. `auth.jwt.token.validated` — invalid.
2. **All lowercase.** `Auth.JWT.Validated` — invalid.
3. **Domain must be one of the four declared domains.** Expanding the domain list requires a PR to the grammar and logscanner.
4. **Action must be one of the 15 declared verbs.** Adding a verb requires a PR to the grammar and logscanner.
5. **No constants file.** New events do not require a PR to a shared registry. Grammar adherence is the contract, not registration.

---

## Valid examples

```
auth.jwt.validated           # JWT checked successfully
auth.jwt.expired             # JWT TTL exceeded
auth.session.created         # New session written
auth.permission.checked      # (uses "validated" in practice — use declared verbs)
doc.document.created         # New document written
doc.document.updated         # Existing document modified
doc.storage.saved            # Bytes written to storage backend
doc.export.failed            # Export operation failed
provider.send.attempted      # Outbound send started
provider.send.rejected       # Provider returned 4xx
provider.api.exhausted       # Rate limit hit
provider.connection.connected # Connection to provider established
infra.db.connected           # Database connection established
infra.kafka.published        # Message sent to Kafka topic
infra.cache.exhausted        # Redis/Valkey pool full
```

---

## Invalid examples and error messages

| Invalid event                    | Scanner error |
|----------------------------------|---------------|
| `auth.jwt`                       | `UNDECLARED_EVENT: event has 2 segments, expected exactly 3` |
| `auth.jwt.token.validated`       | `UNDECLARED_EVENT: event has 4 segments, expected exactly 3` |
| `payments.invoice.created`       | `UNDECLARED_EVENT: domain "payments" not in declared set` |
| `doc.document.processed`         | `UNDECLARED_EVENT: action "processed" not in declared 15-verb set` |
| `Doc.Document.Created`           | `UNDECLARED_EVENT: event must be lowercase` |

---

## Scanner enforcement

The `logscanner` static analyser runs in CI and validates every `log.Info(...)`, `log.Warn(...)`, `log.Error(...)`, and `log.Debug(...)` call for event grammar compliance.

A PR that introduces a grammatically invalid event name receives exit code 1, blocking the merge.

### CI integration

```yaml
- name: Scan logs
  run: logscanner --files $(git diff --name-only origin/main...HEAD | grep '\.go$')
```

### All rules enforced by logscanner

| Rule ID                  | What it checks |
|--------------------------|----------------|
| `UNDECLARED_EVENT`       | Event name violates `domain.object.action` grammar |
| `UNSTRUCTURED_ERROR`     | `log.Error()` or `outcome=failure` uses a string `"error"` field instead of `log.Err{}` |
| `MISSING_DURATION`       | `outcome` field present but `duration_ms` absent |
| `RAW_PII_IN_LOG`         | Field name matches known PII patterns (`email`, `phone`, `ssn`, `credit_card`, `password`) |
| `UNDECLARED_ABBREVIATION`| Field name in Part B is not in the abbreviation dictionary |
| `MISSING_EXIT_LOG`       | Function boundary closes without a terminal log call (stub — requires AST integration) |
| `MISSING_OUTCOME`        | Decision-point function has no `outcome` field (stub — requires AST integration) |

---

## Relationship to journey_stage

`journey_stage` is a separate three-segment field (`http.segment.segment`) auto-derived by middleware from the HTTP path. It is not the same as `event`. Both appear in log entries, but:

- `event` describes **what happened** in the code
- `journey_stage` describes **where in the user journey** this request originated

`journey_stage` is set once at the HTTP or Kafka boundary and is immutable for the lifetime of that trace within that service (Decision 2). It does not follow the event grammar — it uses the request path as its structure.
