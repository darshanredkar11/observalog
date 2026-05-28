# ObservaLog — Go-to-Market Strategy

**Version 1.0 — for founder use in first customer conversations**

---

## 1. Ideal Customer Profile (ICP)

### Primary ICP: Series B+ polyglot engineering teams

**Company characteristics:**
- Series B through Series D, or equivalent growth-stage revenue-generating company
- 50–500 engineers total; 3–20 engineers on the platform or infrastructure team
- SaaS, API product, or high-transaction-volume backend (payments, logistics, healthcare data)
- Engineering-led culture — the CTO or VP Engineering makes tooling decisions, not IT procurement

**Tech stack signals (qualify in):**
- Go + Java + Node.js all in production (or any two of these three)
- Kafka or Pulsar for event streaming
- Kubernetes deployments (Helm or Kustomize)
- Existing logging libraries already adopted (zap, logback, winston) — this indicates they have tried structured logging before
- DataDog or New Relic subscription over $30k/year (budget signal; product frustration signal)
- On-call rotation with more than 5 engineers

**Pain signals (qualify in immediately):**
- P1 incidents consistently take more than 2 hours to resolve
- "Logging cleanup" appears on the engineering roadmap and never gets completed
- The phrase "we need to standardize our logging" has been said in the last 6 months
- AI triage tool (Datadog Watchdog, New Relic AI, or any LLM-based tool) has been evaluated and found disappointing
- On-call attrition or engineer complaints about incident response burden
- Compliance audit (SOC 2, SOC 1) is upcoming or recently struggled

**Economic buyer:** CTO (companies under 200 engineers) or VP Engineering / Director of Platform Engineering (larger companies). Occasionally the Head of SRE or DevOps Platform Lead has budget authority.

**Champion (the person who sells it internally):** Staff or Principal Engineer, SRE Lead, or Platform Engineer who has personally felt the pain of a long P1 triage and has the credibility to advocate for a new tool.

**Decision timeline:** Typically 30–90 days from first conversation to signed contract for Series B/C. Enterprise is 90–180 days. The logscanner can be piloted in a single afternoon — this short time-to-value is the primary driver of fast decisions.

---

### Secondary ICP: Compliance-driven fintech and healthtech

**Company characteristics:**
- Financial services (payments, lending, trading, neobanking) or healthcare technology (EHR, patient data, health insurance)
- Under SOX, PCI-DSS, HIPAA, or SOC 2 Type II audit obligations
- 20–300 engineers
- Has recently failed or nearly failed a log-related audit finding, or is preparing for a first formal audit

**Why ObservaLog is different for this segment:**

The compliance buyer is not primarily motivated by MTTR reduction. They are motivated by audit defensibility. The logscanner's `RAW_PII_IN_LOG` rule directly addresses the most common log-related audit finding: personally identifiable information appearing in log output due to developer error. CI enforcement turns the answer to "how do you prevent PII in logs?" from a policy answer ("we have a guideline") to a structural answer ("our CI pipeline blocks it at merge time — here is the 90-day scanner report").

The wire format's typed event grammar also directly supports audit trail requirements. An auditor asking to see "all log entries related to user_id=X" gets a query result in 0.46ms from `log_index`, not a 30-minute log search.

**Tech stack signals for this segment:**
- Java is particularly common (Spring Boot, Quarkus)
- Regulatory compliance team or security team involved in engineering decisions
- SOC 2 Type II in progress or recently completed
- Annual pen test and log review as part of the security program

---

### Anti-ICP: Who NOT to sell to

**Too small:**
- Fewer than 10 engineers — the wire format adoption cost exceeds the benefit; recommend coming back in 12 months
- Pre-product companies — their logging problems are not yet their most pressing problem

**Wrong stack:**
- All-Python shops — observalog-python is on the roadmap but does not exist today; do not sell to this segment
- All-Ruby shops — same situation
- Single-language monoliths — the cross-language fingerprint parity and multi-service trace correlation provide most of the value; single-language monoliths get less than 50% of the benefit

**Wrong culture:**
- Companies already locked into a proprietary APM agent (Dynatrace, AppDynamics enterprise) where switching cost is prohibitive — not worth the effort until a renewal conversation creates an opening
- Companies with low engineering autonomy where a purchasing committee, not engineers, controls tooling decisions — ObservaLog sells bottom-up; top-down procurement without an internal champion fails

**Wrong stage:**
- Companies in active M&A processes — they will not adopt new tooling during integration
- Companies that have just done a complete logging rewrite (in the last 6 months) — the pain is not fresh enough

