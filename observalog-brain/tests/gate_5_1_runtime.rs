use observalog_brain::lite::ml_advisory::{
    AbstentionReason, AdvisoryProvider, AdvisoryScore, AggregatedWindow, ColdStartProvider,
    ProviderError, ShadowOutcome, ShadowRunner, FEATURE_SCHEMA_VERSION,
};
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
use std::time::Duration;

fn window() -> AggregatedWindow {
    AggregatedWindow {
        tenant_partition: "tenant-token-1".into(),
        service_partition: "service-token-1".into(),
        window_start_epoch_ms: 0, window_end_epoch_ms: 300_000,
        feature_schema_version: FEATURE_SCHEMA_VERSION,
        event_count: 10, error_count: 1, failure_outcome_count: 0,
        retry_count: 0, slow_count: 0, invariant_violation_count: 0,
        independent_trace_count: 4, error_rate: 0.1, failure_rate: 0.0,
        retry_rate: 0.0, duration_mean_ms: Some(20.0), duration_p50_ms: Some(10.0),
        duration_p95_ms: Some(50.0), duration_p99_ms: Some(80.0),
        cpu_utilization: None, memory_utilization: None, queue_depth: None,
        queue_depth_delta: None, completeness_ratio: 1.0, dropped_or_missing_count: Some(0),
    }
}

struct ErrorThenAbstain { calls: AtomicUsize }
impl AdvisoryProvider for ErrorThenAbstain {
    fn provider_id(&self) -> &'static str { "test-provider" }
    fn model_version(&self) -> &'static str { "test-1" }
    fn score(&self, w: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ProviderError { provider_id: "test-provider".into(), code: "temporary_unavailable".into(), retryable: true });
        }
        ColdStartProvider.score(w)
    }
}

struct InvalidScore;
impl AdvisoryProvider for InvalidScore {
    fn provider_id(&self) -> &'static str { "invalid" }
    fn model_version(&self) -> &'static str { "1" }
    fn score(&self, w: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError> {
        let mut s = ColdStartProvider.score(w).unwrap();
        s.anomaly_score = Some(2.0);
        s.abstention = None;
        Ok(s)
    }
}

struct SlowProvider;
impl AdvisoryProvider for SlowProvider {
    fn provider_id(&self) -> &'static str { "slow" }
    fn model_version(&self) -> &'static str { "1" }
    fn score(&self, w: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError> {
        std::thread::sleep(Duration::from_millis(80));
        ColdStartProvider.score(w)
    }
}

#[tokio::test]
async fn disabled_runner_makes_zero_provider_calls() {
    let provider = Arc::new(ErrorThenAbstain { calls: AtomicUsize::new(0) });
    let runner = ShadowRunner::new(false, provider.clone(), 1, Duration::from_millis(30)).unwrap();
    assert_eq!(runner.score(window()).await, ShadowOutcome::Disabled);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn provider_error_is_isolated_and_next_invocation_recovers() {
    let provider = Arc::new(ErrorThenAbstain { calls: AtomicUsize::new(0) });
    let runner = ShadowRunner::new(true, provider.clone(), 1, Duration::from_millis(100)).unwrap();
    assert!(matches!(runner.score(window()).await, ShadowOutcome::ProviderFailed(_)));
    match runner.score(window()).await {
        ShadowOutcome::Completed(score) => assert_eq!(score.abstention, Some(AbstentionReason::InsufficientBaseline)),
        other => panic!("expected recovery after transient provider error, got {other:?}"),
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn invalid_provider_score_is_rejected_and_runner_remains_usable() {
    let runner = ShadowRunner::new(true, Arc::new(InvalidScore), 1, Duration::from_millis(100)).unwrap();
    assert_eq!(runner.score(window()).await, ShadowOutcome::InvalidResult("score_out_of_range"));
    let cold = ShadowRunner::new(true, Arc::new(ColdStartProvider), 1, Duration::from_millis(100)).unwrap();
    assert!(matches!(cold.score(window()).await, ShadowOutcome::Completed(_)));
}

#[tokio::test]
async fn timeout_does_not_block_later_runner_calls_after_provider_finishes() {
    let runner = ShadowRunner::new(true, Arc::new(SlowProvider), 1, Duration::from_millis(5)).unwrap();
    assert_eq!(runner.score(window()).await, ShadowOutcome::TimedOut);
    // The bounded worker still owns its permit until the blocking provider exits.
    assert_eq!(runner.score(window()).await, ShadowOutcome::CapacitySkipped);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(matches!(runner.score(window()).await, ShadowOutcome::Completed(_)));
}
