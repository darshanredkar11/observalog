pub mod adapter;
pub mod gates;
pub mod http;
pub mod model;
pub mod queue;
pub mod schema;

use sqlx::PgPool;
use tracing::info;

pub async fn start_workers(pool: PgPool) {
    tokio::spawn(gates::run_normalize(pool.clone()));
    tokio::spawn(gates::run_quality(pool.clone()));
    tokio::spawn(gates::run_correlation(pool.clone()));
    tokio::spawn(gates::run_detection(pool.clone()));
    tokio::spawn(gates::run_reasoning(pool));
    info!("lite streaming gates started");
}