---

## 2. Positioning and Messaging

### The core insight, in one sentence

AI triage fails because the data is wrong, not because the AI is wrong. ObservaLog fixes the data.

### Messaging by persona

**For CTOs:**

"Your engineers are spending 4 hours per P1 incident correlating unstructured logs across three languages. ObservaLog makes your logs machine-readable at the source — enforced at CI, not by convention. The AI triage brain reduces MTTR to under 8 minutes. You get the answer before the war room finishes coffee."

Supporting points:
- Wire format enforced at CI: you cannot merge bad logs
- Cross-language fingerprint dedup: 95% of LLM triage calls eliminated by deterministic pattern matching
- 0.46ms point lookup: the brain doesn't wait for Kafka lag to catch up
- Self-hosted option: your logs never leave your infrastructure

**For VP Engineering / Engineering Managers:**

"Never merge a PR that logs an error as a string again. The logscanner is a CI check — like a linter, but for observability. It takes 10 minutes to install. On the first PR it blocks, your engineers understand why structured logging matters more than any documentation could explain."

Supporting points:
- logscanner blocks five specific, common mistakes: unstructured errors, undeclared events, missing duration fields, raw PII, undeclared abbreviations
- Works on Go, Java, and TypeScript/JS today — one PR to install in your CI pipeline
- Zero configuration: the grammar is enforced by the wire format spec, not a config file
- Blocks bad logs before they reach production; reduces the blast radius of the next incident

**For SRE / On-call leads:**

"The last time you debugged a P1, how long did you spend opening log tabs in three different windows? ObservaLog gives you the full trace, the failure class, and the typed repair category in a single WebSocket dashboard. The brain has already triaged it before you open your laptop."

Supporting points:
- Sequence counter gaps detect dropped log entries in transit — you know when you're missing data
- Failure class (Isolated, Cascading, ExternalDependency, Timeout, GapDetected) tells you where to look first
- RepairId (NETWORK_RETRY, RATE_LIMIT_BACKOFF, VALIDATION_FIX, etc.) routes directly to your runbooks
- False positive gap detection: 30-second grace window before a sequence gap is declared a real drop

**For CFOs / CEOs (ROI framing):**

"A single P1 incident costs your company $3,000–$20,000 in direct engineer labor, plus revenue loss and customer impact. ObservaLog at $800–$4,000/month pays for itself in the first incident it accelerates. Here is the calculation template."

ROI calculation template to share in conversations:
```
Monthly P1 incident cost =
  (P1 incidents per month)
  × (MTTR in hours)
  × (engineers in the war room)
  × (blended engineering rate per hour)

Example: 3 incidents × 4 hours × 7 engineers × $180/hr = $15,120/month

ObservaLog Cloud tier: $800/month
Monthly ROI: $14,320 (93% cost reduction on P1 labor alone)
```

**For compliance-driven buyers (fintech/healthtech):**

"Your next SOC 2 Type II audit will ask how you prevent PII from appearing in log output. The answer 'we have a policy' fails. The answer 'our CI pipeline blocks it at merge time — here is 90 days of scanner reports' passes. ObservaLog's logscanner is a structural compliance control, not a policy."

---

## 3. Sales Motion

### Motion 1: Bottom-up / Product-Led Growth (PLG)

This is the primary motion for the first 18 months.

**The path:**
1. A developer or SRE finds ObservaLog on GitHub (through a blog post, conference talk, or search for "go structured logging ci")
2. They clone the repo and run the logscanner on their own codebase in under 5 minutes
3. The scanner finds violations — almost every codebase has them
4. They open a PR to fix one violation, showing the scanner output as motivation
5. The team adopts the library in one service (the one that causes the most on-call pain)
6. The library's automatic trace context injection and typed Err structs produce immediately better logs
7. The developer opens a GitHub Discussion or Discord conversation — becomes an internal champion
8. The champion shows the CTO the MTTR improvement after the next incident
9. The CTO looks at the hosted brain tier pricing and signs up

**Key friction points to eliminate:**
- Time-to-first-value must be under 30 minutes (scanner installable in one CI PR, library installable with `go get` / `npm install` / `mvn import`)
- The scanner must have a near-zero false positive rate on the first run — one false positive kills trust
- The library must not require any configuration file or environment variable to start working

**What to build for PLG to work:**
- A `logscanner --report` flag that produces a markdown-formatted PR comment (integrates with GitHub Actions)
- A `npx observalog-scan .` one-liner for Node.js teams
- A Homebrew tap and a pre-built binary for the scanner
- A working example repository with 3 services showing the full wire format in action

