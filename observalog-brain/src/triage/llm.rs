use super::{
    chain::TraceChain,
    classify::Classification,
    context::CodeContext,
    environment::EnvironmentSnapshot,
    repair::RepairId,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, warn};

/// The full triage output for one failure event.
#[derive(Debug, Clone, Serialize)]
pub struct TriageResult {
    pub repair_id: RepairId,
    pub confidence: f32,
    pub summary: String,
    pub root_cause: String,
    pub fix_steps: Vec<String>,
    pub requires_escalation: bool,
    /// Call graph JSON from Decision 12.
    pub call_graph: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct LlmResponse {
    repair_id: String,
    confidence: f32,
    summary: String,
    root_cause: String,
    fix_steps: Vec<String>,
}

/// Anthropic API config.
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl LlmConfig {
    pub fn from_env() -> Result<Self> {
        Ok(LlmConfig {
            api_key: std::env::var("ANTHROPIC_API_KEY")
                .context("ANTHROPIC_API_KEY not set")?,
            model: std::env::var("BRAIN_LLM_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".to_string()),
            max_tokens: std::env::var("BRAIN_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024),
        })
    }
}

/// Run LLM triage for a failure trace.
pub async fn triage(
    chain: &TraceChain,
    classification: &Classification,
    env: &EnvironmentSnapshot,
    code_ctx: &CodeContext,
    cfg: &LlmConfig,
) -> Result<TriageResult> {
    let call_graph = super::chain::build_call_graph(chain)
        .into_iter()
        .map(|n| serde_json::to_value(n).unwrap_or_default())
        .collect::<Vec<_>>();

    let prompt = build_prompt(chain, classification, env, code_ctx, &call_graph);

    debug!(
        trace_id = %chain.trace_id,
        model = %cfg.model,
        "LLM triage request"
    );

    let response = call_anthropic(&prompt, cfg).await?;
    let repair_id = RepairId::from_str(&response.repair_id);

    Ok(TriageResult {
        requires_escalation: repair_id.requires_escalation(),
        repair_id,
        confidence: response.confidence,
        summary: response.summary,
        root_cause: response.root_cause,
        fix_steps: response.fix_steps,
        call_graph,
    })
}

fn build_prompt(
    chain: &TraceChain,
    classification: &Classification,
    env: &EnvironmentSnapshot,
    code_ctx: &CodeContext,
    call_graph: &[serde_json::Value],
) -> String {
    let repair_ids = [
        "NETWORK_RETRY", "RATE_LIMIT_BACKOFF", "VALIDATION_FIX",
        "DATABASE_FIX", "AUTH_REFRESH", "DEPENDENCY_FALLBACK",
        "CONFIG_FIX", "RESOURCE_EXHAUSTED", "CONSISTENCY_FIX", "UNKNOWN",
    ];

    let env_json = serde_json::to_string_pretty(env).unwrap_or_default();
    let class_json = serde_json::to_string_pretty(classification).unwrap_or_default();

    let code_section = if code_ctx.snippets.is_empty() {
        "No code context available.".to_string()
    } else {
        code_ctx
            .snippets
            .iter()
            .map(|s| format!("// {}:{}-{}\n{}", s.file, s.start_line, s.end_line, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        r#"You are a distributed systems triage engine. Analyze this failure trace and produce a structured repair recommendation.

## Failure Classification
{class_json}

## Environment Snapshot
{env_json}

## Call Graph
{call_graph}

## Code Context (version: {version})
{code_section}

## Instructions
Respond with valid JSON only. Schema:
{{
  "repair_id": "<one of: {repair_ids}>",
  "confidence": <0.0-1.0>,
  "summary": "<one sentence>",
  "root_cause": "<technical explanation>",
  "fix_steps": ["<step 1>", "<step 2>"]
}}

repair_id choices: {repair_ids_str}
"#,
        class_json = class_json,
        env_json = env_json,
        call_graph = serde_json::to_string_pretty(call_graph).unwrap_or_default(),
        version = code_ctx.version,
        code_section = code_section,
        repair_ids = repair_ids.join(", "),
        repair_ids_str = repair_ids.join(" | "),
    )
}

async fn call_anthropic(prompt: &str, cfg: &LlmConfig) -> Result<LlmResponse> {
    let client = reqwest::Client::new();

    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "messages": [{
            "role": "user",
            "content": prompt
        }]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Anthropic API request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API error {}: {}", status, &text[..200.min(text.len())]);
    }

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .context("Anthropic API response parse failed")?;

    let content = resp_json["content"][0]["text"]
        .as_str()
        .context("No text content in Anthropic response")?;

    // Strip markdown code fences if present.
    let json_str = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let result: LlmResponse = serde_json::from_str(json_str)
        .with_context(|| format!("LLM response JSON parse failed: {:?}", &json_str[..200.min(json_str.len())]))?;

    Ok(result)
}
