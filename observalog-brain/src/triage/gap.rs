use super::chain::TraceChain;
use crate::db::queries;
use anyhow::Result;
use redis::AsyncCommands;
use serde::Serialize;
use sqlx::PgPool;
use tracing::debug;

/// A detected sequence gap within one service's span of a trace.
#[derive(Debug, Clone, Serialize)]
pub struct Gap {
    pub trace_id: String,
    pub service: i16,
    /// Last seq seen before the gap.
    pub seq_before: u8,
    /// First seq seen after the gap.
    pub seq_after: u8,
    /// Number of missing seq values.
    pub missing_count: u8,
}

/// Gap detection result.
#[derive(Debug, Clone)]
pub struct GapReport {
    pub gaps: Vec<Gap>,
    pub is_confirmed: bool, // false = within 30s grace window (Gap 5)
}

/// Detect sequence gaps in a trace chain.
/// Gap 5: use Valkey 30-second grace window before classifying a gap as real.
/// Out-of-order delivery within 30s is normal; only confirm after the window.
pub async fn detect(
    pool: &PgPool,
    chain: &TraceChain,
    redis_conn: &mut redis::aio::MultiplexedConnection,
) -> Result<GapReport> {
    let seq_rows = queries::fetch_seq_chain(
        pool,
        &chain.trace_id,
        chain.ts_from,
        chain.ts_to,
    )
    .await?;

    let raw_gaps = find_gaps(&seq_rows, &chain.trace_id);

    if raw_gaps.is_empty() {
        return Ok(GapReport { gaps: vec![], is_confirmed: true });
    }

    // Gap 5: check the 30-second Valkey grace window.
    // Key: "gap:<trace_id>:<service>:<seq_before>" with 30s TTL.
    // If the key exists, the gap was first seen < 30s ago — not confirmed yet.
    let mut confirmed_gaps = Vec::new();
    for gap in raw_gaps {
        let key = format!("gap:{}:{}:{}", gap.trace_id, gap.service, gap.seq_before);

        let exists: bool = redis_conn.exists(&key).await.unwrap_or(false);
        if exists {
            debug!(
                trace_id = %gap.trace_id,
                service = gap.service,
                "gap within 30s grace window, skipping"
            );
        } else {
            // Set the grace window marker.
            let _: () = redis_conn
                .set_ex(&key, 1u8, 30)
                .await
                .unwrap_or(());
            confirmed_gaps.push(gap);
        }
    }

    let is_confirmed = !confirmed_gaps.is_empty();
    Ok(GapReport { gaps: confirmed_gaps, is_confirmed })
}

/// Find gaps in a sorted list of (service, seq) pairs.
fn find_gaps(rows: &[(i16, i16)], trace_id: &str) -> Vec<Gap> {
    let mut gaps = Vec::new();

    // Group by service
    let mut by_service: std::collections::HashMap<i16, Vec<u8>> =
        std::collections::HashMap::new();

    for (service, seq) in rows {
        by_service
            .entry(*service)
            .or_default()
            .push(*seq as u8);
    }

    for (service, mut seqs) in by_service {
        seqs.sort_unstable();
        seqs.dedup();

        for window in seqs.windows(2) {
            let prev = window[0];
            let next = window[1];
            // Seq wraps at 256 (2-char hex). A gap is any jump > 1 (mod 256).
            let expected_next = prev.wrapping_add(1);
            if next != expected_next {
                let missing_count = next.wrapping_sub(expected_next);
                gaps.push(Gap {
                    trace_id: trace_id.to_string(),
                    service,
                    seq_before: prev,
                    seq_after: next,
                    missing_count,
                });
            }
        }
    }

    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_gap_in_sequential_seqs() {
        let rows = vec![(1i16, 1i16), (1, 2), (1, 3), (1, 4)];
        let gaps = find_gaps(&rows, "trc_test");
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_detects_gap() {
        let rows = vec![(1i16, 1i16), (1, 2), (1, 5)]; // seq 3 and 4 are missing = 2 missing
        let gaps = find_gaps(&rows, "trc_test");
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].seq_before, 2);
        assert_eq!(gaps[0].seq_after, 5);
        assert_eq!(gaps[0].missing_count, 2); // missing: 3, 4
    }

    #[test]
    fn test_independent_gaps_per_service() {
        let rows = vec![
            (1i16, 1i16), (1, 3), // service 1: gap between 1 and 3
            (2, 1), (2, 2), (2, 3), // service 2: no gap
        ];
        let gaps = find_gaps(&rows, "trc_test");
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].service, 1);
    }
}