---

### Motion 2: Top-down enterprise outreach

This is the primary motion for compliance-driven buyers and enterprise deals.

**The path:**
1. Direct outreach to CTO or VP Engineering at a target company (see outreach playbook below)
2. 30-minute discovery call focused on current P1 MTTR, logging conventions, and compliance requirements
3. 2-week paid pilot: logscanner installed in CI on two repos; one service instrumented with the library; brain running on ObservaLog Cloud
4. Post-pilot ROI review: count how many PRs the scanner blocked; measure the MTTR delta on any incidents during the pilot; calculate the cost savings
5. Sign a 12-month contract

**Who to call first:** CTO at fintech companies that have announced SOC 2 Type II compliance recently (press releases, Crunchbase, LinkedIn updates). The compliance trigger creates budget urgency.

---

### Motion 3: Inbound via content and community

This is the primary long-term compounding motion.

**The mechanism:** A developer reads a blog post or watches a conference talk about "why AI triage fails on unstructured logs." They recognize their situation. They visit GitHub. They star the repo. They try the scanner. The PLG motion begins.

**The key metric:** GitHub stars as a leading indicator for conversion pipeline. Every 100 stars generates approximately 3–5 qualified inbound conversations based on comparable open-source developer tools.

---

## 4. Outreach Playbook

### Cold email: CTO at Series B fintech

**Subject:** Your AI triage tool is producing garbage — and it's not the AI's fault

---

Hi [Name],

I've been following [Company] for a while — the work on [recent funding / product launch / engineering blog post] caught my attention.

Quick question: when your team debugs a P1 incident, how many log windows are open across how many services? And are those logs in Go, Java, and Node.js, or do you have a single-language stack?

I'm building ObservaLog — a structured observability system that solves a specific problem: AI triage fails on polyglot stacks because the logs are structurally incompatible across languages. We fix that with a wire format contract enforced at CI time and a triage brain that produces typed repair categories instead of LLM prose.

The short version: our CI scanner blocked a "log error as string" PR at [Reference company type] last week. Six days later, the brain used the error code from that blocked PR to triage a P1 in 8 minutes instead of 4 hours.

We're open source (MIT) and looking for 3–5 design partner companies to run a paid pilot. If your team is running Go or Java or Node.js in production and incident triage is a known pain point, I'd like to show you the wire format spec and the scanner output on a repository of your choice.

15 minutes this week?

[Your name]
https://github.com/darshanredkar11/observalog

---

### Cold email: VP Engineering at 100+ person SaaS

**Subject:** One CI check that prevents half your P1 investigation time

---

Hi [Name],

You're running [# of engineers] engineers across what looks like a polyglot stack — Go backend, Node.js frontend services, Java somewhere in the mix if I had to guess.

Here's a pattern I see at companies your size: someone installs a structured logging library, writes documentation about how to use it correctly, and then three months later 40% of production error logs still look like this:

```
logger.error("payment processing failed: " + err.message)
```

No trace ID. No error code. No way to correlate with the downstream service. Your AI triage tool, when you eventually evaluate one, will produce unhelpful output on this data.

We built a CI scanner (Rust, 2-second PR check) that blocks this class of problem before it merges. `UNSTRUCTURED_ERROR` is the rule — it fires when an error is passed as a plain string rather than as a typed Err struct. Five minutes to install. Works on Go, Java, and TypeScript.

The scanner is MIT licensed and free. If your team finds it useful, we have a triage brain (Kafka + TimescaleDB + LLM dedup) that works with the structured logs it enforces.

Happy to send you a pre-built binary and instructions. No sales call required for the scanner — try it first.

[Your name]

---

### Cold email: DevOps lead at enterprise (compliance angle)

**Subject:** How to give your SOC 2 auditor a structural answer on PII in logs

---

Hi [Name],

Your SOC 2 Type II audit will (or already did) ask a question like this: "How do you ensure that personally identifiable information does not appear in your application logs?"

The typical answer: "We have a logging policy and developers are trained to avoid logging PII."

The auditor's internal note: "Detective control only. Policy not enforced structurally. Finding: MEDIUM."

We built a CI-level static analyzer that turns this into a structural control. The `RAW_PII_IN_LOG` rule blocks any PR that contains a raw email address, phone number, or SSN pattern in a log literal — before it merges. The scanner report is a 90-day artifact you can hand directly to the auditor.

