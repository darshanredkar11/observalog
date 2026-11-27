use observalog_brain::lite::ml_advisory::{
    AggregateEvent, AdvisoryProvider, AdvisoryScore, AggregatedWindow, FeatureContribution,
    ProviderError, ShadowOutcome, ShadowRunner, WindowAccumulator, FEATURE_SCHEMA_VERSION,
};
use std::{sync::Arc, time::Duration};

/// E2E-like exercise of aggregate telemetry -> feature window -> shadow provider -> validated score.
/// Provider is deliberately a deterministic test double: this does NOT validate a trained ML model.
struct ThresholdTestProvider;
impl AdvisoryProvider for ThresholdTestProvider {
    fn provider_id(&self) -> &'static str { "e2e-test-provider" }
    fn model_version(&self) -> &'static str { "test-threshold-v1" }
    fn score(&self, window: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError> {
        let score = window.error_rate;
        Ok(AdvisoryScore {
            provider_id: self.provider_id().into(),
            model_version: self.model_version().into(),
            feature_schema_version: window.feature_schema_version,
            anomaly_score: Some(score),
            abstention: None,
            feature_contributions: vec![FeatureContribution {
                feature_name: "error_rate".into(),
                contribution: score,
            }],
        })
    }
}

fn event(tenant: &str, service: &str, ts: i64, error: bool, duration: f64, trace: &str) -> AggregateEvent {
    AggregateEvent {
        tenant_partition: tenant.into(),
        service_partition: service.into(),
        event_ts_epoch_ms: ts,
        is_error: error,
        is_failure: false,
        is_retry: false,
        is_slow: duration > 100.0,
        is_invariant_violation: false,
        trace_partition: Some(trace.into()),
        duration_ms: Some(duration),
        cpu_utilization: Some(0.35),
        memory_utilization: Some(0.50),
        queue_depth: Some(3),
        completeness_ratio: 1.0,
        dropped_or_missing_count: Some(0),
    }
}

#[tokio::test]
async fn telemetry_aggregates_flow_through_shadow_runner_to_validated_score() {
    let mut accumulator = WindowAccumulator::new("tenant-token-a", "service-token-a", 0, 300_000).unwrap();
    for i in 0..100 {
        accumulator.push(event("tenant-token-a", "service-token-a", i * 1000, i < 20, 10.0 + i as f64, &format!("trace-{i}"))).unwrap();
    }
    let window = accumulator.build();
    assert_eq!(window.feature_schema_version, FEATURE_SCHEMA_VERSION);
    assert_eq!(window.event_count, 100);
    assert_eq!(window.error_count, 20);
    assert!((window.error_rate - 0.20).abs() < 1e-9);
    assert_eq!(window.independent_trace_count, 100);

    let runner = ShadowRunner::new(true, Arc::new(ThresholdTestProvider), 1, Duration::from_secs(1)).unwrap();
    match runner.score(window).await {
        ShadowOutcome::Completed(score) => {
            assert_eq!(score.provider_id, "e2e-test-provider");
            assert_eq!(score.model_version, "test-threshold-v1");
            assert_eq!(score.feature_schema_version, FEATURE_SCHEMA_VERSION);
            assert_eq!(score.anomaly_score, Some(0.20));
            assert_eq!(score.abstention, None);
            assert_eq!(score.feature_contributions.len(), 1);
            assert_eq!(score.feature_contributions[0].feature_name, "error_rate");
        }
        other => panic!("expected completed validated score, got {other:?}"),
    }
}

#[tokio::test]
async fn tenant_service_partition_mismatch_is_rejected_before_scoring() {
    let mut accumulator = WindowAccumulator::new("tenant-token-a", "service-token-a", 0, 300_000).unwrap();
    assert_eq!(
        accumulator.push(event("tenant-token-b", "service-token-a", 1000, true, 20.0, "trace-cross-tenant")),
        Err("partition_mismatch")
    );
    let runner = ShadowRunner::new(true, Arc::new(ThresholdTestProvider), 1, Duration::from_secs(1)).unwrap();
    let window = accumulator.build();
    assert_eq!(window.event_count, 0);
    // The cross-tenant event was rejected by the accumulator. Empty windows
    // remain valid-but-incomplete by contract; this test provider can still
    // return a bounded zero score, so don't misclassify the empty window as
    // invalid input.
    match runner.score(window).await {
        ShadowOutcome::Completed(score) => assert_eq!(score.anomaly_score, Some(0.0)),
        other => panic!("expected valid empty-window test score, got {other:?}"),
    }
}
