use super::{adapter, model::CanonicalEvent, queue::{self, WorkItem}};
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error};

type GateFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
type GateProcessor = for<'a> fn(&'a PgPool, &'a WorkItem) -> GateFuture<'a>;

#[derive(Debug, FromRow)]
struct EventRow {
    id: i64,
    event_ts: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    service: Option<String>,
    instance: Option<String>,
    version: Option<String>,
    tenant_id: Option<String>,
    severity: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    parent_span_id: Option<String>,
    request_id: Option<String>,
    business_entity_id: Option<String>,
    outcome: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
struct RelatedEvent {
    id: i64,
    event_ts: DateTime<Utc>,
    observed_at: DateTime<Utc>,
    service: Option<String>,
    instance: Option<String>,
    version: Option<String>,
    tenant_id: Option<String>,
    severity: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    parent_span_id: Option<String>,
    request_id: Option<String>,
    business_entity_id: Option<String>,
    outcome: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
struct Cohort {
    instance: Option<String>,
    version: Option<String>,
    tenant_id: Option<String>,
    total: i64,
    failures: i64,
}

pub async fn run_normalize(pool: PgPool) {
    run_loop(pool, "normalize", normalize_one).await;
}

pub async fn run_quality(pool: PgPool) {
    run_loop(pool, "quality", quality_one).await;
}

pub async fn run_correlation(pool: PgPool) {
    run_loop(pool, "correlate", correlate_one).await;
}

pub async fn run_detection(pool: PgPool) {
    run_loop(pool, "detect", detect_one).await;
}

pub async fn run_reasoning(pool: PgPool) {
    run_loop(pool, "reason", reason_one).await;
}

async fn run_loop(
    pool: PgPool,
    gate: &'static str,
    process: GateProcessor,
) {
    loop {
        match queue::claim(&pool, gate).await {
            Ok(Some(work)) => {
                if let Err(err) = process(&pool, &work).await {
                    error!(gate, work_id=work.work_id, attempts=work.attempts + 1, error=%err, "gate processing failed");
                    queue::fail(&pool, &work, &err).await;
                }
            }
            Ok(None) => sleep(TokioDuration::from_millis(200)).await,
            Err(err) => {
                error!(gate, error=%err, "gate queue read failed");
                sleep(TokioDuration::from_secs(1)).await;
            }
        }
    }
}

fn normalize_one<'a>(pool: &'a PgPool, work: &'a WorkItem) -> GateFuture<'a> {
    Box::pin(async move {
        let observed_at = Utc::now();
        let event = adapter::adapt(&work.raw_text, &work.source_metadata, observed_at);
        let canonical = serde_json::to_value(&event)?;
        let mut tx = pool.begin().await?;

        let payload_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO lite_event_payload
                (raw_id, raw_text, source_metadata, canonical, provenance, quality)
            VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (raw_id) DO UPDATE SET raw_id=EXCLUDED.raw_id
            RETURNING id
            "#
        )
        .bind(&work.raw_id)
        .bind(&work.raw_text)
        .bind(&work.source_metadata)
        .bind(&canonical)
        .bind(&event.provenance)
        .bind(&event.quality)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(source_key) = source_identity_key(&event, &work.source_metadata) {
            let inserted: Option<String> = sqlx::query_scalar(
                "INSERT INTO lite_source_identity (source_key,payload_id) VALUES ($1,$2) ON CONFLICT (source_key) DO NOTHING RETURNING source_key"
            )
            .bind(source_key)
            .bind(payload_id)
            .fetch_optional(&mut *tx)
            .await?;
            if inserted.is_none() {
                tx.rollback().await?;
                queue::complete(pool, work, None, None).await?;
                debug!(work_id=work.work_id, "source event identity already processed; duplicate suppressed");
                return Ok(());
            }
        }

        sqlx::query(
            r#"
            INSERT INTO lite_event_index
                (id,event_ts,observed_at,service,instance,version,environment,region,tenant_id,
                 severity,trace_id,span_id,parent_span_id,request_id,business_entity_id,
                 event_name,outcome,duration_ms,error_code,source_name,quality_score,quality_flags)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,0,$21)
            ON CONFLICT (event_ts,id) DO NOTHING
            "#
        )
        .bind(payload_id)
        .bind(event.event_ts)
        .bind(event.observed_at)
        .bind(&event.service)
        .bind(&event.instance)
        .bind(&event.version)
        .bind(&event.environment)
        .bind(&event.region)
        .bind(&event.tenant_id)
        .bind(&event.severity)
        .bind(&event.trace_id)
        .bind(&event.span_id)
        .bind(&event.parent_span_id)
        .bind(&event.request_id)
        .bind(&event.business_entity_id)
        .bind(&event.event_name)
        .bind(&event.outcome)
        .bind(event.duration_ms)
        .bind(&event.error_code)
        .bind(&event.source_name)
        .bind(&event.quality["flags"])
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE lite_gate_work SET state='done', lease_until=NULL, payload_id=$2, event_ts=$3, last_error=NULL WHERE id=$1")
            .bind(work.work_id).bind(payload_id).bind(event.event_ts).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO lite_gate_work (raw_id,gate_name,payload_id,event_ts) VALUES ($1,'quality',$2,$3) ON CONFLICT (raw_id,gate_name) DO NOTHING")
            .bind(&work.raw_id).bind(payload_id).bind(event.event_ts).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    })
}