The scanner also enforces four other rules (undeclared events, unstructured errors, missing duration fields, undeclared abbreviations) that together enforce a log wire format contract across Go, Java, and TypeScript.

It's MIT licensed. Install it in CI in 5 minutes. Let me know if you want the pre-built binary and a sample GitHub Actions step.

[Your name]
https://github.com/darshanredkar11/observalog

---

### LinkedIn message template (connection request + follow-up)

**Connection request note (300 character limit):**

"Working on structured observability enforcement for polyglot stacks — CI scanner that blocks 'log error as string' PRs before merge. Your work on [their engineering blog / recent post] suggests you'd find it interesting. Happy to share the GitHub link."

**Follow-up after connection (send 3 days after acceptance):**

"Thanks for connecting. The GitHub link if you want it: https://github.com/darshanredkar11/observalog — specifically the logscanner README. One CI check, works on Go/Java/TypeScript, zero config. If you try it and it finds nothing in your codebase, I'll buy you a coffee. (It always finds something.)"

---

### Conference / meetup talk angle

**Title: "Why your AI triage tool is hallucinating (and how to fix it in 3 lines of Go)"**

**Abstract:**

You bought an AI observability tool. Or you built one. It's hallucinating root causes, producing prose that doesn't route to a runbook, and your team has started ignoring it. The problem isn't the LLM. The problem is the data.

In this talk, I'll show you what a structured log wire format looks like, why a 55-byte fixed-position header beats JSON for triage speed, how a cross-language fingerprint algorithm eliminates 95% of LLM calls, and how a CI scanner enforces the contract before bad logs ever reach production. Three lines of Go (or Java, or Node.js) to instrument your first service; one CI step to start blocking unstructured errors.

You'll leave with a working scanner binary, a wire format spec you can adopt today, and a clear mental model for why structure at the source is the only solution to AI triage failure.

**Talk angle notes:** This talk works at GopherCon (Go-specific library demo), KubeCon (infrastructure + observability audience), QCon (architecture audience, focus on the moat analysis), and GOTO (enterprise engineering audience, compliance angle). Adjust the code examples to match the audience's primary language.

---

## 5. Pricing Strategy

### Pricing philosophy

ObservaLog uses open-core pricing: the highest-friction parts of the product (library adoption, CI integration) are free, creating the data asset (structured logs) that makes the paid product (the brain) valuable. This is not a freemium play — the open-source tier is fully functional for teams that want to self-host. The cloud tier charges for operational overhead elimination, not feature gating.

### Tier 0: Open Source (MIT, free forever)

Includes:
- observalog-go, observalog-java, observalog-node libraries
- logscanner CI static analyzer
- Wire format specification
- Self-hosted brain source code (requires operational investment)
- All future library and scanner updates

This tier is the primary acquisition channel. It is never compromised by feature gating. Moving features from open-source to paid would destroy the trust that makes the paid product possible.

### Tier 1: ObservaLog Cloud

**Pricing:** $800/month base, includes 10M log lines/month.
Additional log lines: $0.08 per 1,000 lines above 10M/month.
Annual prepay: 15% discount ($8,160/year vs $9,600 month-to-month).

Includes:
- Fully managed Kafka cluster (dedicated, not shared)
- TimescaleDB hosted with automated backups and 99.5% uptime SLA
- Valkey for sequence gap detection
- LLM routing (defaults to Claude Haiku for cost efficiency; upgradeable to Sonnet for complex traces)
- WebSocket dashboard with fingerprint deduplication view and RepairId outputs
- 30-day log index retention (90-day upgrade: $200/month)
- SOC 2 Type II compliant infrastructure
- Email support with 24-hour response SLA

**Pricing anchors and comparisons:**
- Datadog Log Management: $0.10/GB ingested. 10M lines × ~200 bytes = 2GB = $200/month. Datadog at comparable volume: $200/month for storage alone, before APM ($31/host/month × typical 10 hosts = $310/month). Total Datadog for comparable capability: $510+/month with no wire contract, no CI enforcement, no AI triage.
- Honeycomb: $100/month for 20M events with 60-day retention. No AI triage, no CI enforcement, no multi-language contract.
- ObservaLog at $800/month includes everything: storage, triage, AI dedup, CI integration support.

**Expected gross margin at scale:** 70–75%. Compute cost at 10M lines/month is approximately $100–150/month (Kafka, TimescaleDB, Valkey) plus $50–100/month in LLM costs (after 95% dedup). At $800/month revenue, margin is approximately 80% at this tier.

