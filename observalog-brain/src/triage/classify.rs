use super::chain::TraceChain;
use crate::db::queries::IndexRow;
use serde::Serialize;

/// Classification of a trace failure pattern.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// Single service failure, no propagation.
    Isolated,
    /// Failure propagated across multiple services.
    Cascading,
    /// One service timed out waiting for a dependency.
    Timeout,
    /// External dependency (provider/infra) caused the failure.
    ExternalDependency,
    /// Missing log sequence detected — possible dropped logs.
    GapDetected,
    /// Partial success — some operations succeeded, others failed.
    Partial,
    /// No clear failure pattern.
    Unknown,
}

/// Result of classifying a trace chain.
#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub class: FailureClass,
    pub confidence: f32,     // 0.0 – 1.0
    pub failing_service: Option<i16>,
    pub trigger_event: Option<String>,
    pub notes: Vec<String>,
}

/// Classify a trace chain's failure pattern.
pub fn classify(chain: &TraceChain) -> Classification {
    let error_rows: Vec<&IndexRow> = chain.rows.iter().filter(|r| r.level >= 3).collect();
    let failure_rows: Vec<&IndexRow> = chain.rows.iter().filter(|r| r.outcome == Some(2)).collect();

    if error_rows.is_empty() && failure_rows.is_empty() {
        return Classification {
            class: FailureClass::Unknown,
            confidence: 0.5,
            failing_service: None,
            trigger_event: None,
            notes: vec!["No error or failure rows in trace".to_string()],
        };
    }

    // Count distinct services with errors.
    let error_services: std::collections::HashSet<i16> =
        error_rows.iter().map(|r| r.service).collect();

    // Check for cascading failure (errors in multiple services).
    if error_services.len() > 1 {
        let first_error = error_rows.first().unwrap();
        return Classification {
            class: FailureClass::Cascading,
            confidence: 0.85,
            failing_service: Some(first_error.service),
            trigger_event: None,
            notes: vec![format!(
                "Errors in {} services: {:?}",
                error_services.len(),
                error_services
            )],
        };
    }

    // Check for external dependency failure (service=3 = provider, or infra).
    if error_services.contains(&3) || error_services.contains(&0) {
        return Classification {
            class: FailureClass::ExternalDependency,
            confidence: 0.80,
            failing_service: Some(*error_services.iter().next().unwrap()),
            trigger_event: None,
            notes: vec!["Failure in external provider or infrastructure".to_string()],
        };
    }

    // Check for gaps (set by gap.rs).
    if chain.has_gaps {
        return Classification {
            class: FailureClass::GapDetected,
            confidence: 0.70,
            failing_service: error_rows.first().map(|r| r.service),
            trigger_event: None,
            notes: vec!["Sequence gap detected in trace".to_string()],
        };
    }

    // Check for partial outcome.
    if failure_rows
        .iter()
        .any(|r| r.outcome == Some(3)) // partial
    {
        return Classification {
            class: FailureClass::Partial,
            confidence: 0.75,
            failing_service: failure_rows.first().map(|r| r.service),
            trigger_event: None,
            notes: vec!["Partial success/failure outcome".to_string()],
        };
    }

    // Isolated single-service failure.
    let failing_service = error_rows.first().map(|r| r.service);
    Classification {
        class: FailureClass::Isolated,
        confidence: 0.90,
        failing_service,
        trigger_event: None,
        notes: vec!["Single service failure, no propagation".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triage::chain::TraceChain;
    use chrono::Utc;

    fn make_row(service: i16, level: i16, outcome: Option<i16>) -> IndexRow {
        IndexRow {
            id: 1,
            trace_id: "trc_test000001".to_string(),
            span_id: "spn_001".to_string(),
            parent_span: None,
            service,
            level,
            outcome,
            seq: 1,
            ts: Utc::now(),
            user_id: None,
            fingerprint: None,
            payload_id: 1,
        }
    }

    #[test]
    fn test_isolated_failure() {
        let chain = TraceChain {
            trace_id: "trc_test".to_string(),
            rows: vec![make_row(1, 3, Some(2))],
            ts_from: Utc::now(),
            ts_to: Utc::now(),
            service_count: 1,
            has_errors: true,
            has_gaps: false,
        };
        let c = classify(&chain);
        assert_eq!(c.class, FailureClass::Isolated);
    }

    #[test]
    fn test_cascading_failure() {
        let chain = TraceChain {
            trace_id: "trc_test".to_string(),
            rows: vec![make_row(1, 3, Some(2)), make_row(2, 3, Some(2))],
            ts_from: Utc::now(),
            ts_to: Utc::now(),
            service_count: 2,
            has_errors: true,
            has_gaps: false,
        };
        let c = classify(&chain);
        assert_eq!(c.class, FailureClass::Cascading);
    }
}
