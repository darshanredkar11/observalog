use super::repair::RepairId;
use crate::db::queries;
use anyhow::Result;
use sqlx::PgPool;
use tracing::debug;

/// Result of a dedup check.
#[derive(Debug)]
pub enum DedupResult {
    /// Known issue — skip LLM, use cached fix.
    Known {
        repair_id: RepairId,
        cached_fix: Option<String>,
        occurrence_count: i64,
    },
    /// New issue — requires LLM triage.
    New,
}

/// Check fingerprint against known_issues cache.
/// Decision 9: O(1) via HASH index on fingerprint.
/// Eliminates ~95% of LLM calls for repeated known errors.
pub async fn check(pool: &PgPool, fingerprint: i64) -> Result<DedupResult> {
    let known = queries::find_known_issue(pool, fingerprint).await?;

    match known {
        Some(issue) => {
            debug!(
                fingerprint,
                repair_id = %issue.repair_id,
                occurrences = issue.occurrence_count,
                "fingerprint hit — known issue"
            );
            Ok(DedupResult::Known {
                repair_id: RepairId::from_str(&issue.repair_id),
                cached_fix: issue.cached_fix,
                occurrence_count: issue.occurrence_count,
            })
        }
        None => {
            debug!(fingerprint, "fingerprint miss — new issue");
            Ok(DedupResult::New)
        }
    }
}

/// Record a newly triaged issue in the cache.
pub async fn record(
    pool: &PgPool,
    fingerprint: i64,
    service: i16,
    event: &str,
    error_code: &str,
    repair_id: &RepairId,
    cached_fix: Option<&str>,
) -> Result<()> {
    queries::upsert_known_issue(
        pool,
        fingerprint,
        service,
        event,
        error_code,
        repair_id,
        cached_fix,
    )
    .await
}