fn source_identity_key(event: &CanonicalEvent, metadata: &Value) -> Option<String> {
    let source_id = event.source_event_id.as_ref()?;
    let source = metadata.get("source_path").and_then(Value::as_str)
        .or(event.service.as_deref())
        .or(event.source_name.as_deref())
        .unwrap_or("unknown-source");
    let instance = event.instance.as_deref().unwrap_or("unknown-instance");
    Some(format!("{source}:{instance}:{source_id}"))
}

fn quality_one<'a>(pool: &'a PgPool, work: &'a WorkItem) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let payload_id = work.payload_id.context("quality work has no payload_id")?;
        let (canonical,): (Value,) = sqlx::query_as("SELECT canonical FROM lite_event_payload WHERE id=$1")
            .bind(payload_id).fetch_one(pool).await?;
        let event: CanonicalEvent = serde_json::from_value(canonical)?;
        let mut flags: Vec<String> = event.quality["flags"].as_array()
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();

        if event.span_id.is_none() { flags.push("span_id_missing".into()); }
        if event.parent_span_id.is_none() { flags.push("parent_span_missing".into()); }
        if event.version.is_none() { flags.push("service_version_missing".into()); }
        if event.instance.is_none() { flags.push("service_instance_missing".into()); }
        if event.outcome.is_none() { flags.push("outcome_unknown".into()); }
        if event.duration_ms.is_none() { flags.push("duration_unknown".into()); }

        let mapped = [
            event.service.is_some(), event.severity.is_some(), event.message.is_some(),
            !event.event_ts_inferred, event.trace_id.is_some(), event.request_id.is_some(),
        ].into_iter().filter(|present| *present).count();
        let conflict_count = flags.iter().filter(|flag| flag.starts_with("conflicting_")).count();
        let score = (mapped as f64 / 6.0 - conflict_count as f64 * 0.15).clamp(0.0, 1.0);
        flags.sort();
        flags.dedup();
        let quality = json!({
            "format": event.quality["format"],
            "flags": flags,
            "mapping_confidence": score,
            "event_ts_inferred": event.event_ts_inferred,
            "source_fields_preserved": true
        });

        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE lite_event_payload SET quality=$2 WHERE id=$1")
            .bind(payload_id).bind(&quality).execute(&mut *tx).await?;
        sqlx::query("UPDATE lite_event_index SET quality_score=$2, quality_flags=$3 WHERE id=$1 AND event_ts=$4")
            .bind(payload_id).bind(score).bind(&quality["flags"]).bind(event.event_ts).execute(&mut *tx).await?;
        sqlx::query("UPDATE lite_gate_work SET state='done', lease_until=NULL, event_ts=$2, last_error=NULL WHERE id=$1")
            .bind(work.work_id).bind(event.event_ts).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO lite_gate_work (raw_id,gate_name,payload_id,event_ts) VALUES ($1,'correlate',$2,$3) ON CONFLICT (raw_id,gate_name) DO NOTHING")
            .bind(&work.raw_id).bind(payload_id).bind(event.event_ts).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    })
}

