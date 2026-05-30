use crate::finding::Finding;
use crate::go::{GoFile, GoFunction, FunctionClass, classify_function};
use crate::grammar;

pub fn check_all_rules(file: &GoFile) -> Vec<Finding> {
    let mut findings = Vec::new();

    for func in &file.functions {
        let class = classify_function(&func.signature, &func.body);

        // Check rules
        findings.extend(check_undeclared_event(&file.path, func));
        findings.extend(check_missing_exit_log(&file.path, func, class));
        findings.extend(check_unstructured_error(&file.path, func));
        findings.extend(check_missing_duration(&file.path, func));
        findings.extend(check_missing_outcome(&file.path, func));
        findings.extend(check_raw_pii_in_log(&file.path, func));
        findings.extend(check_undeclared_abbreviation(&file.path, func));
    }

    findings
}

fn check_undeclared_event(path: &str, func: &GoFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    let event_pattern = regex::Regex::new(r#"log\.(Info|Warn|Error|Debug)\([^,]+,\s*"([^"]+)""#).unwrap();

    for caps in event_pattern.captures_iter(&func.body) {
        if let Some(event_match) = caps.get(2) {
            let event = event_match.as_str();
            if let Err(err) = grammar::validate_event(event) {
                findings.push(Finding::error(
                    "UNDECLARED_EVENT",
                    path,
                    &func.name,
                    func.line_start,
                    0,
                    err,
                ));
            }
        }
    }

    findings
}

fn check_missing_exit_log(_path: &str, _func: &GoFunction, _class: FunctionClass) -> Vec<Finding> {
    // TODO: Implement AST analysis for return paths
    Vec::new()
}

fn check_unstructured_error(path: &str, func: &GoFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Check for Error logs without log.Err struct
    let error_call = regex::Regex::new(r#"log\.Error\([^)]*"error":\s*([^,}]+)"#).unwrap();

    for caps in error_call.captures_iter(&func.body) {
        if let Some(value_match) = caps.get(1) {
            let value = value_match.as_str();
            // If it's a string (quoted), it's unstructured
            if value.starts_with('"') && !value.contains("&log.Err") {
                findings.push(Finding::error(
                    "UNSTRUCTURED_ERROR",
                    path,
                    &func.name,
                    func.line_start,
                    0,
                    "error field must be &log.Err struct, not string",
                ));
            }
        }
    }

    findings
}

fn check_missing_duration(path: &str, func: &GoFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Check for outcome without duration_ms
    let outcome_pattern = regex::Regex::new(r#"log\.(Info|Warn|Error|Debug)\([^)]*"outcome":[^)]*\)"#).unwrap();
    let duration_pattern = regex::Regex::new(r#""duration_ms""#).unwrap();

    for _ in outcome_pattern.captures_iter(&func.body) {
        // Simple check: if outcome is present but duration_ms is not in function body
        if !duration_pattern.is_match(&func.body) {
            findings.push(Finding::error(
                "MISSING_DURATION",
                path,
                &func.name,
                func.line_start,
                0,
                "outcome field requires duration_ms field",
            ));
            break; // Only report once per function
        }
    }

    findings
}

fn check_missing_outcome(_path: &str, _func: &GoFunction) -> Vec<Finding> {
    // TODO: Implement branch analysis for decision points
    Vec::new()
}

fn check_raw_pii_in_log(path: &str, func: &GoFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    let pii_fields = ["email", "phone", "password", "token", "ssn"];

    // Only scan keys inside log.F{...} blocks.
    // Scanning the whole function body causes false positives on JWT claim maps,
    // gin.H{} response bodies, struct tags, and test fixtures — all of which
    // legitimately reference "email", "token", etc. without logging them.
    let log_f_blocks = extract_log_f_blocks(&func.body);
    if log_f_blocks.is_empty() {
        return findings;
    }

    let key_pat = regex::Regex::new(r#""([^"]+)"\s*:"#).unwrap();
    let mut reported = std::collections::HashSet::new();

    for block in &log_f_blocks {
        for caps in key_pat.captures_iter(block) {
            if let Some(m) = caps.get(1) {
                let key = m.as_str();
                if pii_fields.contains(&key) && reported.insert(key.to_string()) {
                    findings.push(Finding::warn(
                        "RAW_PII_IN_LOG",
                        path,
                        &func.name,
                        func.line_start,
                        0,
                        format!("Potential PII field '{}' in log.F context", key),
                    ));
                }
            }
        }
    }

    findings
}

/// Extract the content inside every log.F{...} block in body.
/// Uses brace counting so multi-line blocks are handled correctly.
fn extract_log_f_blocks(body: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search = body;

    while let Some(pos) = search.find("log.F{") {
        let after_brace = pos + "log.F{".len();
        let rest = &search[after_brace..];

        let mut depth = 1i32;
        let mut end = rest.len(); // fallback: unclosed brace
        for (i, ch) in rest.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        blocks.push(rest[..end].to_string());

        // Advance past current "log.F{" to find the next occurrence.
        search = &search[pos + 1..];
    }

    blocks
}

fn check_undeclared_abbreviation(path: &str, func: &GoFunction) -> Vec<Finding> {
    let mut findings = Vec::new();

    let ctx_pattern = regex::Regex::new(r#"log\.F\{\s*"([^"]+)""#).unwrap();

    for caps in ctx_pattern.captures_iter(&func.body) {
        if let Some(key_match) = caps.get(1) {
            let key = key_match.as_str();
            if !is_known_field(key) {
                findings.push(Finding::warn(
                    "UNDECLARED_ABBREVIATION",
                    path,
                    &func.name,
                    func.line_start,
                    0,
                    format!("Field '{}' not in abbreviation dictionary", key),
                ));
            }
        }
    }

    findings
}

fn is_known_field(key: &str) -> bool {
    matches!(
        key,
        // ── Structural fields (injected by library or always meaningful) ──
        "outcome" | "duration_ms" | "error" | "user_id" | "journey_stage" | "ctx_primary_key"
        // ── Dict full names (library abbreviates these in Part B) ──
        | "event" | "message" | "ctx" | "doc_id" | "topic" | "partition"
        | "offset" | "provider" | "http_status"
        // ── Dict abbreviated wire keys ──
        | "e" | "m" | "ms" | "c" | "o" | "er" | "ek" | "ec" | "em" | "rt"
        | "di" | "tp" | "pt" | "of" | "pr" | "hs" | "ui" | "js"
        // ── Common HTTP observability fields ──
        // These appear in request-logging middleware in virtually every web service.
        | "method" | "path" | "status" | "ip" | "user_agent" | "latency"
        | "bytes" | "request_id" | "host" | "scheme" | "query"
        // abbreviated forms
        | "mt" | "ph" | "st" | "ua" | "lt" | "byt" | "rid"
    )
}
