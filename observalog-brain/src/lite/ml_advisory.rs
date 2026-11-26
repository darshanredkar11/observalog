//! Gate 5.1 advisory ML boundary and bounded shadow scoring.
//! Gate 5 remains authoritative; this module receives aggregates only.
//! Tenant scoping is enforced by the host before provider invocation.

use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, task::JoinError, time::timeout};

pub const FEATURE_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_MAX_EVENTS_PER_WINDOW: usize = 4096;
pub const DEFAULT_MAX_DURATION_SAMPLES: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedWindow {
    /// Opaque host-assigned tenant partition token. Never used for provider lookups.
    pub tenant_partition: String,
    /// Opaque service grouping, set by the host; no raw identifiers.
    pub service_partition: String,
    pub window_start_epoch_ms: i64,
    pub window_end_epoch_ms: i64,
    pub feature_schema_version: u16,
    pub event_count: u64,
    pub error_count: u64,
    pub failure_outcome_count: u64,
    pub retry_count: u64,
    pub slow_count: u64,
    pub invariant_violation_count: u64,
    pub independent_trace_count: u64,
    pub error_rate: f64,
    pub failure_rate: f64,
    pub retry_rate: f64,
    pub duration_mean_ms: Option<f64>,
    pub duration_p50_ms: Option<f64>,
    pub duration_p95_ms: Option<f64>,
    pub duration_p99_ms: Option<f64>,
    pub cpu_utilization: Option<f64>,
    pub memory_utilization: Option<f64>,
    pub queue_depth: Option<u64>,
    pub queue_depth_delta: Option<i64>,
    pub completeness_ratio: f64,
    pub dropped_or_missing_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AggregateEvent {
    pub tenant_partition: String,
    pub service_partition: String,
    pub event_ts_epoch_ms: i64,
    pub is_error: bool,
    pub is_failure: bool,
    pub is_retry: bool,
    pub is_slow: bool,
    pub is_invariant_violation: bool,
    pub trace_partition: Option<String>,
    pub duration_ms: Option<f64>,
    pub cpu_utilization: Option<f64>,
    pub memory_utilization: Option<f64>,
    pub queue_depth: Option<u64>,
    pub completeness_ratio: f64,
    pub dropped_or_missing_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AbstentionReason {
    ProviderDisabled,
    InsufficientBaseline,
    IncompleteWindow,
    UnsupportedFeatureSchema,
    ProviderUnavailable,
    InvalidScore,
    CapacityLimit,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryScore {
    pub provider_id: String,
    pub model_version: String,
    pub feature_schema_version: u16,
    /// Advisory score only; not a root-cause probability.
    pub anomaly_score: Option<f64>,
    pub abstention: Option<AbstentionReason>,
    pub feature_contributions: Vec<FeatureContribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureContribution {
    pub feature_name: String,
    pub contribution: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderError {
    pub provider_id: String,
    pub code: String,
    pub retryable: bool,
}

pub trait AdvisoryProvider: Send + Sync + 'static {
    fn provider_id(&self) -> &'static str;
    fn model_version(&self) -> &'static str;
    fn score(&self, window: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError>;
}

/// Bounded in-memory window accumulator. One instance is scoped to one tenant and
/// service partition. Rejects cross-partition events and out-of-window inputs.
/// It stores only aggregate counters and a capped duration sample.
pub struct WindowAccumulator {
    tenant_partition: String,
    service_partition: String,
    start_ms: i64,
    end_ms: i64,
    max_events: usize,
    max_duration_samples: usize,
    seen: usize,
    errors: u64,
    failures: u64,
    retries: u64,
    slow: u64,
    invariants: u64,
    traces: std::collections::HashSet<String>,
    durations: Vec<f64>,
    duration_sum: f64,
    duration_count: u64,
    duration_sample_seen: u64,
    sample_rng_state: u64,
    cpu_sum: f64,
    cpu_count: u64,
    memory_sum: f64,
    memory_count: u64,
    latest_queue: Option<u64>,
    initial_queue: Option<u64>,
    completeness_sum: f64,
    missing_sum: u64,
}

impl WindowAccumulator {
    pub fn new(tenant: impl Into<String>, service: impl Into<String>, start_ms: i64, end_ms: i64) -> Result<Self, &'static str> {
        Self::with_limits(tenant, service, start_ms, end_ms, DEFAULT_MAX_EVENTS_PER_WINDOW, DEFAULT_MAX_DURATION_SAMPLES)
    }

    pub fn with_limits(tenant: impl Into<String>, service: impl Into<String>, start_ms: i64, end_ms: i64, max_events: usize, max_duration_samples: usize) -> Result<Self, &'static str> {
        let tenant = tenant.into();
        let service = service.into();
        if tenant.trim().is_empty() || service.trim().is_empty() { return Err("partition_required"); }
        if end_ms <= start_ms { return Err("invalid_window_bounds"); }
        if max_events == 0 || max_duration_samples == 0 { return Err("limits_must_be_nonzero"); }
        Ok(Self {
            tenant_partition: tenant, service_partition: service, start_ms, end_ms,
            max_events, max_duration_samples, seen: 0, errors: 0, failures: 0,
            retries: 0, slow: 0, invariants: 0, traces: std::collections::HashSet::new(),
            durations: Vec::with_capacity(max_duration_samples.min(512)),
            duration_sum: 0.0, duration_count: 0, duration_sample_seen: 0, sample_rng_state: 0x9E3779B97F4A7C15,
            cpu_sum: 0.0, cpu_count: 0, memory_sum: 0.0, memory_count: 0,
            latest_queue: None, initial_queue: None, completeness_sum: 0.0, missing_sum: 0,
        })
    }

    pub fn push(&mut self, e: AggregateEvent) -> Result<(), &'static str> {
        if e.tenant_partition != self.tenant_partition || e.service_partition != self.service_partition { return Err("partition_mismatch"); }
        if e.event_ts_epoch_ms < self.start_ms || e.event_ts_epoch_ms >= self.end_ms { return Err("event_outside_window"); }
        if self.seen >= self.max_events { return Err("window_capacity_exceeded"); }
        if !ratio(e.completeness_ratio) { return Err("invalid_completeness_ratio"); }
        if e.duration_ms.is_some_and(|d| !d.is_finite() || d < 0.0) { return Err("invalid_duration"); }
        if e.cpu_utilization.is_some_and(|v| !ratio(v)) || e.memory_utilization.is_some_and(|v| !ratio(v)) { return Err("invalid_resource_ratio"); }
        self.seen += 1;
        self.errors += if e.is_error { 1 } else { 0 };
        self.failures += if e.is_failure { 1 } else { 0 };
        self.retries += if e.is_retry { 1 } else { 0 };
        self.slow += if e.is_slow { 1 } else { 0 };
        self.invariants += if e.is_invariant_violation { 1 } else { 0 };
        if let Some(t) = e.trace_partition { self.traces.insert(t); }
        if let Some(d) = e.duration_ms {
            self.duration_sum += d; self.duration_count += 1;
            self.duration_sample_seen += 1;
            if self.durations.len() < self.max_duration_samples { self.durations.push(d); }
            else {
                // Deterministic reservoir sampling: bounded memory without first-N bias.
                self.sample_rng_state ^= self.sample_rng_state << 13;
                self.sample_rng_state ^= self.sample_rng_state >> 7;
                self.sample_rng_state ^= self.sample_rng_state << 17;
                let slot = self.sample_rng_state % self.duration_sample_seen;
                if slot < self.max_duration_samples as u64 { self.durations[slot as usize] = d; }
            }
        }
        if let Some(c) = e.cpu_utilization { self.cpu_sum += c; self.cpu_count += 1; }
        if let Some(m) = e.memory_utilization { self.memory_sum += m; self.memory_count += 1; }
        if let Some(q) = e.queue_depth { self.initial_queue.get_or_insert(q); self.latest_queue = Some(q); }
        self.completeness_sum += e.completeness_ratio;
        self.missing_sum += e.dropped_or_missing_count.unwrap_or(0);
        Ok(())
    }

    pub fn build(self) -> AggregatedWindow {
        let count = self.seen as f64;
        let mut samples = self.durations;
        samples.sort_by(f64::total_cmp);
        let mean = (self.duration_count > 0).then(|| self.duration_sum / self.duration_count as f64);
        let percentile = |p: f64| if samples.is_empty() { None } else {
            let idx = ((samples.len() - 1) as f64 * p).round() as usize;
            Some(samples[idx])
        };
        let queue_delta = self.initial_queue.zip(self.latest_queue).map(|(a,b)| b as i64 - a as i64);
        AggregatedWindow {
            tenant_partition: self.tenant_partition,
            service_partition: self.service_partition,
            window_start_epoch_ms: self.start_ms,
            window_end_epoch_ms: self.end_ms,
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            event_count: self.seen as u64, error_count: self.errors,
            failure_outcome_count: self.failures, retry_count: self.retries,
            slow_count: self.slow, invariant_violation_count: self.invariants,
            independent_trace_count: self.traces.len() as u64,
            error_rate: if count == 0.0 { 0.0 } else { self.errors as f64 / count },
            failure_rate: if count == 0.0 { 0.0 } else { self.failures as f64 / count },
            retry_rate: if count == 0.0 { 0.0 } else { self.retries as f64 / count },
            duration_mean_ms: mean, duration_p50_ms: percentile(0.50), duration_p95_ms: percentile(0.95), duration_p99_ms: percentile(0.99),
            cpu_utilization: (self.cpu_count > 0).then(|| self.cpu_sum / self.cpu_count as f64),
            memory_utilization: (self.memory_count > 0).then(|| self.memory_sum / self.memory_count as f64),
            queue_depth: self.latest_queue, queue_depth_delta: queue_delta,
            completeness_ratio: if count == 0.0 { 0.0 } else { self.completeness_sum / count },
            dropped_or_missing_count: Some(self.missing_sum),
        }
    }
}

pub struct ColdStartProvider;
impl AdvisoryProvider for ColdStartProvider {
    fn provider_id(&self) -> &'static str { "cold-start-abstainer" }
    fn model_version(&self) -> &'static str { "cold-0" }
    fn score(&self, window: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError> {
        Ok(AdvisoryScore {
            provider_id: self.provider_id().into(), model_version: self.model_version().into(),
            feature_schema_version: window.feature_schema_version, anomaly_score: None,
            abstention: Some(AbstentionReason::InsufficientBaseline), feature_contributions: vec![],
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShadowOutcome {
    Disabled,
    CapacitySkipped,
    TimedOut,
    ProviderFailed(String),
    InvalidResult(&'static str),
    Completed(AdvisoryScore),
}

/// Optional, fail-open provider execution. Gate 5 callers must not await this
/// inline; it is bounded and advisory. Semaphore capacity and timeout are explicit.
pub struct ShadowRunner {
    enabled: bool,
    provider: Arc<dyn AdvisoryProvider>,
    slots: Arc<Semaphore>,
    timeout: Duration,
}
impl ShadowRunner {
    pub fn new(enabled: bool, provider: Arc<dyn AdvisoryProvider>, max_concurrency: usize, timeout: Duration) -> Result<Self, &'static str> {
        if max_concurrency == 0 || timeout.is_zero() { return Err("limits_must_be_nonzero"); }
        Ok(Self { enabled, provider, slots: Arc::new(Semaphore::new(max_concurrency)), timeout })
    }

    pub async fn score(&self, window: AggregatedWindow) -> ShadowOutcome {
        if !self.enabled { return ShadowOutcome::Disabled; }
        if let Err(e) = validate_window(&window) { return ShadowOutcome::InvalidResult(e); }
        let permit = match self.slots.clone().try_acquire_owned() {
            Ok(p) => p, Err(_) => return ShadowOutcome::CapacitySkipped,
        };
        let provider = self.provider.clone();
        let fut = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            provider.score(&window)
        });
        match timeout(self.timeout, fut).await {
            Err(_) => ShadowOutcome::TimedOut,
            Ok(Err(join)) => ShadowOutcome::ProviderFailed(join_error_code(join)),
            Ok(Ok(Err(e))) => ShadowOutcome::ProviderFailed(e.code),
            Ok(Ok(Ok(score))) => match validate_score(&score) {
                Ok(()) => ShadowOutcome::Completed(score),
                Err(e) => ShadowOutcome::InvalidResult(e),
            }
        }
    }
}

fn join_error_code(err: JoinError) -> String {
    if err.is_panic() { "provider_panicked".into() } else { "provider_join_failed".into() }
}

pub fn validate_window(w: &AggregatedWindow) -> Result<(), &'static str> {
    if w.feature_schema_version != FEATURE_SCHEMA_VERSION { return Err("unsupported_feature_schema"); }
    if w.tenant_partition.trim().is_empty() || w.service_partition.trim().is_empty() { return Err("partition_required"); }
    if w.window_end_epoch_ms <= w.window_start_epoch_ms { return Err("invalid_window_bounds"); }
    if !ratio(w.error_rate) || !ratio(w.failure_rate) || !ratio(w.retry_rate) || !ratio(w.completeness_ratio) { return Err("invalid_ratio"); }
    for x in [w.duration_mean_ms, w.duration_p50_ms, w.duration_p95_ms, w.duration_p99_ms].into_iter().flatten() {
        if !x.is_finite() || x < 0.0 { return Err("invalid_duration"); }
    }
    for x in [w.cpu_utilization, w.memory_utilization].into_iter().flatten() { if !ratio(x) { return Err("invalid_utilization"); } }
    Ok(())
}
pub fn validate_score(s: &AdvisoryScore) -> Result<(), &'static str> {
    match s.anomaly_score {
        Some(v) if !v.is_finite() || !(0.0..=1.0).contains(&v) => Err("score_out_of_range"),
        Some(_) if s.abstention.is_some() => Err("score_and_abstention_conflict"),
        None if s.abstention.is_none() => Err("abstention_reason_required"),
        _ => Ok(()),
    }
}
fn ratio(v: f64) -> bool { v.is_finite() && (0.0..=1.0).contains(&v) }

const SHADOW_WINDOW_QUERY: &str = r#"
            WITH recent AS (
                SELECT id,event_ts,tenant_id,service,severity,outcome,trace_id,duration_ms,quality_score
                FROM lite_event_index
                WHERE event_ts >= NOW() - INTERVAL '5 minutes'
                  AND tenant_id IS NOT NULL AND service IS NOT NULL
            ), signal_agg AS (
                SELECT r.tenant_id,r.service,
                    COUNT(*) FILTER (WHERE s.signal_type='retry_activity') AS retries,
                    COUNT(*) FILTER (WHERE s.signal_type='slow_operation') AS slow,
                    COUNT(*) FILTER (WHERE s.signal_type='business_invariant_or_idempotency') AS invariants
                FROM recent r JOIN lite_signals s ON s.event_id=r.id AND s.event_ts=r.event_ts
                GROUP BY r.tenant_id,r.service
            )
            SELECT r.tenant_id,r.service,COUNT(*) AS event_count,
                COUNT(*) FILTER (WHERE r.severity IN ('ERROR','FATAL')) AS error_count,
                COUNT(*) FILTER (WHERE r.outcome='failure') AS failure_count,
                COUNT(DISTINCT r.trace_id) FILTER (WHERE r.trace_id IS NOT NULL) AS trace_count,
                COALESCE(sa.retries,0) AS retry_count, COALESCE(sa.slow,0) AS slow_count,
                COALESCE(sa.invariants,0) AS invariant_count,
                AVG(r.duration_ms) AS duration_mean_ms,
                percentile_cont(0.50) WITHIN GROUP (ORDER BY r.duration_ms) AS duration_p50_ms,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY r.duration_ms) AS duration_p95_ms,
                percentile_cont(0.99) WITHIN GROUP (ORDER BY r.duration_ms) AS duration_p99_ms,
                AVG(r.quality_score) AS mapping_completeness,
                COUNT(*) FILTER (WHERE r.quality_score < 0.5) AS missing_count
            FROM recent r LEFT JOIN signal_agg sa ON sa.tenant_id=r.tenant_id AND sa.service=r.service
            GROUP BY r.tenant_id,r.service,sa.retries,sa.slow,sa.invariants
            ORDER BY COUNT(*) DESC LIMIT 100
        "#;

#[derive(Debug, sqlx::FromRow)]
struct DbWindowRow {
    tenant_id: String, service: String, event_count: i64, error_count: i64,
    failure_count: i64, trace_count: i64, retry_count: i64, slow_count: i64,
    invariant_count: i64, duration_mean_ms: Option<f64>, duration_p50_ms: Option<f64>,
    duration_p95_ms: Option<f64>, duration_p99_ms: Option<f64>,
    mapping_completeness: Option<f64>, missing_count: i64,
}

/// Optional five-minute SQL aggregate shadow worker. Disabled by default.
/// It queries aggregate rows only, tokenizes tenant/service scope before provider
/// invocation, and never writes into Gate 5 tables.
pub async fn run_shadow_worker(pool: sqlx::PgPool) {
    let enabled = std::env::var("LITE_ML_SHADOW_ENABLED").map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);
    let runner = match ShadowRunner::new(enabled, Arc::new(ColdStartProvider), 4, Duration::from_millis(500)) {
        Ok(r) => Arc::new(r), Err(_) => return,
    };
    let mut tenant_tokens: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut service_tokens: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    loop {
        if !enabled { tokio::time::sleep(Duration::from_secs(30)).await; continue; }
        let start_time = chrono::Utc::now();
        let query = SHADOW_WINDOW_QUERY;

        match tokio::time::timeout(Duration::from_secs(2), sqlx::query_as::<_, DbWindowRow>(query).fetch_all(&pool)).await {
            Ok(Ok(rows)) => for row in rows {
                if !tenant_tokens.contains_key(&row.tenant_id) && tenant_tokens.len() >= 1000 { tenant_tokens.clear(); }
                let tenant_token = tenant_tokens.entry(row.tenant_id).or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
                let service_key = format!("{}:{}", tenant_token, row.service);
                if !service_tokens.contains_key(&service_key) && service_tokens.len() >= 1000 { service_tokens.clear(); }
                let service_token = service_tokens.entry(service_key).or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
                let total = row.event_count.max(0) as u64;
                let start_ms = start_time.timestamp_millis() - 300_000;
                let end_ms = start_time.timestamp_millis();
                let window = AggregatedWindow {
                    tenant_partition: tenant_token, service_partition: service_token,
                    window_start_epoch_ms: start_ms, window_end_epoch_ms: end_ms,
                    feature_schema_version: FEATURE_SCHEMA_VERSION,
                    event_count: total, error_count: row.error_count.max(0) as u64,
                    failure_outcome_count: row.failure_count.max(0) as u64,
                    retry_count: row.retry_count.max(0) as u64, slow_count: row.slow_count.max(0) as u64,
                    invariant_violation_count: row.invariant_count.max(0) as u64,
                    independent_trace_count: row.trace_count.max(0) as u64,
                    error_rate: if total == 0 { 0.0 } else { row.error_count.max(0) as f64 / total as f64 },
                    failure_rate: if total == 0 { 0.0 } else { row.failure_count.max(0) as f64 / total as f64 },
                    retry_rate: if total == 0 { 0.0 } else { row.retry_count.max(0) as f64 / total as f64 },
                    duration_mean_ms: row.duration_mean_ms, duration_p50_ms: row.duration_p50_ms,
                    duration_p95_ms: row.duration_p95_ms, duration_p99_ms: row.duration_p99_ms,
                    cpu_utilization: None, memory_utilization: None, queue_depth: None, queue_depth_delta: None,
                    // Mapping quality is a proxy; no telemetry completeness is claimed.
                    completeness_ratio: row.mapping_completeness.unwrap_or(0.0).clamp(0.0,1.0),
                    dropped_or_missing_count: Some(row.missing_count.max(0) as u64),
                };
                match runner.score(window).await {
                    ShadowOutcome::Completed(score) => tracing::debug!(provider=%score.provider_id, model=%score.model_version, "Gate 5.1 shadow score"),
                    other => tracing::debug!(outcome=?other, "Gate 5.1 shadow outcome"),
                }
            },
            Ok(Err(err)) => tracing::warn!(error=%err, "Gate 5.1 aggregate query failed; Gate 5 remains unaffected"),
            Err(_) => tracing::warn!("Gate 5.1 aggregate query timed out; Gate 5 remains unaffected"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(ts: i64) -> AggregateEvent {
        AggregateEvent {
            tenant_partition: "tenant-a".into(), service_partition: "payments".into(), event_ts_epoch_ms: ts,
            is_error: false, is_failure: false, is_retry: false, is_slow: false, is_invariant_violation: false,
            trace_partition: None, duration_ms: Some(10.0), cpu_utilization: Some(0.5), memory_utilization: Some(0.6),
            queue_depth: Some(4), completeness_ratio: 1.0, dropped_or_missing_count: Some(0),
        }
    }
    fn window() -> AggregatedWindow { WindowAccumulator::new("tenant-a","payments",0,1000).unwrap().build() }

    #[test] fn empty_window_is_valid_but_incomplete() { let w=window(); assert_eq!(w.event_count,0); assert_eq!(w.completeness_ratio,0.0); assert_eq!(validate_window(&w),Ok(())); }
    #[test] fn rejects_cross_tenant_and_service() { let mut a=WindowAccumulator::new("tenant-a","payments",0,1000).unwrap(); let mut e=event(1); e.tenant_partition="tenant-b".into(); assert_eq!(a.push(e),Err("partition_mismatch")); let mut e=event(2); e.service_partition="auth".into(); assert_eq!(a.push(e),Err("partition_mismatch")); }
    #[test] fn rejects_events_outside_window() { let mut a=WindowAccumulator::new("tenant-a","payments",10,100).unwrap(); assert_eq!(a.push(event(9)),Err("event_outside_window")); assert_eq!(a.push(event(100)),Err("event_outside_window")); }
    #[test] fn hard_cap_prevents_unbounded_accumulation() { let mut a=WindowAccumulator::with_limits("tenant-a","payments",0,100,2,1).unwrap(); a.push(event(1)).unwrap(); a.push(event(2)).unwrap(); assert_eq!(a.push(event(3)),Err("window_capacity_exceeded")); assert_eq!(a.build().event_count,2); }
    #[test] fn duration_sample_is_capped_but_event_count_keeps_total() { let mut a=WindowAccumulator::with_limits("tenant-a","payments",0,100,10,2).unwrap(); for d in [10.0,20.0,30.0,40.0] { let mut e=event(1); e.duration_ms=Some(d); a.push(e).unwrap(); } let w=a.build(); assert_eq!(w.event_count,4); assert_eq!(w.duration_mean_ms,Some(25.0)); assert!(w.duration_p50_ms.is_some_and(|v| (10.0..=40.0).contains(&v))); assert!(w.duration_p95_ms.is_some_and(|v| (10.0..=40.0).contains(&v))); }
    #[test] fn aggregates_flags_rates_and_distinct_trace_count() { let mut a=WindowAccumulator::new("tenant-a","payments",0,100).unwrap(); for (ts,trace,is_error,is_retry) in [(1,"t1",true,false),(2,"t1",true,false),(3,"t2",false,true)] { let mut e=event(ts); e.trace_partition=Some(trace.into()); e.is_error=is_error; e.is_failure=is_error; e.is_retry=is_retry; e.is_slow=is_error; e.is_invariant_violation=is_retry; a.push(e).unwrap(); } let w=a.build(); assert_eq!(w.event_count,3); assert_eq!(w.error_count,2); assert_eq!(w.retry_count,1); assert_eq!(w.independent_trace_count,2); assert!((w.error_rate-2.0/3.0).abs()<1e-9); }
    #[test] fn rejects_invalid_feature_values() { let mut a=WindowAccumulator::new("tenant-a","payments",0,100).unwrap(); let mut e=event(1); e.cpu_utilization=Some(f64::NAN); assert_eq!(a.push(e),Err("invalid_resource_ratio")); }
    #[test] fn cold_provider_always_abstains() { let p=ColdStartProvider; let w=window(); let s=p.score(&w).unwrap(); assert_eq!(s.anomaly_score,None); assert_eq!(s.abstention,Some(AbstentionReason::InsufficientBaseline)); }
    #[tokio::test] async fn disabled_runner_never_invokes_provider() { let r=ShadowRunner::new(false,Arc::new(ColdStartProvider),1,Duration::from_millis(100)).unwrap(); assert_eq!(r.score(window()).await,ShadowOutcome::Disabled); }
    #[tokio::test] async fn cold_runner_returns_abstention_advisory() { let r=ShadowRunner::new(true,Arc::new(ColdStartProvider),1,Duration::from_millis(100)).unwrap(); match r.score(window()).await { ShadowOutcome::Completed(s) => assert_eq!(s.abstention,Some(AbstentionReason::InsufficientBaseline)), other=>panic!("unexpected: {other:?}") } }
    #[tokio::test] async fn panicking_provider_is_fail_open() { struct Panic; impl AdvisoryProvider for Panic { fn provider_id(&self)->&'static str{"panic"} fn model_version(&self)->&'static str{"1"} fn score(&self,_:&AggregatedWindow)->Result<AdvisoryScore,ProviderError>{panic!("simulated")}} let r=ShadowRunner::new(true,Arc::new(Panic),1,Duration::from_secs(1)).unwrap(); assert!(matches!(r.score(window()).await,ShadowOutcome::ProviderFailed(_))); }
    #[tokio::test] async fn timeout_is_fail_open() { struct Slow; impl AdvisoryProvider for Slow { fn provider_id(&self)->&'static str{"slow"} fn model_version(&self)->&'static str{"1"} fn score(&self,_:&AggregatedWindow)->Result<AdvisoryScore,ProviderError>{std::thread::sleep(Duration::from_millis(100)); Ok(ColdStartProvider.score(&window()).unwrap())}} let r=ShadowRunner::new(true,Arc::new(Slow),1,Duration::from_millis(5)).unwrap(); assert_eq!(r.score(window()).await,ShadowOutcome::TimedOut); }
    #[tokio::test] async fn capacity_saturated_is_fail_open() { struct Slow; impl AdvisoryProvider for Slow { fn provider_id(&self)->&'static str{"slow"} fn model_version(&self)->&'static str{"1"} fn score(&self,_:&AggregatedWindow)->Result<AdvisoryScore,ProviderError>{std::thread::sleep(Duration::from_millis(100)); Ok(ColdStartProvider.score(&window()).unwrap())}} let r=Arc::new(ShadowRunner::new(true,Arc::new(Slow),1,Duration::from_secs(1)).unwrap()); let first_runner=r.clone(); let first=tokio::spawn(async move {first_runner.score(window()).await}); tokio::time::sleep(Duration::from_millis(10)).await; assert_eq!(r.score(window()).await,ShadowOutcome::CapacitySkipped); let _=first.await; }
    #[tokio::test]
    async fn shadow_sql_aggregates_tenant_service_groups_when_test_db_is_available() {
        let Ok(url) = std::env::var("GATE4_DATABASE_URL") else { return; };
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        sqlx::raw_sql(r#"
            CREATE TEMP TABLE lite_event_index (
                id BIGINT, event_ts TIMESTAMPTZ, tenant_id TEXT, service TEXT,
                severity TEXT, outcome TEXT, trace_id TEXT, duration_ms DOUBLE PRECISION,
                quality_score DOUBLE PRECISION
            );
            CREATE TEMP TABLE lite_signals (event_id BIGINT, event_ts TIMESTAMPTZ, signal_type TEXT);
            INSERT INTO lite_event_index VALUES
                (101,NOW()-INTERVAL '1 minute','tenant-a','checkout','ERROR','failure','trace-a',10.0,0.9),
                (102,NOW()-INTERVAL '2 minutes','tenant-a','checkout','INFO','success','trace-a',20.0,0.8),
                (201,NOW()-INTERVAL '1 minute','tenant-b','checkout','ERROR','success','trace-b',30.0,1.0);
            INSERT INTO lite_signals VALUES
                (101,NOW()-INTERVAL '1 minute','retry_activity'),
                (101,NOW()-INTERVAL '1 minute','slow_operation'),
                (201,NOW()-INTERVAL '1 minute','business_invariant_or_idempotency');
        "#).execute(&pool).await.unwrap();
        let rows = sqlx::query_as::<_, DbWindowRow>(SHADOW_WINDOW_QUERY).fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 2, "tenants must not merge even for same service name");
        let a = rows.iter().find(|r| r.tenant_id == "tenant-a").unwrap();
        assert_eq!(a.service, "checkout");
        assert_eq!(a.event_count, 2);
        assert_eq!(a.error_count, 1);
        assert_eq!(a.failure_count, 1);
        assert_eq!(a.trace_count, 1);
        assert_eq!(a.retry_count, 1);
        assert_eq!(a.slow_count, 1);
        let b = rows.iter().find(|r| r.tenant_id == "tenant-b").unwrap();
        assert_eq!(b.event_count, 1);
        assert_eq!(b.error_count, 1);
        assert_eq!(b.failure_count, 0);
        assert_eq!(b.invariant_count, 1);
    }
}
