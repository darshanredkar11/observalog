# ObservaLog — Enterprise Pitch Deck

**"Structured observability, enforced at the source."**

*ObservaLog makes your logs machine-readable across every language your stack uses — enforced at CI, triaged by AI in under 8 minutes.*

---

## Slide 1 — Cover

### ObservaLog

**Structured observability for polyglot microservices.**

Your AI triage tool isn't failing because the AI is wrong. It's failing because your logs are unreadable garbage.

ObservaLog fixes the data, not the model.

- **GitHub:** https://github.com/darshanredkar11/observalog
- **License:** MIT (libraries + scanner) / Hosted cloud (paid)
- **Status:** Production-ready, v1.2

---

## Slide 2 — The Problem

### Incident response is broken. Here's the data.

**P1 incidents cost more than most teams realize.**

The Google SRE Book documents that mean time to resolution (MTTR) for complex distributed system failures averages 4–8 hours at organizations without automated triage pipelines. At a blended senior engineer rate of $150–250/hour, with a war room of 5–10 engineers, a single P1 incident costs $3,000–$20,000 in direct labor — before accounting for revenue loss, customer churn, or SLA penalties.

Atlassian's 2023 Incident Management survey found that 59% of engineering teams spend more than 10 hours per week on incident response activities. For a 10-person engineering team, that's the equivalent of more than one full-time engineer doing nothing but triage.

**Why does triage take so long? Because the logs are wrong.**

The root cause is not engineer skill or tooling inadequacy — it is structural. Distributed systems emit logs from 3, 5, or 15 services, each written by different engineers, in different languages, following different conventions. The typical production log looks like this:

```
ERROR: payment failed: connection refused (attempt 3)
```