### Tier 2: ObservaLog Enterprise

**Pricing:** Starting at $4,000/month; typical contract $6,000–$12,000/month. Annual contracts only.

Pricing variables:
- Log volume (10M to 500M+ lines/month)
- Number of services instrumented
- Retention requirements (30 days to 2 years)
- Deployment model (hosted vs. on-premises)

Includes everything in Tier 1, plus:
- On-premises deployment via Kubernetes Helm chart (customer provides infrastructure)
- Customer-controlled LLM provider (bring your own OpenAI / Anthropic / Azure OpenAI key)
- 4-hour response SLA with dedicated Slack channel
- SSO via SAML 2.0 or OIDC
- Custom retention policy configuration
- Audit trail export (CSV or JSON) for SOC 2, SOX, HIPAA compliance documentation
- RepairId playbook integration (PagerDuty, JIRA, ServiceNow) — custom configuration
- Priority feature requests and co-development input
- Quarterly business reviews

**Why enterprise customers pay this rate:** Datadog enterprise contracts at comparable engineering team sizes routinely exceed $500k/year. ObservaLog at $72k–$144k/year (enterprise tier) represents 15–30% of a comparable Datadog spend while providing structural log enforcement that Datadog does not offer. The ROI story for enterprise closes on MTTR reduction alone.

### Negotiation guidance

- Do not discount the base rate more than 20% for annual prepay unless there is a design partner arrangement
- Design partner rate: 50–60% of standard pricing for first 12 months, in exchange for reference story and feedback access
- Volume discounts trigger at 100M log lines/month: $0.06/1,000 additional lines; at 500M: $0.04/1,000
- Never offer free-forever cloud access in exchange for introductions or social proof — it creates permanent cost with no conversion path

---

## 6. First 10 Customer Strategy

### Why the first 10 customers are different from every subsequent customer

The first 10 customers are not revenue milestones. They are proof points. Each one serves a different purpose in the GTM narrative:
- Customers 1–3: prove the product installs and works in a real production environment
- Customers 4–6: prove the product works across different tech stacks and team sizes
- Customers 7–9: prove the product works in a compliance-driven context
- Customer 10: the reference customer — the one whose name you say in every subsequent enterprise conversation

### The first 10: who to approach and why

**Customers 1–2: Former colleagues or professional network companies**

The highest-probability first customers are people who trust you personally. Identify 3–5 engineers you have worked with who are now at companies matching the primary ICP. Do not sell to them — ask them to try the scanner on their codebase and tell you what it finds. Every codebase has violations. When they show their CTO the scanner output, the conversation shifts from "a friend's startup" to "a tool that found real problems in our code."

Approach: direct message, zero formality, "I built something — will you try it and tell me what breaks?"

**Customers 3–5: GitHub early adopters**

The first 100 GitHub stars will include engineers who are actively evaluating observability tools. Monitor GitHub Discussions and Issues for anyone asking usage questions — these are your warmest leads. Respond personally, offer to do a 30-minute onboarding call, and ask permission to keep in touch for feedback.

Approach: GitHub Discussion follow-up, personal email, offer of a free onboarding call with the founder directly.

**Customers 6–8: Conference and meetup contacts**

A single GopherCon talk generates 20–40 meaningful engineering contacts. Of these, 3–5 will have active pain that matches the ICP. Follow up within 48 hours with a personal email referencing the specific question they asked or comment they made. This is not a sales sequence — it's a conversation that started at the talk.

Approach: Conference follow-up email within 48 hours, GitHub link, offer of a 15-minute "show me how to install the scanner" call.

**Customers 9–10: Direct cold outreach to fintech compliance target**

Identify 10 fintech companies that have announced SOC 2 Type II completion in the last 12 months (press releases, security.txt files, trust center pages). Cold email the CTO or Head of Security Engineering with the compliance angle. These are slower sales (60–90 days) but higher ACV ($4k–$12k/month) and provide the compliance reference story that unlocks the secondary ICP segment.

Approach: personalized cold email using the SOC 2 compliance angle, direct offer of a 2-week paid pilot with a refund guarantee if the scanner finds zero violations.

### What to offer the first 10 customers

**Option A: Free pilot (Customers 1–5)**
- 30-day free access to ObservaLog Cloud
- Weekly 30-minute check-in with the founder
- In exchange: written feedback after the pilot; permission to use their experience (anonymized) in marketing materials
- No obligation to convert, but present the Cloud pricing at day 25

