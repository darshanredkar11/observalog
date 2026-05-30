use crate::finding::Finding;
use crate::grammar;
use super::walker::JavaMethod;

pub fn check_all_rules(path: &str, methods: &[JavaMethod]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for method in methods {
        findings.extend(check_undeclared_event(path, method));
        findings.extend(check_unstructured_error(path, method));
        findings.extend(check_missing_duration(path, method));
        findings.extend(check_raw_pii_in_log(path, method));
        findings.extend(check_undeclared_abbreviation(path, method));
    }
    findings
}

// ─── Rule: UNDECLARED_EVENT ───────────────────────────────────────────────────
// ObservaLog.info("event", ...) / ObservaLog.error("event", ...)
fn check_undeclared_event(path: &str, method: &JavaMethod) -> Vec<Finding> {
    let mut findings = Vec::new();
    let pat = regex::Regex::new(
        r#"ObservaLog\s*\.\s*(?:info|warn|error|debug)\s*\(\s*"([^"]+)""#
    ).unwrap();

    for caps in pat.captures_iter(&method.body) {
        if let Some(m) = caps.get(1) {
            if let Err(e) = grammar::validate_event(m.as_str()) {
                findings.push(Finding::error(
                    "UNDECLARED_EVENT", path, &method.name,
                    method.line_start, 0, e,
                ));
            }
        }
    }
    findings
}

// ─── Rule: UNSTRUCTURED_ERROR ────────────────────────────────────────────────
// "error", someString   →  must be new Err(...)
fn check_unstructured_error(path: &str, method: &JavaMethod) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Match "error", <value> inside Map.of(...) where value is a string literal
    let pat = regex::Regex::new(
        r#""error"\s*,\s*"([^"]*)"#
    ).unwrap();

    if pat.is_match(&method.body) {
        findings.push(Finding::error(
            "UNSTRUCTURED_ERROR", path, &method.name,
            method.line_start, 0,
            "error field must be new Err(...) struct, not a plain string",
        ));
    }
    findings
}

