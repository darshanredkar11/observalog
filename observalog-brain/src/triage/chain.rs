use crate::db::queries::{self, IndexRow};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;

/// A fully reconstructed trace journey — ordered sequence of index rows.
#[derive(Debug, Clone, Serialize)]
pub struct TraceChain {
    pub trace_id: String,
    pub rows: Vec<IndexRow>,
    pub ts_from: DateTime<Utc>,
    pub ts_to: DateTime<Utc>,
    pub service_count: usize,
    pub has_errors: bool,
    pub has_gaps: bool,
}

/// Call graph node — Decision 12 (borrowed from vercel-labs/zero).
/// Represents structural dependency context per triage finding.
#[derive(Debug, Clone, Serialize)]
pub struct CallGraphNode {
    pub service: i16,
    pub span_id: String,
    pub parent_span: Option<String>,
    pub event: String,
    pub level: i16,
    pub seq: i16,
    pub ts: DateTime<Utc>,
}

/// Reconstruct the full trace chain for a trace_id.
/// CRITICAL: always supplies ts bounds (Gap 1 / Decision 10).
/// Default window: event ts ± 2 hours forward, 5 minutes back.
pub async fn reconstruct(
    pool: &PgPool,
    trace_id: &str,
    anchor_ts: DateTime<Utc>,
) -> Result<TraceChain> {
    // Gap 1: include ts bounds — TimescaleDB scans 1 chunk instead of 30.
    let ts_from = anchor_ts - Duration::hours(2);
    let ts_to = anchor_ts + Duration::minutes(5);

    let rows = queries::fetch_journey(pool, trace_id, ts_from, ts_to).await?;

    let has_errors = rows.iter().any(|r| r.level >= 3);
    let service_ids: std::collections::HashSet<i16> =
        rows.iter().map(|r| r.service).collect();

    Ok(TraceChain {
        trace_id: trace_id.to_string(),
        rows,
        ts_from,
        ts_to,
        service_count: service_ids.len(),
        has_errors,
        has_gaps: false, // set by gap.rs after detection
    })
}

/// Build call graph JSON from a trace chain — Decision 12.
/// Provides structural dependency context per triage finding.
pub fn build_call_graph(chain: &TraceChain) -> Vec<CallGraphNode> {
    chain
        .rows
        .iter()
        .map(|r| CallGraphNode {
            service: r.service,
            span_id: r.span_id.clone(),
            parent_span: r.parent_span.clone(),
            event: format!("log_index.{}", r.id), // placeholder — real event from payload
            level: r.level,
            seq: r.seq,
            ts: r.ts,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_window_calculation() {
        let anchor = Utc::now();
        let ts_from = anchor - Duration::hours(2);
        let ts_to = anchor + Duration::minutes(5);
        assert!(ts_from < anchor);
        assert!(ts_to > anchor);
        // Verify exact window
        assert_eq!((anchor - ts_from).num_hours(), 2);
        assert_eq!((ts_to - anchor).num_minutes(), 5);
    }
}
