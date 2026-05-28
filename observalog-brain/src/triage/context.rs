use anyhow::Result;
use serde::Serialize;
use tracing::debug;

/// Code context retrieved for a triage finding — Decision 11/12.
/// Decision 11: Semble (Model2Vec + BM25 + RRF fusion) does semantic search
/// against a version-pinned code index (exact git SHA from `version` log field).
/// Decision 12: version-pinned code indexing ensures brain reads the actual code
/// that was running at failure time — not grep, semantic search.
#[derive(Debug, Clone, Serialize)]
pub struct CodeContext {
    /// Git SHA the code was indexed at (from `version` log field).
    pub version: String,
    /// Top-k semantically relevant code snippets.
    pub snippets: Vec<CodeSnippet>,
    /// BM25 + Model2Vec + RRF fusion score of best match.
    pub top_score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeSnippet {
    pub file: String,
    pub function: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub score: f32,
}

/// Retrieve semantically relevant code snippets for a triage query.
/// Uses version-pinned index — the exact git SHA from the `version` log field.
///
/// Note: Full Semble (MinishLab/semble) integration is a build-time dependency.
/// This module provides the interface; the underlying index is populated by
/// `semble index --version <git-sha> --dir <service-root>` during CI.
pub async fn retrieve(
    query: &str,
    version: &str,
    index_path: &str,
    top_k: usize,
) -> Result<CodeContext> {
    // Semble integration point.
    // In production: call into the semble index via FFI or subprocess.
    // The index is version-pinned: each git SHA has its own pre-built index.
    debug!(version, query = &query[..50.min(query.len())], "code context retrieval");

    // Stub: return empty context. Real implementation calls semble.
    Ok(CodeContext {
        version: version.to_string(),
        snippets: retrieve_stub(query, top_k),
        top_score: 0.0,
    })
}

fn retrieve_stub(query: &str, top_k: usize) -> Vec<CodeSnippet> {
    // Placeholder until semble crate is integrated.
    // Returns empty — LLM will triage without code context.
    vec![]
}
