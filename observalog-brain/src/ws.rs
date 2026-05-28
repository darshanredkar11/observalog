use crate::triage::{
    chain, classify, context, dedup, environment, llm,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, warn};

pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub llm_cfg: Arc<llm::LlmConfig>,
}

/// Request from the UI or alerting system to triage a trace.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsRequest {
    /// Triage a specific trace by ID.
    Triage { trace_id: String, anchor_ts_ms: Option<i64> },
    /// Look up user's recent error traces.
    UserErrors { user_id: String },
}

/// Response sent back over the WebSocket.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsResponse {
    TriageResult(serde_json::Value),
    UserErrors { traces: Vec<String> },
    Error { message: String },
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let response = handle_request(&text, &state).await;
        let json = serde_json::to_string(&response).unwrap_or_else(|_| {
            json!({"type": "error", "message": "serialization failure"}).to_string()
        });

        if socket.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
}

async fn handle_request(text: &str, state: &AppState) -> WsResponse {
    let req: WsRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            return WsResponse::Error {
                message: format!("invalid request: {}", e),
            }
        }
    };

    match req {
        WsRequest::Triage { trace_id, anchor_ts_ms } => {
            triage_trace(state, &trace_id, anchor_ts_ms).await
        }
        WsRequest::UserErrors { user_id } => {
            match crate::db::queries::user_error_traces(&state.db, &user_id).await {
                Ok(traces) => WsResponse::UserErrors { traces },
                Err(e) => WsResponse::Error { message: e.to_string() },
            }
        }
    }
}

async fn triage_trace(state: &AppState, trace_id: &str, anchor_ts_ms: Option<i64>) -> WsResponse {
    let anchor_ts = anchor_ts_ms
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    // Reconstruct trace chain (includes ts-bounded queries — Gap 1).
    let chain = match chain::reconstruct(&state.db, trace_id, anchor_ts).await {
        Ok(c) => c,
        Err(e) => return WsResponse::Error { message: format!("chain error: {}", e) },
    };

    // Classify failure pattern.
    let classification = classify::classify(&chain);

    // Check fingerprint dedup for the first error row.
    let fingerprint = chain.rows.iter().find_map(|r| r.fingerprint);
    if let Some(fp) = fingerprint {
        match dedup::check(&state.db, fp).await {
            Ok(dedup::DedupResult::Known { repair_id, cached_fix, occurrence_count }) => {
                info!(trace_id, fingerprint = fp, %repair_id, "dedup hit — returning cached fix");
                return WsResponse::TriageResult(json!({
                    "trace_id": trace_id,
                    "dedup": true,
                    "occurrence_count": occurrence_count,
                    "repair_id": repair_id.as_str(),
                    "cached_fix": cached_fix,
                    "classification": classification,
                }));
            }
            Ok(dedup::DedupResult::New) => {}
            Err(e) => warn!(error = %e, "dedup check failed, proceeding to LLM"),
        }
    }

    // Environment snapshot.
    let mut redis_conn = match state.redis.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Redis unavailable, proceeding without env snapshot");
            // Continue without environment snapshot.
            return run_llm_triage(state, &chain, &classification, anchor_ts).await;
        }
    };
    let env = environment::EnvironmentSnapshot::capture(
        &mut redis_conn,
        classification.failing_service,
    )
    .await;

    // Code context (version-pinned).
    let version = std::env::var("SERVICE_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let index_path = std::env::var("CODE_INDEX_PATH").unwrap_or_else(|_| "/index".to_string());
    let query = format!(
        "{} {} {}",
        classification.trigger_event.as_deref().unwrap_or(""),
        chain.rows.iter().find(|r| r.level >= 3).map(|r| r.service.to_string()).unwrap_or_default(),
        classification.notes.first().map(|s| s.as_str()).unwrap_or("")
    );
    let code_ctx = context::retrieve(&query, &version, &index_path, 5)
        .await
        .unwrap_or_else(|_| context::CodeContext {
            version: version.clone(),
            snippets: vec![],
            top_score: 0.0,
        });

    // LLM triage.
    match llm::triage(&chain, &classification, &env, &code_ctx, &state.llm_cfg).await {
        Ok(result) => {
            // Cache the result.
            if let Some(fp) = fingerprint {
                let error_code = chain
                    .rows
                    .iter()
                    .find(|r| r.level >= 3)
                    .map(|r| r.service.to_string())
                    .unwrap_or_default();
                let _ = dedup::record(
                    &state.db,
                    fp,
                    classification.failing_service.unwrap_or(0),
                    classification.trigger_event.as_deref().unwrap_or(""),
                    &error_code,
                    &result.repair_id,
                    Some(&result.summary),
                )
                .await;
            }
            WsResponse::TriageResult(serde_json::to_value(&result).unwrap_or_default())
        }
        Err(e) => WsResponse::Error { message: format!("LLM triage failed: {}", e) },
    }
}

async fn run_llm_triage(
    state: &AppState,
    chain: &chain::TraceChain,
    classification: &classify::Classification,
    anchor_ts: chrono::DateTime<chrono::Utc>,
) -> WsResponse {
    let env = environment::EnvironmentSnapshot {
        kafka_consumer_lag: None,
        kafka_topic: None,
        valkey_gap_keys_count: None,
        error_rate_1m: None,
        services_degraded: vec![],
        snapshot_ts: anchor_ts,
    };
    let code_ctx = context::CodeContext {
        version: "unknown".to_string(),
        snippets: vec![],
        top_score: 0.0,
    };

    match llm::triage(chain, classification, &env, &code_ctx, &state.llm_cfg).await {
        Ok(result) => WsResponse::TriageResult(serde_json::to_value(&result).unwrap_or_default()),
        Err(e) => WsResponse::Error { message: format!("LLM triage failed: {}", e) },
    }
}