**Option B: Paid design partner (Customers 6–10)**
- 12-month contract at 60% of standard Cloud or Enterprise pricing
- Bi-weekly 30-minute feedback call with the founder
- Priority feature requests — the founder commits to reviewing one feature request per quarter from each design partner
- In exchange: a reference quote and logo for the website; a case study (one published blog post) after 6 months; permission to name-drop in investor conversations

**The success metric that unlocks the conversion conversation:**

For every pilot, define one success metric at the start: adoption of the wire format in 2+ services within 30 days. If 2+ services are instrumented and the scanner is running in CI by day 30, the pilot is a success by definition — and the conversion conversation is straightforward. If only 1 service is instrumented, do a root cause analysis on what blocked the second service before trying to convert.

---

## 7. Content and Community Strategy

### Technical blog posts (titles and outlines)

**Post 1: "Why your AI triage tool is hallucinating: a structural explanation"**

Outline:
- The problem: AI triage tools produce garbage output on unstructured logs
- Why this happens: LLMs cannot extract fields that don't exist in the data
- What "structured" actually means: trace ID, error code, sequence counter, typed Err struct — not just JSON
- The polyglot problem: three services, three log shapes, no shared contract
- The fix: wire format contract enforced at CI time
- Call to action: try the logscanner on your codebase — link to binary download

Target audience: engineering managers, SRE leads, CTOs who have already tried an AI observability tool and been disappointed. SEO target: "AI triage logs not working", "structured logging AI observability", "LLM hallucination incident response".

**Post 2: "The 55-byte log header that made our triage 100x faster"**

Outline:
- The problem with JSON parsing at triage time: every field lookup requires parsing the full document
- What a fixed-position header is and why it matters for performance
- The ObservaLog Part A layout: 55 bytes, zero JSON parsing, Rust reads `trace_id` at byte 2
- Benchmark: 0.46ms TimescaleDB point lookup vs 35–85ms ClickHouse
- Why this matters at scale: 10M log lines/day × 0.46ms vs 35ms = the difference between a 2-second triage and a 2-minute triage
- How to adopt it: three lines in Go / Java / Node.js

Target audience: platform engineers, infrastructure engineers, performance-focused engineers. SEO target: "TimescaleDB vs ClickHouse logs", "structured logging performance Go", "fast log query TimescaleDB".

**Post 3: "We blocked 847 LLM calls with a 64-bit hash"**

Outline:
- The problem: LLM triage is expensive and slow for recurring errors
- What a fingerprint is in the ObservaLog context: xxHash64 of service+event+error_code+ctx_primary_key
- The math: 10M log lines/day × 5% error rate × 95% fingerprint cache hit = 47,500 LLM calls/day avoided
- Cross-language parity: the same hash in Go, Java, Node.js, and Rust
- The encoding gotcha: raw byte value, not ASCII digit — the mistake that breaks cross-language parity
- How to verify cross-language parity in your test suite

Target audience: engineers building LLM-based tools, cost-conscious engineering managers. SEO target: "reduce LLM API costs observability", "fingerprint deduplication logs", "xxHash64 cross-language".

**Post 4: "How to pass your SOC 2 audit question about PII in logs"**

Outline:
- The audit question: "How do you prevent PII from appearing in logs?"
- Why policy answers fail audits: detective control vs. preventive control
- What a structural answer looks like: a CI check that blocks the PR
- The logscanner `RAW_PII_IN_LOG` rule: what it checks, how it works
- How to install it in GitHub Actions in 5 minutes
- The 90-day report artifact: what to hand the auditor

Target audience: fintech and healthtech engineering managers, security engineers, DevOps leads with compliance responsibility. SEO target: "SOC 2 PII logs compliance CI", "prevent PII in logs CI pipeline", "log compliance audit engineering".

**Post 5: "The sequence counter trick that detects dropped log entries"**

Outline:
- The hidden problem: logs get dropped in transit and you never know
- What a sequence counter is: per-request atomic uint8, auto-increments on every Emit() call
- How the brain detects gaps: `WHERE trace_id = X AND service = Y ORDER BY seq` — a jump > 1 means a dropped entry
- The false positive problem: out-of-order Kafka delivery
- The 30-second Valkey grace window: how it eliminates false positives without coordination
- Why this matters for compliance: you can prove your logs are complete

Target audience: SRE engineers, platform engineers, compliance engineers. This post is highly technical and earns credibility with the engineering audience.

---

### GitHub repo strategy

