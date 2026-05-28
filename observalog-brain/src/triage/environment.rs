use anyhow::Result;
use rdkafka::{
    admin::{AdminClient, AdminOptions},
    client::DefaultClientContext,
    ClientConfig,
};
use redis::AsyncCommands;
use serde::Serialize;
use tracing::warn;

/// Environment snapshot at triage trigger time — Decision 12 (borrowed from vercel-labs/zero).
/// Captured before LLM call so the model has current service health context.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentSnapshot {
    pub kafka_consumer_lag: Option<i64>,
    pub kafka_topic: Option<String>,
    pub valkey_gap_keys_count: Option<usize>,
    pub error_rate_1m: Option<f64>,     // placeholder — from metrics sink
    pub services_degraded: Vec<String>, // service names with recent failures
    pub snapshot_ts: chrono::DateTime<chrono::Utc>,
}

impl EnvironmentSnapshot {
    /// Capture current environment state.
    pub async fn capture(
        redis_conn: &mut redis::aio::MultiplexedConnection,
        failing_service: Option<i16>,
    ) -> Self {
        let valkey_gap_keys_count =
            count_gap_keys(redis_conn, failing_service).await.ok();

        EnvironmentSnapshot {
            kafka_consumer_lag: None, // populated by Kafka admin client if available
            kafka_topic: std::env::var("KAFKA_TOPIC").ok(),
            valkey_gap_keys_count,
            error_rate_1m: None,
            services_degraded: vec![],
            snapshot_ts: chrono::Utc::now(),
        }
    }
}

/// Count open gap grace-window keys for a service in Valkey.
/// Gap 5 / Gap 7: Valkey is the out-of-order sequence buffer (30s TTL).
async fn count_gap_keys(
    conn: &mut redis::aio::MultiplexedConnection,
    service: Option<i16>,
) -> Result<usize> {
    let pattern = match service {
        Some(svc) => format!("gap:*:{}:*", svc),
        None => "gap:*".to_string(),
    };
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(&pattern)
        .query_async(conn)
        .await
        .unwrap_or_default();
    Ok(keys.len())
}