fn correlate_one<'a>(pool: &'a PgPool, work: &'a WorkItem) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let payload_id = work.payload_id.context("correlation work has no payload_id")?;
        let current: EventRow = sqlx::query_as(
            r#"SELECT id,event_ts,observed_at,service,instance,version,tenant_id,
                      severity,trace_id,span_id,parent_span_id,request_id,business_entity_id,outcome,error_code
               FROM lite_event_index WHERE id=$1 AND event_ts=$2 LIMIT 1"#
        ).bind(payload_id).bind(work.event_ts.context("gate work has no event timestamp")?).fetch_one(pool).await?;

        let trace = current.trace_id.clone().filter(|v| !v.starts_with("sys_"));
        let request = current.request_id.clone();
        let entity = current.business_entity_id.clone();
        if trace.is_some() || request.is_some() || entity.is_some() {
            let matches: Vec<RelatedEvent> = sqlx::query_as(
                r#"
                SELECT id,event_ts,observed_at,service,instance,version,tenant_id,severity,trace_id,span_id,parent_span_id,request_id,
                       business_entity_id,outcome,error_code
                FROM lite_event_index
                WHERE id <> $1
                  AND event_ts BETWEEN $2 AND $3
                  AND (($4::text IS NOT NULL AND trace_id=$4)
                    OR ($5::text IS NOT NULL AND request_id=$5)
                    OR ($6::text IS NOT NULL AND business_entity_id=$6))
                ORDER BY event_ts DESC LIMIT 100
                "#
            )
            .bind(payload_id)
            .bind(current.event_ts - Duration::hours(2))
            .bind(current.event_ts + Duration::hours(2))
            .bind(trace)
            .bind(request)
            .bind(entity)
            .fetch_all(pool).await?;

            for other in matches {
                let request_conflict = current.request_id.as_ref().zip(other.request_id.as_ref())
                    .is_some_and(|(left,right)| left != right);
                let entity_conflict = current.business_entity_id.as_ref().zip(other.business_entity_id.as_ref())
                    .is_some_and(|(left,right)| left != right);
                let tenant_conflict = current.tenant_id.as_ref().zip(other.tenant_id.as_ref())
                    .is_some_and(|(left,right)| left != right);
                let same_trace = current.trace_id.as_ref().is_some_and(|v|
                    !v.starts_with("sys_") && Some(v) == other.trace_id.as_ref());

                if same_trace && (request_conflict || entity_conflict || tenant_conflict) {
                    let evidence = json!({
                        "trace_id":current.trace_id,
                        "current_request_id":current.request_id,
                        "other_request_id":other.request_id,
                        "current_entity_id":current.business_entity_id,
                        "other_entity_id":other.business_entity_id,
                        "current_tenant_id":current.tenant_id,
                        "other_tenant_id":other.tenant_id
                    });
                    sqlx::query(
                        "INSERT INTO lite_signals (event_id,event_ts,signal_type,severity,evidence) VALUES ($1,$2,'trace_identity_conflict','high',$3) ON CONFLICT (event_id,signal_type) DO NOTHING"
                    )
                    .bind(payload_id).bind(current.event_ts).bind(evidence).execute(pool).await?;
                }

                let mut relations = Vec::new();
                if same_trace && !request_conflict && !entity_conflict && !tenant_conflict {
                    relations.push(("same_trace", 0.99, json!({"trace_id":&current.trace_id})));
                }
                if current.request_id.is_some() && current.request_id == other.request_id && !tenant_conflict {
                    relations.push(("same_request", 0.98, json!({"request_id":&current.request_id})));
                }
                if current.business_entity_id.is_some() && current.business_entity_id == other.business_entity_id && !tenant_conflict {
                    relations.push(("same_business_entity", 0.94, json!({"business_entity_id":&current.business_entity_id})));
                }

                let same_flow = (same_trace && !request_conflict && !entity_conflict && !tenant_conflict)
                    || (current.request_id.is_some() && current.request_id == other.request_id && !tenant_conflict);
                if same_flow {
                    if current.parent_span_id.is_some() && current.parent_span_id == other.span_id {
                        relations.push(("parent_child", 0.995, json!({
                            "parent_event_id":other.id,"child_event_id":current.id,
                            "parent_span_id":other.span_id,"child_span_id":current.span_id
                        })));
                    }
                    if other.parent_span_id.is_some() && other.parent_span_id == current.span_id {
                        relations.push(("parent_child", 0.995, json!({
                            "parent_event_id":current.id,"child_event_id":other.id,
                            "parent_span_id":current.span_id,"child_span_id":other.span_id
                        })));
                    }
                }

                for (relation, confidence, evidence) in relations {
                    let (a,b) = if payload_id < other.id {(payload_id,other.id)} else {(other.id,payload_id)};
                    sqlx::query(
                        "INSERT INTO lite_event_links (event_a,event_b,relation_type,confidence,evidence) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
                    )
                    .bind(a).bind(b).bind(relation).bind(confidence).bind(evidence)
                    .execute(pool).await?;
                }
            }
        }

        queue::complete(pool, work, Some("detect"), Some(payload_id)).await?;
        Ok(())
    })
}