**README priorities:**
- First 200 words: the problem statement and the one-sentence solution
- Code snippet within 3 scrolls: a 3-line Go (or Node.js) library usage example
- Scanner installation one-liner: `brew install observalog-scanner` or `curl -fsSL install.sh | sh`
- Benchmark table: visible in the README without scrolling on a laptop
- "Try it now" section: a command that runs the scanner against a demo repository with known violations

**Issue hygiene for PLG:**
- `good first issue` label on issues that require knowledge of the wire format spec but no Rust expertise — these attract Go, Java, and Node.js engineers who become library contributors and champions
- `help wanted` issues for Python library implementation (this generates community interest before the feature is built)
- Issue templates that ask for the language and stack — this doubles as a lead qualification form

**CONTRIBUTING.md priorities:**
- Cross-language parity test instructions: how to run the fingerprint parity test suite across all four implementations
- Wire format dictionary update process: how to propose a new abbreviation (dictionary is a contract, not a free-for-all)
- Scanner rule development guide: how to add a new scanner rule for Go, Java, or TypeScript

**Star campaign mechanics:**
- Post the repo to Hacker News Show HN once — not twice. One well-timed Show HN (Tuesday–Thursday morning Pacific time) with a clear demo generates 50–150 stars in 24 hours.
- Submit the "Why your AI triage tool is hallucinating" blog post to lobste.rs, r/programming, r/golang, and r/node simultaneously with the Show HN
- Do not use star-farming services or paid star campaigns — they produce low-quality stars that do not convert to users

---

### Conference talks

**GopherCon (Go audience):**
Title: "Building a log wire contract in Go: seq counters, typed Err structs, and CI enforcement"
Format: 30-minute technical talk with live demo of the logscanner on an open-source Go project
Goal: Library adoption from Go engineers; GitHub stars from the Go community

**KubeCon (Infrastructure/platform audience):**
Title: "Structured observability at the source: a wire format for polyglot Kubernetes services"
Format: 25-minute talk with emphasis on Fluent Bit integration and Kafka pipeline
Goal: Cloud brain interest from platform engineering teams; enterprise contacts

**QCon (Architecture audience):**
Title: "Why AI triage fails on unstructured logs and how to fix it architecturally"
Format: 40-minute architecture talk, no live demo, focused on the moat analysis and competitive differentiation
Goal: CTO and VP Engineering contacts; investor introductions

**NodeConf / Node.js Interactive (Node.js audience):**
Title: "AsyncLocalStorage and the log wire contract: structured observability for Node.js microservices"
Format: 25-minute technical talk focused on the Node.js library's AsyncLocalStorage context propagation
Goal: Node.js library adoption; npm download growth

---

### Developer social media strategy (X/Twitter, LinkedIn, Bluesky)

**Content cadence:** 3–4 posts per week during the first 6 months. Reduce to 2 per week once the content library is established.

**Post types that work for developer tools:**

1. **"I just found this" posts:** Take a real open-source Go/Java/Node.js repository and run the logscanner against it. Post the output (with violations highlighted). Tag the repo maintainer. Example: "Ran our CI log scanner against [popular open-source Go service]. Found 23 UNSTRUCTURED_ERROR violations and 4 RAW_PII_IN_LOG violations. [screenshot]. All fixable in < 1 hour. Scanner is MIT licensed: [link]"

2. **Performance benchmark posts:** A single number with context. "0.46ms. That's how long TimescaleDB takes to look up a trace by ID. ClickHouse takes 35–85ms. The difference is the two-table split [diagram]. [GitHub link]"

3. **"The mistake that breaks cross-language parity" posts:** Technical depth posts that demonstrate expertise. "The xxHash64 fingerprint must use the raw service code byte, not its ASCII digit. In Go: `string(uint8(code))`. In Java: `new byte[]{(byte)code}`. In Node.js: `Buffer.from([code])`. One line, four languages, all wrong in the same way if you're not careful. [GitHub link to cross-language parity test]"

4. **Incident stories (anonymized):** "A design partner's P1 this week: 41% failure rate, provider-service origin, 1,247 affected traces. Time from alert to RepairId: 8 minutes. RepairId: RATE_LIMIT_BACKOFF. No engineers paged. [link to how the brain works]"

**What not to post:**
- Competitor bashing — it reads as insecure and alienates potential customers who use those tools
- Vague "observability is broken" takes without a specific technical point
- Excessive fundraising or sales posts — the developer audience disengages immediately

---

## 8. Metrics and Milestones

### Month 1–3: Awareness and adoption signals

