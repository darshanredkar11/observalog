use crate::finding::Finding;
use crate::grammar;
use super::walker::NodeFunction;

pub fn check_all_rules(path: &str, functions: &[NodeFunction]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for func in functions {
        findings.extend(check_undeclared_event(path, func));
        findings.extend(check_unstructured_error(path, func));
        findings.extend(check_missing_duration(path, func));
        findings.extend(check_raw_pii_in_log(path, func));
        findings.extend(check_undeclared_abbreviation(path, func));
    }
    findings
}

// ─── Rule: UNDECLARED_EVENT ───────────────────────────────────────────────────
// log.info("event", ...) / log.error("event", ...) / ObservaLog.info(...)
fn check_undeclared_event(path: &str, func: &NodeFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Matches: log.info("event", ...) or ObservaLog.info("event", ...)
    let pat = regex::Regex::new(
        r#"(?:log|ObservaLog|logger)\s*\.\s*(?:info|warn|error|debug)\s*\(\s*['"`]([^'"`]+)['"`]"#
    ).unwrap();

    for caps in pat.captures_iter(&func.body) {
        if let Some(m) = caps.get(1) {
            if let Err(e) = grammar::validate_event(m.as_str()) {
                findings.push(Finding::error(
                    "UNDECLARED_EVENT", path, &func.name,
                    func.line_start, 0, e,
                ));
            }
        }
    }
    findings
}

// ─── Rule: UNSTRUCTURED_ERROR ────────────────────────────────────────────────
// { error: "some string" } — must be { error: new Err(...) } or { error: { kind, code, ... } }
fn check_unstructured_error(path: &str, func: &NodeFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Matches: error: "some literal string"
    let pat = regex::Regex::new(
        r#"error\s*:\s*['"`][^'"`]*['"`]"#
    ).unwrap();

    if pat.is_match(&func.body) {
        findings.push(Finding::error(
            "UNSTRUCTURED_ERROR", path, &func.name,
            func.line_start, 0,
            "error field must be an Err instance or { kind, code, message, retryable } object, not a plain string",
        ));
    }
    findings
}

// ─── Rule: MISSING_DURATION ──────────────────────────────────────────────────
// { outcome: Outcome.Success } / { outcome: 'success' } without duration_ms
fn check_missing_duration(path: &str, func: &NodeFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Match outcome field in a log call
    let outcome_pat = regex::Regex::new(
        r#"outcome\s*:\s*(?:Outcome\.\w+|['"`](?:success|failure|partial|pending)['"`])"#
    ).unwrap();
    let dur_pat = regex::Regex::new(r#"duration_ms\s*:"#).unwrap();

    if outcome_pat.is_match(&func.body) && !dur_pat.is_match(&func.body) {
        findings.push(Finding::error(
            "MISSING_DURATION", path, &func.name,
            func.line_start, 0,
            "outcome field requires duration_ms field",
        ));
    }
    findings
}

// ─── Rule: RAW_PII_IN_LOG ────────────────────────────────────────────────────
fn check_raw_pii_in_log(path: &str, func: &NodeFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    let pii_fields = ["email", "phone", "password", "token", "ssn"];

    for pii in &pii_fields {
        // Match as an object key: email: or 'email': or "email":
        let pattern = format!(r#"(?:['"`]?{}['"`]?\s*:)"#, pii);
        if let Ok(re) = regex::Regex::new(&pattern) {
            if re.is_match(&func.body) {
                findings.push(Finding::warn(
                    "RAW_PII_IN_LOG", path, &func.name,
                    func.line_start, 0,
                    format!("Potential PII field '{}' in log context", pii),
                ));
            }
        }
    }
    findings
}

// ─── Rule: UNDECLARED_ABBREVIATION ───────────────────────────────────────────
// { someKey: value } passed to log.info — key must be a known field
fn check_undeclared_abbreviation(path: &str, func: &NodeFunction) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Match keys in the fields object passed to log calls:
    //   log.info('evt', 'msg', { key: val, ... })
    // We look for identifiers or quoted strings used as object keys followed by ':'
    // inside what appears to be a log call context.
    let log_call_pat = regex::Regex::new(
        r#"(?:log|ObservaLog|logger)\s*\.\s*(?:info|warn|error|debug)\s*\([^{]*\{([^}]*)\}"#
    ).unwrap();

    let key_pat = regex::Regex::new(r#"['"`]?([a-zA-Z_][a-zA-Z0-9_]*)['"`]?\s*:"#).unwrap();

    for call_caps in log_call_pat.captures_iter(&func.body) {
        if let Some(fields_match) = call_caps.get(1) {
            for key_caps in key_pat.captures_iter(fields_match.as_str()) {
                if let Some(key_m) = key_caps.get(1) {
                    let key = key_m.as_str();
                    if !is_known_field(key) {
                        findings.push(Finding::warn(
                            "UNDECLARED_ABBREVIATION", path, &func.name,
                            func.line_start, 0,
                            format!("Field '{}' not in abbreviation dictionary", key),
                        ));
                    }
                }
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
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::walker::NodeFunction;

    fn make_func(body: &str) -> NodeFunction {
        NodeFunction {
            name: "testFn".into(),
            signature: "async function testFn()".into(),
            body: body.to_string(),
            line_start: 1,
            line_end: 10,
        }
    }

    #[test]
    fn test_valid_event_passes() {
        let f = make_func(r#"log.info('doc.document.created', 'Saved', {});"#);
        let findings = check_undeclared_event("test.ts", &f);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_invalid_event_caught() {
        let f = make_func(r#"log.info('failed', 'bad', {});"#);
        let findings = check_undeclared_event("test.ts", &f);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule, "UNDECLARED_EVENT");
    }

    #[test]
    fn test_string_error_caught() {
        let f = make_func(r#"log.error('doc.send.failed', 'bad', { error: 'oops' });"#);
        let findings = check_unstructured_error("test.ts", &f);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule, "UNSTRUCTURED_ERROR");
    }

    #[test]
    fn test_structured_error_passes() {
        let f = make_func(r#"log.error('doc.send.failed', 'bad', { error: new Err('K','C','msg',true) });"#);
        let findings = check_unstructured_error("test.ts", &f);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_missing_duration_caught() {
        let f = make_func(r#"log.info('auth.jwt.validated', 'ok', { outcome: Outcome.Success });"#);
        let findings = check_missing_duration("test.ts", &f);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_duration_present_passes() {
        let f = make_func(r#"log.info('auth.jwt.validated', 'ok', { outcome: Outcome.Success, duration_ms: elapsed });"#);
        let findings = check_missing_duration("test.ts", &f);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_pii_caught() {
        let f = make_func(r#"log.info('user.created', 'ok', { email: user.email });"#);
        let findings = check_raw_pii_in_log("test.ts", &f);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule, "RAW_PII_IN_LOG");
    }
}