// ─── Rule: MISSING_DURATION ──────────────────────────────────────────────────
// Outcome.SUCCESS/FAILURE/etc without "duration_ms"
fn check_missing_duration(path: &str, method: &JavaMethod) -> Vec<Finding> {
    let mut findings = Vec::new();
    let outcome_pat = regex::Regex::new(r"Outcome\.(SUCCESS|FAILURE|PARTIAL|PENDING)").unwrap();
    let dur_pat     = regex::Regex::new(r#""duration_ms""#).unwrap();

    if outcome_pat.is_match(&method.body) && !dur_pat.is_match(&method.body) {
        findings.push(Finding::error(
            "MISSING_DURATION", path, &method.name,
            method.line_start, 0,
            "outcome field requires duration_ms field",
        ));
    }
    findings
}

// ─── Rule: RAW_PII_IN_LOG ────────────────────────────────────────────────────
fn check_raw_pii_in_log(path: &str, method: &JavaMethod) -> Vec<Finding> {
    let mut findings = Vec::new();
    let pii_fields = ["email", "phone", "password", "token", "ssn"];

    // Only scan keys inside Map.of(...) arguments that appear in ObservaLog calls.
    // Scanning the whole method body causes false positives on entity fields,
    // JWT claims, HTTP response maps, and test fixtures.
    let map_blocks = extract_map_of_blocks(&method.body);
    if map_blocks.is_empty() {
        return findings;
    }

    let key_pat = regex::Regex::new(r#""([^"]+)"\s*,"#).unwrap();
    let mut reported = std::collections::HashSet::new();

    for block in &map_blocks {
        for caps in key_pat.captures_iter(block) {
            if let Some(m) = caps.get(1) {
                let key = m.as_str();
                if pii_fields.contains(&key) && reported.insert(key.to_string()) {
                    findings.push(Finding::warn(
                        "RAW_PII_IN_LOG", path, &method.name,
                        method.line_start, 0,
                        format!("Potential PII field '{}' in Map.of log context", key),
                    ));
                }
            }
        }
    }
    findings
}

/// Extract the content inside every Map.of(...) block in body.
fn extract_map_of_blocks(body: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search = body;

    while let Some(pos) = search.find("Map.of(") {
        let after_paren = pos + "Map.of(".len();
        let rest = &search[after_paren..];

        let mut depth = 1i32;
        let mut end = rest.len();
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
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
        search = &search[pos + 1..];
    }

    blocks
}

// ─── Rule: UNDECLARED_ABBREVIATION ───────────────────────────────────────────
// Map.of("key", value) — key must be a known field
fn check_undeclared_abbreviation(path: &str, method: &JavaMethod) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Match Map.of("key", ...) key strings
    let pat = regex::Regex::new(r#"Map\.of\([^)]*"([a-z_]+)"\s*,"#).unwrap();

    for caps in pat.captures_iter(&method.body) {
        if let Some(m) = caps.get(1) {
            let key = m.as_str();
            if !is_known_field(key) {
                findings.push(Finding::warn(
                    "UNDECLARED_ABBREVIATION", path, &method.name,
                    method.line_start, 0,
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
        "outcome" | "duration_ms" | "error" | "user_id" | "journey_stage" | "ctx_primary_key"
        | "event" | "message" | "ctx" | "doc_id" | "topic" | "partition"
        | "offset" | "provider" | "http_status"
        | "e" | "m" | "ms" | "c" | "o" | "er" | "ek" | "ec" | "em" | "rt"
        | "di" | "tp" | "pt" | "of" | "pr" | "hs" | "ui" | "js"
        | "method" | "path" | "status" | "ip" | "user_agent" | "latency"
        | "bytes" | "request_id" | "host" | "scheme" | "query"
        | "mt" | "ph" | "st" | "ua" | "lt" | "byt" | "rid"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java::walker::JavaMethod;

    fn make_method(body: &str) -> JavaMethod {
        JavaMethod {
            name: "testMethod".into(),
            signature: "public void testMethod()".into(),
            body: body.to_string(),
            line_start: 1,
            line_end: 10,
        }
    }

    #[test]
    fn test_valid_event_passes() {
        let m = make_method(r#"ObservaLog.info("doc.document.created", "Saved", Map.of());"#);
        let f = check_undeclared_event("Test.java", &m);
        assert!(f.is_empty());
    }

    #[test]
    fn test_invalid_event_caught() {
        let m = make_method(r#"ObservaLog.error("failed", "bad", Map.of());"#);
        let f = check_undeclared_event("Test.java", &m);
        assert!(!f.is_empty());
        assert_eq!(f[0].rule, "UNDECLARED_EVENT");
    }

    #[test]
    fn test_string_error_caught() {
        let m = make_method(r#"ObservaLog.error("doc.send.failed", "bad", Map.of("error", "oops"));"#);
        let f = check_unstructured_error("Test.java", &m);
        assert!(!f.is_empty());
        assert_eq!(f[0].rule, "UNSTRUCTURED_ERROR");
    }

    #[test]
    fn test_typed_error_passes() {
        let m = make_method(r#"ObservaLog.error("doc.send.failed", "bad", Map.of("error", new Err("K","C","msg",true)));"#);
        let f = check_unstructured_error("Test.java", &m);
        assert!(f.is_empty());
    }

    #[test]
    fn test_missing_duration_caught() {
        let m = make_method(r#"ObservaLog.info("auth.jwt.validated", "ok", Map.of("outcome", Outcome.SUCCESS));"#);
        let f = check_missing_duration("Test.java", &m);
        assert!(!f.is_empty());
    }

    #[test]
    fn test_duration_present_passes() {
        let m = make_method(r#"ObservaLog.info("auth.jwt.validated", "ok", Map.of("outcome", Outcome.SUCCESS, "duration_ms", ms));"#);
        let f = check_missing_duration("Test.java", &m);
        assert!(f.is_empty());
    }
}