fn detect_one<'a>(pool: &'a PgPool, work: &'a WorkItem) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let payload_id = work.payload_id.context("detection work has no payload_id")?;
        let (canonical,): (Value,) = sqlx::query_as("SELECT canonical FROM lite_event_payload WHERE id=$1")
            .bind(payload_id).fetch_one(pool).await?;
        let event: CanonicalEvent = serde_json::from_value(canonical)?;
        let mut signals: Vec<(&str,&str,Value)> = Vec::new();

        let severity = event.severity.as_deref().unwrap_or("");
        if matches!(severity, "ERROR" | "FATAL") {
            signals.push(("error_event","high",json!({"severity":severity,"error_code":event.error_code.as_deref()})));
        }
        if event.outcome.as_deref() == Some("failure") {
            signals.push(("business_or_operation_failure","high",json!({"outcome":event.outcome.as_deref()})));
        }
        if event.error_code.is_some() || event.error_kind.is_some() || event.error_message.is_some() {
            signals.push(("error_payload_present","medium",json!({
                "error_code":event.error_code.as_deref(),
                "error_kind":event.error_kind.as_deref(),
                "severity":severity
            })));
            if matches!(severity, "INFO" | "DEBUG" | "TRACE") {
                signals.push(("error_payload_level_mismatch","medium",json!({
                    "error_code":event.error_code.as_deref(),
                    "severity":severity
                })));
            }
        }
        let threshold = std::env::var("LITE_SLOW_THRESHOLD_MS").ok()
            .and_then(|s| s.parse::<f64>().ok()).unwrap_or(1000.0);
        if event.duration_ms.is_some_and(|value| value >= threshold) {
            signals.push(("slow_operation","medium",json!({"duration_ms":event.duration_ms,"threshold_ms":threshold})));
        }
        if contains_retry_marker(&event.attributes) || event.message.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains("retry")) {
            signals.push(("retry_activity","medium",json!({"message":event.message.as_deref()})));
        }
        let code = event.error_code.as_deref().unwrap_or("").to_ascii_uppercase();
        if code.contains("DUPLICATE") || code.contains("IDEMPOTENCY") || code.contains("STATE_MISMATCH") || code.contains("INVARIANT") {
            signals.push(("business_invariant_or_idempotency","high",json!({"error_code":event.error_code.as_deref()})));
        }

        let mut tx = pool.begin().await?;
        for (kind, level, evidence) in &signals {
            sqlx::query(
                "INSERT INTO lite_signals (event_id,event_ts,signal_type,severity,evidence) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (event_id,signal_type) DO NOTHING"
            )
            .bind(payload_id).bind(event.event_ts).bind(kind).bind(level).bind(evidence)
            .execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE lite_gate_work SET state='done', lease_until=NULL, event_ts=$2, last_error=NULL WHERE id=$1")
            .bind(work.work_id).bind(event.event_ts).execute(&mut *tx).await?;
        let existing_signal: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM lite_signals WHERE event_id=$1)"
        ).bind(payload_id).fetch_one(&mut *tx).await?;
        if !signals.is_empty() || existing_signal {
            sqlx::query("INSERT INTO lite_gate_work (raw_id,gate_name,payload_id,event_ts) VALUES ($1,'reason',$2,$3) ON CONFLICT (raw_id,gate_name) DO NOTHING")
                .bind(&work.raw_id).bind(payload_id).bind(event.event_ts).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    })
}