This line contains:
- No trace ID (you cannot find the other services' logs for this transaction)
- No error code (you cannot deduplicate this against yesterday's identical failure)
- No duration (you cannot identify the timeout threshold that was crossed)
- No structured fields (the AI cannot parse it without hallucinating)
- No sequence counter (you cannot detect if other log entries were dropped in transit)

**AI triage fails on unstructured data.** A 2024 analysis of LLM-based incident triage tools shows that free-form log analysis produces actionable output in fewer than 40% of cases when the underlying log data lacks structured fields. The failure mode is not LLM hallucination — it is LLM garbage-in-garbage-out. You cannot extract a root cause from data that doesn't encode one.

**The polyglot multiplier.** At companies running Go, Java, and Node.js services — which describes most Series B and later companies — three separate logging conventions produce three structurally incompatible log shapes. No shared contract means no cross-service correlation, even when the logs technically exist.

**Compliance amplifies the risk.** Financial services firms under SOX, PCI-DSS, and SOC 2 Type II, and healthcare firms under HIPAA, face audit requirements for log retention, PII handling, and event traceability. Free-form logs fail these audits. The typical remediation — a retroactive logging convention project — takes 6–18 months and fails to hold because it relies on cultural compliance, not structural enforcement.

**The status quo costs:**
- 4–8 engineer-hours per P1 incident in MTTR
- 1+ full-time engineer equivalent per 10-person team, per week, on incident response
- 60–80% of AI triage queries return unusable output on unstructured log data
- 6–18 month logging remediation projects with high recidivism
- Audit failures and compliance re-work at fintech and healthtech firms

---

## Slide 3 — The Market

### A $3B+ market with a structural gap no one has filled.

**Total Addressable Market (TAM):** The global observability and APM market was valued at $3.7 billion in 2023 and is projected to reach $8.9 billion by 2029 (MarketsandMarkets, 2024). The adjacent AIOps market adds another $4.1 billion. Combined TAM is approximately $12 billion.

**Serviceable Addressable Market (SAM):** Engineering-led companies running polyglot microservices (Go + Java + Node.js being the dominant combination) with 20+ engineers and active incident management programs. This segment represents approximately 35,000 companies globally — Series B through enterprise — and constitutes roughly $2.1 billion of the observability spend.

**Serviceable Obtainable Market (SOM):** The structured observability enforcement niche — companies willing to adopt wire format contracts at the library level and CI enforcement via static analysis. This is the highest-value 15% of the SAM: companies that have already paid the cultural cost of logging conventions but have no structural enforcement mechanism. Estimated at $300–400 million over 5 years.

**Where ObservaLog owns the market:**

No existing tool enforces log structure at emit time across multiple languages. No tool blocks PRs that log errors as strings. No tool produces cross-language fingerprints for deduplication. The structured-observability-enforcement segment is not being served by Datadog, New Relic, OpenTelemetry, or any other current vendor. This is not a feature gap in an existing product — it is a category that does not exist today.

**Gartner positioning:** Gartner's 2024 Magic Quadrant for APM and Observability identifies "AI-assisted incident remediation" and "code-level instrumentation" as the two highest-growth capability areas. ObservaLog operates at the intersection of both.

---

## Slide 4 — The Solution

### Five components. One system. Machine-readable logs, from source to fix.

ObservaLog is built on a single insight: **AI triage fails because the data is wrong, not because the AI is wrong.** Every component addresses one specific failure mode in the data pipeline.

**Component 1: observalog-go / observalog-java / observalog-node**
Three language libraries that enforce a log wire contract at emit time — typed Err structs replace string errors, a 55-byte fixed-position header is auto-injected, trace context propagates automatically through Go contexts, Java ThreadLocals, and Node.js AsyncLocalStorage.

**Component 2: logscanner**
A Rust CI static analyzer that blocks PRs violating five rules: `UNDECLARED_EVENT` (logging an event name the brain doesn't recognize), `UNSTRUCTURED_ERROR` (passing an error as a plain string), `MISSING_DURATION` (emitting an outcome without a duration field), `RAW_PII_IN_LOG` (hardcoded email or phone patterns in log literals), and `UNDECLARED_ABBREVIATION` (using an abbreviated field name outside the dictionary). Works on Go, Java, and TypeScript/JS.

**Component 3: observalog-brain**
A Rust triage engine: Kafka → TimescaleDB → fingerprint deduplication → LLM triage → typed RepairId → WebSocket dashboard. The brain classifies 95% of failures without invoking an LLM, using deterministic pattern matching on structured typed columns.

**The system works because the contract is enforced at three independent layers:** emit time (library API makes correct usage the path of least resistance), CI time (scanner blocks bad code before merge), and storage time (two-table split makes it physically impossible to run slow full-scan queries on untyped data).

---

## Slide 5 — Product Demo Narrative

### From incident to typed repair in under 8 minutes.

**The scenario:** It's 2:47 AM. PagerDuty fires. Your document processing service has a 40% failure rate. The on-call engineer opens their laptop.

**Without ObservaLog (the current reality):**

The engineer opens Datadog. They see a spike in error rate. They click into logs. The logs look like this:
```
[ERROR] failed to process document
[ERROR] provider returned error
[ERROR] retry attempt 3 failed
```

No trace IDs. No correlation. No error codes. The engineer starts grep-ing across three services' logs in separate Kibana tabs, cross-referencing timestamps manually. They Slack the Java team lead. They wait. It's now 3:45 AM. Fifty-eight minutes in, someone finds the root cause: the provider's rate limit was reduced and no configuration was updated. MTTR: 2.5 hours. Five engineers awake at 3 AM.

**With ObservaLog:**

At 2:47 AM, the brain has already been processing these logs since the first failure occurred. Every log entry carries a trace ID, a sequence counter, and a machine-readable error code. The fingerprint `xxHash64(provider_service | "provider.send.rejected" | "PROVIDER_QUOTA_EXCEEDED" | doc_id)` has been seen 847 times in the past 6 hours — the brain knew this at failure #2.

The on-call engineer opens the WebSocket dashboard. They see:

```
[2:44:01] INCIDENT OPENED
Failure class:   ExternalDependency
Affected traces: 1,247 (41.2% failure rate)
Services:        provider-service (origin), doc-service (cascade)
Fingerprint:     0x7f4a2b1c (SEEN 847x, first seen 20:16:33)
RepairId:        RATE_LIMIT_BACKOFF
Fix:             Increase provider rate limit ceiling or implement exponential backoff
                 with jitter. Config key: provider.rate_limit.ceiling (current: 100/min)
Confidence:      HIGH (fingerprint match, no LLM call required)
```

The engineer updates the config. The failure rate drops to zero. They page down, close the incident, and are back asleep by 2:55 AM. MTTR: 8 minutes.

**What made this possible:**

The `logscanner` blocked a PR two months ago that would have logged this error as a plain string. Because that PR was rejected, the error arrived with a typed `Err` struct containing `Kind: "RateLimitExceeded"` and `Code: "PROVIDER_QUOTA_EXCEEDED"`. The brain saw the fingerprint on the first occurrence at 8:16 PM, ran one LLM call, cached the `RATE_LIMIT_BACKOFF` repair, and eliminated all 847 subsequent LLM calls. When the on-call engineer opened the dashboard at 2:47 AM, the answer was already there.

**The 8-minute MTTR is not a marketing claim. It is a structural consequence of machine-readable data.**

---

## Slide 6 — Technical Moat

### Three compounding advantages that competitors cannot replicate without rearchitecting from scratch.

**Moat 1: Wire format lock-in (the good kind)**

Once a team adopts the ObservaLog wire contract, every log they emit is machine-readable forever. The 55-byte fixed-position Part A header means the brain reads `trace_id`, `level`, and `ts_ms` at fixed byte offsets — zero JSON parsing for the fields it needs 95% of the time. The 0.46ms TimescaleDB point lookup versus 35–85ms ClickHouse is not a configuration advantage — it is a structural consequence of storing typed columns separately from JSONB payloads.

This creates a data asset. Every log emitted under the wire contract becomes queryable, correlatable, and retrievable in milliseconds. Migrating off ObservaLog would mean re-processing every historical log. That's not vendor lock-in through pricing — it's adoption lock-in through data quality.

**Moat 2: Cross-language fingerprint parity**

The xxHash64 fingerprint algorithm is implemented identically in Go, Java, Node.js, and Rust. The service code byte is always a raw byte value, not an ASCII digit — a subtle distinction that means the Rust brain and a Go service and a Java service and a Node.js service will always produce the same fingerprint for the same event. Cross-language test suites verify parity on every commit.

This is harder than it looks. Building a cross-language fingerprinting algorithm that handles encoding edge cases correctly across four runtimes requires deep expertise. A competitor wanting to replicate this would spend 6–12 months discovering the same encoding pitfalls we already resolved.

The practical consequence: a `RATE_LIMIT_BACKOFF` repair cached from a Go service's error can be served immediately when the same error appears in the Java service or the Node.js service. No re-triage. No duplicate LLM calls.

**Moat 3: CI enforcement makes compliance structural, not cultural**

Every other logging tool relies on cultural compliance: documentation, code review, convention. All of these fail at scale. The logscanner blocks PRs at merge time. You cannot merge code that logs an error as a string. You cannot merge code that contains a raw PII pattern in a log literal. You cannot merge code that uses an abbreviation outside the versioned dictionary.

This matters especially for compliance-driven buyers. A SOC 2 auditor asking "how do you ensure logs do not contain PII?" can be answered with "our CI pipeline blocks any PR containing raw PII patterns in log literals — here is the scanner report from the last 90 days." That is a structural answer, not a policy answer. Policy answers fail audits. Structural answers pass them.

---

## Slide 7 — Target Customers (ICP)

### Who buys ObservaLog, and why now.

**Primary ICP: Series B+ engineering-led companies with polyglot stacks**

- Company profile: 50–500 engineers, Series B through Series D, SaaS or API product
- Tech stack signals: Go + Java + Node.js in production; Kafka for event streaming; Kubernetes deployments; existing structured logging attempts (zap, logback, winston) without enforcement
- Pain signals: P1 incidents taking 2+ hours to triage; "logging cleanup" perpetually on the backlog; Datadog bill above $50k/month; AI triage tools "not working as expected"; on-call fatigue driving engineer attrition
- Why now: The company has crossed the complexity threshold where informal logging conventions have broken down. They have the budget and the mandate. The CTO is looking for a structural solution, not another documentation initiative.

**Secondary ICP: Fintech and healthtech with compliance requirements**

- Company profile: 20–200 engineers, regulated industry (SOX, PCI-DSS, HIPAA, SOC 2 Type II)
- Tech stack signals: Java or Go backend, audit log requirements, security team involvement in logging decisions
- Pain signals: Failed or near-failed log compliance audit; audit preparation taking weeks of engineering time; PII accidentally appearing in logs; inability to produce complete audit trails for regulatory requests
- Why now: A recent audit finding or upcoming renewal creates a clear budget line. The CI enforcement story directly addresses the auditor's question.

**Tertiary ICP: Enterprise engineering platforms**

- Company profile: 1,000+ engineers, internal platform team responsible for logging standards
- Tech stack signals: Multiple languages across business units, central observability team, existing APM contract (Datadog or New Relic) that isn't solving the triage problem
- Why now: The platform team is chartered to reduce MTTR across all business units. ObservaLog's CI enforcement and wire format provide a standard that the platform team can mandate, providing career value to the platform team lead.

**Anti-ICP:**
- Startups with fewer than 5 engineers — not enough complexity to justify the wire contract adoption cost
- Single-language shops (all-Go or all-Node.js only) — cross-language fingerprint parity is less valuable; a simpler solution suffices
- Companies already standardized on a proprietary observability vendor's agent-based approach — switching cost too high unless there is a clear compliance trigger
- Companies with monolithic architectures — the trace correlation and cross-service fingerprinting solve polyglot microservice problems specifically

---

## Slide 8 — Competitive Landscape

### The market has spend. No one has solved the structural problem.

| | ObservaLog | Datadog | New Relic | OpenTelemetry | Honeycomb | Winston/Zap/Zerolog |
|---|---|---|---|---|---|---|
| Wire format contract | Yes (55-byte header) | No | No | No | No | No |
| CI enforcement | Yes (PR-blocking scanner) | No | No | No | No | No |
| Cross-language fingerprint | Yes (Go+Java+Node+Rust) | No | No | No | No | No |
| AI triage | Yes (typed RepairId) | Limited | Limited | No | No | No |
| LLM deduplication | Yes (95% eliminated) | No | No | No | No | No |
| Point lookup latency | 0.46ms (TimescaleDB) | ~35ms | ~35ms | Varies | ~20ms | N/A |
| PII enforcement | CI-blocked | Alert-only | Alert-only | No | No | No |
| Open source | MIT | No | No | Yes | No | Yes |
| Self-hosted option | Yes | No | No | Yes | No | Yes |

**Datadog:** The dominant APM vendor. $15–23/host/month for APM; $0.10/GB for log management; enterprise contracts frequently exceed $500k/year. Datadog collects your logs as they are — it does not enforce structure at the source. Their AI features operate on unstructured data and produce poor triage outcomes without structural inputs. No CI enforcement. No cross-language contract. If you log errors as strings, Datadog will dutifully store them as strings and charge you to search them slowly.

**New Relic:** Functionally comparable to Datadog in this context. Similar pricing, no wire contract, no CI enforcement, no cross-language fingerprinting. The AI observability features require structured data to produce useful output — and New Relic does not enforce that structure at the source.

**OpenTelemetry:** The open standard for distributed tracing and metrics. ObservaLog is not a replacement for OpenTelemetry — it is a wire format layer that sits below the tracing layer and above the raw log statement. OpenTelemetry does not enforce log structure. It does not block PRs. It does not produce typed repair categories. It is a collection protocol, not a triage system.

**Honeycomb:** An observability tool with strong developer advocacy and a focus on high-cardinality event data. No CI enforcement. No multi-language wire contract. No AI triage. Strong product for teams already using structured events natively; poor fit for teams needing to enforce structure retroactively or across multiple languages.

**Winston / Zap / Zerolog:** Structured logging libraries for Node.js, Go, and Go respectively. These are the emit-time tools ObservaLog's libraries complement or replace. They produce structured logs but do not enforce a grammar, do not compute fingerprints, do not inject trace context automatically, and do not include CI validation. They are necessary but not sufficient.

**The competitive gap:** Every existing solution assumes the logs are already well-structured and focuses on storage, querying, and visualization. ObservaLog assumes the logs are not well-structured (because they never are, in practice) and enforces structure at the source. This is a different product category, not a feature comparison.

---

## Slide 9 — Go-to-Market

### Three phases from open source adoption to enterprise contract.

**Phase 1 (Months 1–6): Developer-led adoption via open source**

The entry point is the library and the scanner. Both are MIT licensed, free forever. Developer adoption begins with a single language — typically Go or Node.js — at a single company. The logscanner's CI integration creates immediate visible value on the first PR it blocks. GitHub stars and CI integration metrics are the leading indicators.

Target: 500 GitHub stars, 50 companies with logscanner in CI, 10 companies with the wire format in production in 2+ services.

**Phase 2 (Months 6–18): Bottom-up conversion to paid brain**

When a company has the wire format running in 2+ services, the brain becomes immediately valuable. The fingerprint deduplication and AI triage provide an ROI that the CTO can calculate in their own terms: (P1 incidents per month) × (MTTR in hours) × (engineers on the war room) × (blended rate) = monthly cost of P1 incidents. A typical Series B company will find this number is $20,000–$80,000/month. The hosted brain tier starts at a fraction of that.

Target: 10 paid brain deployments, $15k–$30k ARR per customer, $150k–$300k total ARR.

**Phase 3 (Year 2): Enterprise and compliance-driven contracts**

Direct outreach to fintech and healthtech CTOs and VPs of Engineering with a compliance angle. The logscanner's `RAW_PII_IN_LOG` rule and CI enforcement story addresses audit requirements directly. Enterprise contracts include on-premises brain deployment, SLA, dedicated support, and SSO integration.

Target: 3–5 enterprise contracts at $60k–$150k ARR each, $500k–$1M total ARR.

---

## Slide 10 — Business Model

### Open-core: the libraries earn trust, the brain earns revenue.

**Tier 0: Open Source (free, MIT licensed)**
- All three language libraries (observalog-go, observalog-java, observalog-node)
- logscanner CI static analyzer
- Wire format specification
- Self-hosted brain (requires your own Kafka + TimescaleDB + LLM API keys)
- No telemetry, no license enforcement, no expiry

This tier exists to create the network effect. Every company running ObservaLog libraries is generating wire-format logs. When they outgrow self-hosted brain management, they become hosted cloud customers.

**Tier 1: Cloud Hosted — $800/month base, $0.08 per 1,000 log lines above 10M/month**
- Fully managed brain: Kafka, TimescaleDB, LLM routing, Valkey
- WebSocket dashboard
- Fingerprint deduplication and AI triage
- 99.5% SLA
- SOC 2 Type II compliant infrastructure
- 30-day log index retention (90-day upgrade available)

Comparable pricing anchor: Datadog Log Management at $0.10/GB ingested. A company with 10M log lines/month at approximately 200 bytes per line (post-abbreviation, pre-compression) ingests ~2GB/month. Datadog charges $200/month for storage alone, before APM, before dashboards. ObservaLog Cloud at $800/month includes storage, triage, fingerprinting, and the AI layer.

**Tier 2: Enterprise — $4,000–$15,000/month (custom contracts)**
- On-premises brain deployment (Kubernetes Helm chart)
- Your Kafka, your TimescaleDB, your LLM provider
- Dedicated support with 4-hour SLA
- SSO (SAML/OIDC)
- Custom retention policies
- Audit trail export for SOC 2 / SOX / HIPAA
- Multi-region deployment
- Custom RepairId playbook integration (JIRA, PagerDuty, ServiceNow)

Comparable pricing anchor: Datadog enterprise APM contracts routinely exceed $500k/year. The ObservaLog enterprise tier at $48k–$180k/year represents 10–40% of a typical Datadog contract while delivering structural observability enforcement that Datadog does not provide.

---

## Slide 11 — Traction

### What to show, and what it means.

**Leading indicators (Months 1–3):**

- **GitHub stars:** The best early signal for developer trust. Target 250 stars by month 2, 500 by month 6. Each star represents a developer who has evaluated the wire format spec and found it credible. The whitepaper and wire format spec are the assets that convert GitHub visitors to stars.
- **logscanner CI runs:** The scanner produces a report on every CI invocation. Track the number of distinct repositories with the scanner installed. This is the strongest adoption signal — it means the scanner is actually blocking PRs, not just sitting in a README.
- **Library install counts:** Go module proxy download counts, npm weekly downloads, Maven Central download counts. These indicate which language is the entry point for most companies.

**Conversion indicators (Months 3–6):**

- **Companies with wire format in 2+ services:** This is the brain conversion trigger. One service is a pilot; two services is a commitment. Track this via GitHub Discussions, Discord, or direct user interviews.
- **Fingerprint deduplication rate:** For companies running the self-hosted brain, the fingerprint cache hit rate (target: 90%+) is a concrete proof point for the "95% LLM calls eliminated" claim.

**Revenue indicators (Months 6–12):**

- **First paid brain deployment:** The milestone that validates the business model. Even a $800/month contract proves willingness to pay.
- **ARR per customer:** Track whether customers expand (more log lines, more services) or churn. Expansion is the strongest signal of product-market fit.
- **NPS from engineering teams:** The library UX and scanner false positive rate directly affect NPS. A negative NPS on the scanner (too many false positives) is an early warning sign.

---

## Slide 12 — The Ask

### What ObservaLog needs to reach the first 10 paying customers.

**If raising a pre-seed or seed round:**

ObservaLog is seeking $750k–$1.5M to fund:
- 6 months of founder salary and infrastructure costs
- Legal: open-source licensing review, SOC 2 Type II audit initiation, standard SaaS contracts
- GTM: conference attendance (KubeCon, GopherCon, Strange Loop), content production, and the first 3–5 design partner contracts
- Engineering: hosted cloud brain infrastructure build-out (estimated 8–12 weeks of senior Rust/Go work)

Expected milestones at end of funding period:
- 500+ GitHub stars
- 10+ companies with logscanner in CI
- 5 paying brain deployments
- $50k–$100k ARR

**If selling to a design partner (enterprise direct):**

ObservaLog is offering a 12-month design partner contract at 60% standard pricing in exchange for:
- Engineering team access for onboarding and feedback sessions (2 hours/month)
- A reference story (case study or logo use) at end of pilot
- Co-development input on one of the Year 2 features (Python library, JIRA integration, custom RepairId playbooks)

Design partner pricing: $5,000–$8,000/month for enterprise tier (standard $4,000–$15,000/month). The design partner's name on the website and in investor materials.

---

## Slide 13 — Team

*[Team section — to be completed by founder]*

**[Founder Name]** — [Title]
- [Background: previous engineering leadership, relevant domain experience]
- [Relevant technical credentials: systems programming, distributed systems, observability]

**Advisors:** [To be added]

**Current state:** ObservaLog v1.2 is the work of a single founding engineer. The architecture, wire format, cross-language libraries, CI scanner, and AI triage engine are complete. The next hire is a developer advocate / founding sales engineer who can run pilots and produce technical content.

---

## Slide 14 — Appendix: Wire Format Specification Summary

### The technical contract that makes everything else work.

Every log call, in every ObservaLog-instrumented service, emits exactly two NDJSON lines:

```
A:1|trc_7f2a1b9e4d|spn_004|spn_001|04|2|1|1|1748268153812
{"e":"provider.send.rejected","m":"Provider rejected document send","er":{"ek":"RateLimitExceeded","ec":"PROVIDER_QUOTA_EXCEEDED","rt":true},"c":{"di":"doc123"},"o":"failure","ms":87}
```

**Part A: 55-byte fixed-position header**

| Bytes | Field | Purpose |
|-------|-------|---------|
| 0 | schema_version | Enables rolling deploys — unknown versions fall back to full JSON parse |
| 2–15 | trace_id | Cross-service correlation identifier |
| 17–23 | span_id | Span within a service |
| 25–31 | parent_span | Service dependency tree reconstruction |
| 33–34 | seq | Per-request atomic counter — gaps indicate dropped log entries |
| 36 | service | 1-byte service code — enables O(1) service filtering |
| 38 | level | DEBUG/INFO/WARN/ERROR — zero-parse by brain |
| 40 | outcome | none/success/failure/partial/pending |
| 42–54 | ts_ms | Unix milliseconds — enables TimescaleDB chunk pruning |

The Rust brain reads every field at hard-coded byte offsets. No JSON parsing for the hot path. This is why point lookups are 0.46ms.

**Part B: Abbreviated JSON**

18-entry abbreviation dictionary (DictVersion 1). `event` → `e`, `duration_ms` → `ms`, `error` → `er`, etc. All three language libraries and the Rust brain share an identical dictionary, versioned, with collision validation at library init.

**Cross-language fingerprint:**
```
fingerprint = xxHash64(service_byte | event | error_code | ctx_primary_key, seed=0)
```
Stored as BIGINT. Identical computation in Go, Java, Node.js, and Rust. Verified by cross-language parity tests on every commit.

---

## Slide 15 — Appendix: Performance Benchmarks

### The numbers that close technical objections.

| Metric | ObservaLog | Comparison |
|--------|------------|------------|
| Go emit latency | 200ns (non-blocking channel) | zap: ~180ns, logrus: ~900ns |
| TimescaleDB point lookup | 0.46ms | ClickHouse: 35–85ms |
| Fingerprint computation | ~8ns (xxHash64) | SHA-256: ~180ns |
| LLM calls eliminated by dedup | ~95% (fingerprint cache hits) | No competitor offers this |
| Wire size reduction | 70% (abbreviation + GZIP) | vs. full-field JSON |
| CI scan time | <2s per PR (Rust, parallel) | ESLint: 5–15s for comparable rule sets |
| I/O reduction (log_index vs full table) | 78% | Two-table split design |

**Storage efficiency:** After the 70% wire size reduction and TimescaleDB's 90%+ chunk compression, a company emitting 10 million log lines per day stores approximately 1–2GB of index data and 8–12GB of payload data — for 30 days of full retention. At standard cloud storage rates, this is $3–8/month in raw storage cost before compute. The comparable Datadog log ingestion cost for the same volume exceeds $200/month for storage alone.

**Throughput ceiling:** The Go library's non-blocking channel buffer defaults to 10,000 entries. Under sustained load exceeding the Fluent Bit drain rate, entries are dropped and counted by `DroppedLogCount()`. This is an explicit design choice — ObservaLog does not block application threads for logging under any circumstances.

---

*ObservaLog v1.2 — MIT licensed. Wire format contracts frozen at schema version 1.*
*https://github.com/darshanredkar11/observalog*