| Metric | Target | Measurement |
|--------|--------|-------------|
| GitHub stars | 250 | GitHub repo star count |
| logscanner CI runs | 15 distinct repos | Scanner telemetry (opt-in) or GitHub Actions runner count |
| Library installs: go | 500 module downloads | pkg.go.dev download count |
| Library installs: node | 200 npm weekly installs | npm stats |
| Blog posts published | 3 | Published to dev.to and company blog |
| Conference proposals submitted | 2 | Accepted at least 1 |
| Design partner conversations | 5 | CRM (even a spreadsheet) |

**Key risk at this stage:** The scanner has false positives that frustrate early adopters. Monitor GitHub Issues for "false positive" keywords. One unresolved false positive report in the first 30 days can kill the reputation. Fix within 48 hours.

---

### Month 3–6: Conversion and first revenue

| Metric | Target | Measurement |
|--------|--------|-------------|
| GitHub stars | 500 | GitHub |
| Companies with scanner in CI | 30 | Scanner telemetry or user-reported |
| Companies with wire format in 2+ services | 10 | User interviews, GitHub Discussions |
| Free pilots active | 5 | CRM |
| First paid brain deployment | 1 | Revenue |
| ARR | $10k–$25k | Stripe / billing system |
| NPS from library users | > 40 | Post-install survey (in GitHub Discussions) |

**Key milestone:** The first paid customer. This is binary — either you have one or you don't. Until there is one paid customer, every other metric is a proxy. The first paying customer validates that the value proposition is real, that the pricing is acceptable, and that the sales motion works.

**Key risk:** PLG motion stalls at the library-adoption stage because the "install in 2 services within 30 days" success metric is not met. Root cause is usually one of: (a) the library onboarding documentation is unclear, (b) the library requires configuration that is not obvious, or (c) the second service has a different language and the cross-language story is not clearly documented. Fix whichever one it is.

---

### Month 6–12: Growth and repeatability

| Metric | Target | Measurement |
|--------|--------|-------------|
| ARR | $80k–$150k | Billing |
| Paying customers | 5–8 | CRM |
| Avg. contract value | $12k–$20k ARR | ARR / customers |
| Churn rate | < 5% | Churned customers / total |
| Net Revenue Retention | > 110% | Including expansions |
| GitHub stars | 1,000 | GitHub |
| Brain deployments (self-hosted) | 20+ | GitHub releases download count |
| First enterprise conversation | 1 | CRM |
| Press / media mentions | 2+ | Google Alerts, Hacker News |

**Key milestone:** Repeatability. Can the founder run the same sales motion twice and get the same result? If five customers have converted through the same PLG → pilot → conversion path, the motion is repeatable and ready for the first sales hire.

**Key risk:** The brain operational burden (Kafka, TimescaleDB management) exceeds what a single founder can support for 5+ cloud customers simultaneously. This is the point where the hosted brain infrastructure needs to be robust enough to operate with minimal manual intervention. If this isn't resolved before Month 9, cloud customer churn will spike.

---

### Year 2: Scale and enterprise

| Metric | Target | Measurement |
|--------|--------|-------------|
| ARR | $500k–$1M | Billing |
| Paying customers | 20–40 | CRM |
| Enterprise customers (>$60k ARR) | 3–5 | CRM |
| Avg. contract value | $15k–$25k ARR (blended) | ARR / customers |
| Net Revenue Retention | > 120% | Expansions exceeding churn |
| Employees | 3–5 (founder + sales engineer + infrastructure) | Headcount |
| Python library released | Yes | GitHub release |
| Compliance reference customers | 2+ | Case studies published |

**Key milestone:** First enterprise contract above $100k ARR. This validates the top-down sales motion and the compliance story. A single $100k+ ARR enterprise customer de-risks the company's revenue concentration risk and provides the reference story needed for the next 5 enterprise conversations.

**Key risk at Year 2:** Competitive response. If ObservaLog reaches 1,000+ GitHub stars and $500k ARR, Datadog or a well-funded competitor may attempt to replicate the wire format and CI scanner. The moat defense is: (1) the cross-language fingerprint parity is harder to replicate than it appears; (2) the community that has already adopted the wire contract creates switching cost; (3) the compliance reference customers are relationship-dependent, not product-dependent. Invest in the community and the reference customers, not just the product.

---

*ObservaLog GTM Strategy v1.0 — for founder use. Revisit at Month 3, Month 6, and Month 12.*
*https://github.com/darshanredkar11/observalog*