fn contains_retry_marker(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            (key.contains("retry") || key == "attempt" || key == "delivery_attempt")
                || contains_retry_marker(value)
        }),
        Value::Array(values) => values.iter().any(contains_retry_marker),
        _ => false,
    }
}

fn reason_one<'a>(pool: &'a PgPool, work: &'a WorkItem) -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let payload_id = work.payload_id.context("reasoning work has no payload_id")?;
        let current: EventRow = sqlx::query_as(
            r#"SELECT id,event_ts,service,instance,version,tenant_id,
                      severity,trace_id,request_id,business_entity_id,outcome,error_code
               FROM lite_event_index WHERE id=$1 AND event_ts=$2 LIMIT 1"#
        ).bind(payload_id).bind(work.event_ts.context("gate work has no event timestamp")?).fetch_one(pool).await?;

        let related: Vec<RelatedEvent> = sqlx::query_as(
            r#"
            SELECT DISTINCT e.id,e.event_ts,e.observed_at,e.service,e.instance,e.version,e.tenant_id,e.severity,
                   e.trace_id,e.span_id,e.parent_span_id,e.request_id,e.business_entity_id,e.outcome,e.error_code
            FROM lite_event_index e
            LEFT JOIN lite_event_links l
              ON (l.event_a=$1 AND l.event_b=e.id) OR (l.event_b=$1 AND l.event_a=e.id)
            WHERE (e.id=$1 OR l.event_a IS NOT NULL)
              AND e.event_ts BETWEEN $2 - INTERVAL '2 hours' AND $2 + INTERVAL '2 hours'
            ORDER BY e.event_ts ASC LIMIT 100
            "#
        ).bind(payload_id).bind(work.event_ts.context("gate work has no event timestamp")?).fetch_all(pool).await?;

        let cohorts: Vec<Cohort> = if let Some(service) = current.service.as_deref() {
            sqlx::query_as(
                r#"
                SELECT instance,version,tenant_id,COUNT(*) AS total,
                       COUNT(*) FILTER (WHERE severity IN ('ERROR','FATAL') OR outcome='failure') AS failures
                FROM lite_event_index
                WHERE service=$1 AND event_ts BETWEEN $2 - INTERVAL '5 minutes' AND $2 + INTERVAL '5 minutes'
                GROUP BY instance,version,tenant_id ORDER BY total DESC LIMIT 100
                "#
            ).bind(service).bind(current.event_ts).fetch_all(pool).await?
        } else { Vec::new() };

        let (mut category, mut cause, mut confidence) = classify_hypothesis(
            current.error_code.as_deref(),
            current.service.as_deref(),
            current.severity.as_deref(),
            current.outcome.as_deref(),
        );
        let mut alternatives = Vec::new();
        let mut cohort_reason = None;

        if let (Some(instance), Some(version)) = (current.instance.as_deref(), current.version.as_deref()) {
            let current_failures: i64 = cohorts.iter().filter(|c| c.instance.as_deref()==Some(instance) && c.version.as_deref()==Some(version)).map(|c| c.failures).sum();
            let current_total: i64 = cohorts.iter().filter(|c| c.instance.as_deref()==Some(instance) && c.version.as_deref()==Some(version)).map(|c| c.total).sum();
            let peer_success: i64 = cohorts.iter().filter(|c| c.instance.as_deref()!=Some(instance) && c.version.as_deref()==Some(version)).map(|c| c.total-c.failures).sum();
            if current_failures > 0 && peer_success > 0 && current_total > 0 {
                category = "instance_or_configuration_drift".into();
                cause = format!("Failures are concentrated on instance {instance} while peers running version {version} show successful events; inspect instance-specific configuration and runtime state.");
                confidence = confidence.max(0.78);
                cohort_reason = Some("same-version instance comparison shows a failing instance and healthy peers");
            }
        }

        if let Some(tenant) = current.tenant_id.as_deref() {
            let tenant_failures: i64 = cohorts.iter().filter(|c| c.tenant_id.as_deref()==Some(tenant)).map(|c| c.failures).sum();
            let other_success: i64 = cohorts.iter().filter(|c| c.tenant_id.as_deref()!=Some(tenant)).map(|c| c.total-c.failures).sum();
            if tenant_failures > 0 && other_success > 0 {
                category = "tenant_scoped_configuration_or_behavior".into();
                cause = format!("Failures are concentrated in tenant {tenant} while other tenants have successful events; compare tenant-specific flags, policies and configuration.");
                confidence = confidence.max(0.78);
                cohort_reason = Some("tenant cohort comparison reveals a healthy control group");
            }
        }

        if let Some(version) = current.version.as_deref() {
            let version_failures: i64 = cohorts.iter().filter(|c| c.version.as_deref()==Some(version)).map(|c| c.failures).sum();
            let other_version_success: i64 = cohorts.iter().filter(|c| c.version.as_deref()!=Some(version)).map(|c| c.total-c.failures).sum();
            if version_failures > 0 && other_version_success > 0 {
                category = "version_correlated_regression".into();
                cause = format!("Failures are associated with version {version}, while other versions have successful events; compare deployment changes and the affected code path.");
                confidence = confidence.max(0.76);
                cohort_reason = Some("version cohort comparison shows different outcomes across versions");
            }
        }

        let fallback_event = RelatedEvent {
            id:current.id,event_ts:current.event_ts,observed_at:current.observed_at,service:current.service.clone(),instance:current.instance.clone(),
            version:current.version.clone(),tenant_id:current.tenant_id.clone(),severity:current.severity.clone(),
            trace_id:current.trace_id.clone(),span_id:current.span_id.clone(),parent_span_id:current.parent_span_id.clone(),request_id:current.request_id.clone(),
            business_entity_id:current.business_entity_id.clone(),outcome:current.outcome.clone(),error_code:current.error_code.clone()
        };
        let primary_error = related.iter()
            .filter(|e| matches!(e.severity.as_deref(),Some("ERROR"|"FATAL")) || e.outcome.as_deref()==Some("failure"))
            .min_by_key(|e| e.event_ts)
            .unwrap_or_else(|| related.first().unwrap_or(&fallback_event));
        let evidence_ids: Vec<i64> = related.iter().map(|e| e.id).collect();
        let mut services: Vec<String> = related.iter().filter_map(|e| e.service.clone()).collect();
        if let Some(service) = current.service.clone() { services.push(service); }
        services.sort(); services.dedup();

        if related.len() > 1 && primary_error.id != current.id {
            alternatives.push(json!({"category":"downstream_symptom","event_id":current.id,"note":"An earlier correlated failure may be the initiating signal; temporal order alone is not proof of causality."}));
        }

        let primary_event_id = primary_error.id;
        let evidence = json!({
            "primary_event_id":primary_event_id,
            "trigger_event_id":current.id,
            "related_events":related,
            "cohort_comparison":cohorts,
            "cohort_interpretation":cohort_reason,
            "correlation_is_causal":false,
            "limitations":["This deterministic first-pass finding is a hypothesis, not an automated remediation decision.",
                "Temporal ordering is not sufficient to prove causality."]
        });
        let bucket = current.event_ts.timestamp().div_euclid(300);
        let incident_key = format!("{}:{}:{}:{}", category, current.service.as_deref().unwrap_or("unknown"),
            current.error_code.as_deref().unwrap_or("no-code"), bucket);
        let title = format!("{} signal in {}", category.replace('_'," "), current.service.as_deref().unwrap_or("unidentified service"));
        let affected = json!(services);
        let evidence_ids = json!(evidence_ids);

        let mut tx = pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO lite_findings
                (incident_key,category,title,likely_cause,confidence,primary_event_id,
                 evidence_event_ids,affected_services,alternatives,evidence)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (incident_key) DO UPDATE SET
                last_seen=now(),
                occurrence_count=lite_findings.occurrence_count+1,
                confidence=GREATEST(lite_findings.confidence,EXCLUDED.confidence),
                evidence=EXCLUDED.evidence,
                evidence_event_ids=EXCLUDED.evidence_event_ids,
                affected_services=EXCLUDED.affected_services,
                alternatives=EXCLUDED.alternatives
            "#
        )
        .bind(&incident_key).bind(&category).bind(&title).bind(&cause).bind(confidence)
        .bind(primary_event_id).bind(evidence_ids).bind(affected).bind(json!(alternatives)).bind(evidence)
        .execute(&mut *tx).await?;

        sqlx::query("UPDATE lite_gate_work SET state='done', lease_until=NULL, last_error=NULL WHERE id=$1")
            .bind(work.work_id).execute(&mut *tx).await?;
        tx.commit().await?;
        debug!(work_id=work.work_id, incident_key, category, confidence, "evidence-backed finding updated");
        Ok(())
    })
}

