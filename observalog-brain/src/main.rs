use anyhow::{Context, Result};
use observalog_brain::lite::{self, http};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("ObservaLog Lite starting");

    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL is required")?;
    let ingest_token = std::env::var("LITE_INGEST_TOKEN")
        .context("LITE_INGEST_TOKEN is required to protect raw ingestion")?;
    anyhow::ensure!(ingest_token.len() >= 24, "LITE_INGEST_TOKEN must be at least 24 bytes");
    let max_connections = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(20);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("TimescaleDB connection failed")?;

    lite::schema::run(&pool)
        .await
        .context("lite schema initialization failed")?;

    lite::start_workers(pool.clone()).await;

    let app = http::router(pool, ingest_token);
    let address: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4000".to_string())
        .parse()
        .context("invalid LISTEN_ADDR")?;

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("HTTP bind failed")?;
    info!(%address, "ObservaLog Lite HTTP ingest listening");
    axum::serve(listener, app).await.context("HTTP server failed")?;

    Ok(())
}
