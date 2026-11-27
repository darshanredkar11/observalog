pub mod adapter;
pub mod gates;
pub mod ml_advisory;
pub mod ml_state_contract;
pub mod correlation;
pub mod http;
pub mod model;
pub mod graph_diagnosis;
pub mod queue;
pub mod schema;

use std::{future::Future, time::Duration};

use sqlx::PgPool;
use tracing::{error, info, warn};

async fn supervise<F, Fut>(gate: &'static str, pool: PgPool, worker: F)
where
    F: Fn(PgPool) -> Fut + Copy + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        let handle = tokio::spawn(worker(pool.clone()));
        match handle.await {
            Ok(()) => warn!(gate, "gate worker exited unexpectedly; restarting"),
            Err(join_error) => error!(gate, error=%join_error, "gate worker failed; restarting"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn start_workers(pool: PgPool) {
    tokio::spawn(supervise("normalize", pool.clone(), gates::run_normalize));
    tokio::spawn(supervise("quality", pool.clone(), gates::run_quality));
    tokio::spawn(supervise("correlate", pool.clone(), gates::run_correlation));
    tokio::spawn(supervise("detect", pool.clone(), gates::run_detection));
    tokio::spawn(supervise("reason", pool.clone(), gates::run_reasoning));
    // Gate 5.1 is an independent, disabled-by-default advisory shadow worker.
    tokio::spawn(ml_advisory::run_shadow_worker(pool));
    info!("lite streaming gates started under per-gate supervision");
}