fn classify_hypothesis(error_code: Option<&str>, service: Option<&str>, severity: Option<&str>, outcome: Option<&str>) -> (String,String,f64) {
    let code = error_code.unwrap_or("").to_ascii_uppercase();
    let service = service.unwrap_or("unknown");
    if ["JWT","OIDC","ISSUER","AUDIENCE","TOKEN","CREDENTIAL"].iter().any(|v|code.contains(v)) {
        ("authentication_or_identity_configuration".into(),format!("Identity validation failed in {service}; compare issuer, audience, credential and clock configuration."),0.78)
    } else if ["CONFIG","FLAG","SECRET","SETTING","ENV_MISSING"].iter().any(|v|code.contains(v)) {
        ("configuration".into(),format!("Configuration-related failure reported by {service}; compare effective configuration across instances and environments."),0.72)
    } else if ["DEADLOCK","SQL","DB_","DATABASE","CONNECTION_POOL","40P01","40001"].iter().any(|v|code.contains(v)) {
        ("database_or_concurrency".into(),format!("Database or concurrency failure reported by {service}; inspect transaction ordering, lock waits and pool utilization."),0.74)
    } else if ["429","503","504","TIMEOUT","DNS","EAI_AGAIN","UPSTREAM","RATE_LIMIT"].iter().any(|v|code.contains(v)) {
        ("external_dependency_or_network".into(),format!("Dependency/network failure reported by {service}; inspect upstream health, response codes, latency and retry policy."),0.76)
    } else if ["OOM","MEMORY","RESOURCE","POOL_EXHAUSTED","TOO_MANY"].iter().any(|v|code.contains(v)) {
        ("resource_exhaustion".into(),format!("Resource pressure reported by {service}; correlate heap, CPU, threads, sockets and queue depth."),0.76)
    } else if ["SCHEMA","TYPE_MISMATCH","DESERIALIZE","SERDE","CONTRACT"].iter().any(|v|code.contains(v)) {
        ("data_or_schema_contract".into(),format!("Data contract incompatibility is indicated in {service}; compare producer and consumer field types and schema versions."),0.78)
    } else if ["DUPLICATE","IDEMPOTENCY","REPLAY"].iter().any(|v|code.contains(v)) {
        ("async_delivery_or_idempotency".into(),format!("Duplicate processing or idempotency behavior is implicated in {service}; compare message IDs, delivery attempts and side effects."),0.78)
    } else if ["STATE_MISMATCH","INVARIANT","BUSINESS_RULE","LEDGER"].iter().any(|v|code.contains(v)) {
        ("business_logic_or_consistency".into(),format!("A business-state invariant appears violated in {service}; compare persisted state transitions with external side effects."),0.78)
    } else if severity==Some("ERROR") || severity==Some("FATAL") || outcome==Some("failure") {
        ("application_failure".into(),format!("{service} emitted a failure signal, but the root-cause category is not yet established."),0.48)
    } else {
        ("insufficient_evidence".into(),format!("A signal was emitted by {service}, but available fields do not support a specific cause."),0.25)
    }
}
