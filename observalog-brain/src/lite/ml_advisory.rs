//! Gate 5.1 advisory ML boundary.
//!
//! Gate 5 remains authoritative. This module defines the feature-only contract
//! that an optional provider may score. It deliberately contains no raw-log
//! payloads, DB access, tenant authorization, or provider implementation.

use serde::{Deserialize, Serialize};

pub const FEATURE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedWindow {
    /// Tenant partition is resolved and enforced by the host pipeline.
    /// Providers must not use this value to perform cross-tenant lookup.
    pub tenant_partition: String,
    pub window_start_epoch_ms: i64,
    pub window_end_epoch_ms: i64,
    pub feature_schema_version: u16,

    // Volume and deterministic signals.
    pub event_count: u64,
    pub error_count: u64,
    pub failure_outcome_count: u64,
    pub retry_count: u64,
    pub slow_count: u64,
    pub invariant_violation_count: u64,
    pub independent_trace_count: u64,

    // Rates and aggregate measurements (never individual event data).
    pub error_rate: f64,
    pub failure_rate: f64,
    pub retry_rate: f64,
    pub duration_mean_ms: Option<f64>,
    pub duration_p50_ms: Option<f64>,
    pub duration_p95_ms: Option<f64>,
    pub duration_p99_ms: Option<f64>,

    // Resource/queue signals, optional where telemetry is absent.
    pub cpu_utilization: Option<f64>,
    pub memory_utilization: Option<f64>,
    pub queue_depth: Option<u64>,
    pub queue_depth_delta: Option<i64>,

    // Window quality is explicit; cold/incomplete windows must be abstain-able.
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisoryScore {
    pub provider_id: String,
    pub model_version: String,
    pub feature_schema_version: u16,
    /// 0.0..=1.0, advisory only. None means the provider abstained.
    pub anomaly_score: Option<f64>,
    pub abstention: Option<AbstentionReason>,
    /// Provider's explanation of feature-level contribution. No raw log text.
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

/// Host-facing provider contract. Provider-specific model state and persistence
/// are intentionally owned by the provider implementation and versioned there.
/// Host code must apply timeout, concurrency, and failure isolation.
pub trait AdvisoryProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn model_version(&self) -> &'static str;
    fn score(&self, window: &AggregatedWindow) -> Result<AdvisoryScore, ProviderError>;
}

/// Validate the window contract before calling any provider.
pub fn validate_window(window: &AggregatedWindow) -> Result<(), &'static str> {
    if window.feature_schema_version != FEATURE_SCHEMA_VERSION {
        return Err("unsupported_feature_schema");
    }
    if window.tenant_partition.trim().is_empty() {
        return Err("tenant_partition_required");
    }
    if window.window_end_epoch_ms <= window.window_start_epoch_ms {
        return Err("invalid_window_bounds");
    }
    if !finite_ratio(window.error_rate)
        || !finite_ratio(window.failure_rate)
        || !finite_ratio(window.retry_rate)
        || !finite_ratio(window.completeness_ratio)
    {
        return Err("invalid_ratio");
    }
    for value in [
        window.duration_mean_ms,
        window.duration_p50_ms,
        window.duration_p95_ms,
        window.duration_p99_ms,
    ].into_iter().flatten() {
        if !value.is_finite() || value < 0.0 {
            return Err("invalid_duration");
        }
    }
    for value in [window.cpu_utilization, window.memory_utilization].into_iter().flatten() {
        if !finite_ratio(value) {
            return Err("invalid_utilization");
        }
    }
    Ok(())
}

fn finite_ratio(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

pub fn validate_score(score: &AdvisoryScore) -> Result<(), &'static str> {
    match score.anomaly_score {
        Some(value) if !value.is_finite() || !(0.0..=1.0).contains(&value) => {
            Err("score_out_of_range")
        }
        Some(_) if score.abstention.is_some() => Err("score_and_abstention_conflict"),
        None if score.abstention.is_none() => Err("abstention_reason_required"),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_window() -> AggregatedWindow {
        AggregatedWindow {
            tenant_partition: "opaque-tenant-scope".into(),
            window_start_epoch_ms: 1000,
            window_end_epoch_ms: 2000,
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            event_count: 100,
            error_count: 5,
            failure_outcome_count: 2,
            retry_count: 10,
            slow_count: 3,
            invariant_violation_count: 0,
            independent_trace_count: 50,
            error_rate: 0.05,
            failure_rate: 0.02,
            retry_rate: 0.10,
            duration_mean_ms: Some(25.0),
            duration_p50_ms: Some(10.0),
            duration_p95_ms: Some(100.0),
            duration_p99_ms: Some(300.0),
            cpu_utilization: Some(0.55),
            memory_utilization: Some(0.62),
            queue_depth: Some(12),
            queue_depth_delta: Some(4),
            completeness_ratio: 1.0,
            dropped_or_missing_count: Some(0),
        }
    }

    #[test]
    fn validates_aggregated_window() {
        assert_eq!(validate_window(&sample_window()), Ok(()));
    }

    #[test]
    fn rejects_invalid_window_bounds() {
        let mut window = sample_window();
        window.window_end_epoch_ms = window.window_start_epoch_ms;
        assert_eq!(validate_window(&window), Err("invalid_window_bounds"));
    }

    #[test]
    fn rejects_invalid_feature_ratios() {
        let mut window = sample_window();
        window.error_rate = f64::NAN;
        assert_eq!(validate_window(&window), Err("invalid_ratio"));
    }

    #[test]
    fn rejects_unknown_feature_schema() {
        let mut window = sample_window();
        window.feature_schema_version = 999;
        assert_eq!(validate_window(&window), Err("unsupported_feature_schema"));
    }

    #[test]
    fn score_is_bounded_and_advisory() {
        let score = AdvisoryScore {
            provider_id: "test".into(),
            model_version: "cold-0".into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            anomaly_score: Some(0.42),
            abstention: None,
            feature_contributions: vec![],
        };
        assert_eq!(validate_score(&score), Ok(()));
    }

    #[test]
    fn cold_model_can_abstain() {
        let score = AdvisoryScore {
            provider_id: "test".into(),
            model_version: "cold-0".into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            anomaly_score: None,
            abstention: Some(AbstentionReason::InsufficientBaseline),
            feature_contributions: vec![],
        };
        assert_eq!(validate_score(&score), Ok(()));
    }

    #[test]
    fn rejects_score_and_abstention_together() {
        let mut score = AdvisoryScore {
            provider_id: "test".into(),
            model_version: "test".into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            anomaly_score: Some(0.5),
            abstention: None,
            feature_contributions: vec![],
        };
        score.abstention = Some(AbstentionReason::ProviderUnavailable);
        assert_eq!(validate_score(&score), Err("score_and_abstention_conflict"));
    }

    #[test]
    fn rejects_unbounded_score() {
        let score = AdvisoryScore {
            provider_id: "test".into(),
            model_version: "test".into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            anomaly_score: Some(1.01),
            abstention: None,
            feature_contributions: vec![],
        };
        assert_eq!(validate_score(&score), Err("score_out_of_range"));
    }

    #[test]
    fn rejects_score_without_abstention_reason() {
        let score = AdvisoryScore {
            provider_id: "test".into(),
            model_version: "test".into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION,
            anomaly_score: None,
            abstention: None,
            feature_contributions: vec![],
        };
        assert_eq!(validate_score(&score), Err("abstention_reason_required"));
    }
}
