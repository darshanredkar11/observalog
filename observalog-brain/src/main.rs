use anyhow::{Context, Result};
use observalog_brain::{
    db::schema,
    ingest::kafka::{KafkaConfig, run as kafka_run},
    triage::llm::LlmConfig,
    ws::{router, AppState},
};
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, sync::Arc};
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Tracing setup.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("observalog-brain starting");

    // Database.
    let db_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL required (postgres://user:pass@host/db)")?;

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&db_url)
        .await
        .context("DB connect failed")?;

    // Apply schema DDL.
    schema::run(&pool).await.context("schema migration failed")?;
    info!("schema applied");

    // Redis / Valkey (Gap 5/7: out-of-order buffer, 30s grace window).
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis_client = redis::Client::open(redis_url.as_str())
        .context("Redis client init failed")?;

    // LLM config.
    let llm_cfg = LlmConfig::from_env().context("LLM config failed")?;

    // Start Kafka consumer in background.
    let pool_kafka = pool.clone();
    let kafka_cfg = KafkaConfig::from_env().context("Kafka config failed")?;
    tokio::spawn(async move {
        if let Err(e) = kafka_run(pool_kafka, kafka_cfg).await {
            warn!(error = %e, "Kafka consumer exited");
        }
    });

    // Start WebSocket API.
    let state = Arc::new(AppState {
        db: pool,
        redis: redis_client,
        llm_cfg: Arc::new(llm_cfg),
    });

    let app = router(state);
    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4000".to_string())
        .parse()
        .context("invalid LISTEN_ADDR")?;

    info!(addr = %addr, "WebSocket API listening");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("bind failed")?;
    axum::serve(listener, app)
        .await
        .context("server error")?;

    Ok(())
}
