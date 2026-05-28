use serde::{Deserialize, Serialize};
use std::fmt;

/// Typed repair categories — Decision 12 (borrowed from vercel-labs/zero).
/// LLM output must map to one of these rather than free-form prose.
/// This enables deterministic routing to fix playbooks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RepairId {
    /// Increase timeout or add retry with backoff.
    NetworkRetry,
    /// Rate limit hit — implement exponential backoff or queue.
    RateLimitBackoff,
    /// Input failed validation — fix schema or caller.
    ValidationFix,
    /// DB query failed — check constraints, missing index, deadlock.
    DatabaseFix,
    /// Auth token invalid/expired — refresh token or re-authenticate.
    AuthRefresh,
    /// Service dependency unavailable — circuit breaker or fallback.
    DependencyFallback,
    /// Configuration error — environment variable or config file.
    ConfigFix,
    /// Resource exhausted (memory, file handles, connection pool).
    ResourceExhausted,
    /// Data consistency issue — idempotency or saga rollback.
    ConsistencyFix,
    /// Unknown — LLM could not determine a specific repair category.
    Unknown,
}

impl RepairId {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepairId::NetworkRetry       => "NETWORK_RETRY",
            RepairId::RateLimitBackoff   => "RATE_LIMIT_BACKOFF",
            RepairId::ValidationFix      => "VALIDATION_FIX",
            RepairId::DatabaseFix        => "DATABASE_FIX",
            RepairId::AuthRefresh        => "AUTH_REFRESH",
            RepairId::DependencyFallback => "DEPENDENCY_FALLBACK",
            RepairId::ConfigFix          => "CONFIG_FIX",
            RepairId::ResourceExhausted  => "RESOURCE_EXHAUSTED",
            RepairId::ConsistencyFix     => "CONSISTENCY_FIX",
            RepairId::Unknown            => "UNKNOWN",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "NETWORK_RETRY"       => RepairId::NetworkRetry,
            "RATE_LIMIT_BACKOFF"  => RepairId::RateLimitBackoff,
            "VALIDATION_FIX"      => RepairId::ValidationFix,
            "DATABASE_FIX"        => RepairId::DatabaseFix,
            "AUTH_REFRESH"        => RepairId::AuthRefresh,
            "DEPENDENCY_FALLBACK" => RepairId::DependencyFallback,
            "CONFIG_FIX"          => RepairId::ConfigFix,
            "RESOURCE_EXHAUSTED"  => RepairId::ResourceExhausted,
            "CONSISTENCY_FIX"     => RepairId::ConsistencyFix,
            _                     => RepairId::Unknown,
        }
    }

    /// Whether this repair requires human escalation.
    pub fn requires_escalation(&self) -> bool {
        matches!(self, RepairId::ConsistencyFix | RepairId::Unknown)
    }

    /// Whether an automated fix can be attempted without human review.
    pub fn is_automatable(&self) -> bool {
        matches!(
            self,
            RepairId::NetworkRetry | RepairId::RateLimitBackoff | RepairId::AuthRefresh
        )
    }
}

impl fmt::Display for RepairId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let r = RepairId::RateLimitBackoff;
        assert_eq!(RepairId::from_str(r.as_str()), r);
    }

    #[test]
    fn test_serde_json() {
        let r = RepairId::DatabaseFix;
        let s = serde_json::to_string(&r).unwrap();
        let r2: RepairId = serde_json::from_str(&s).unwrap();
        assert_eq!(r, r2);
    }
}
