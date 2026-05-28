use crate::finding::Finding;
use crate::go::walker::parse_go_file;
use crate::go::rules as go_rules;
use crate::java::walker::parse_java_file;
use crate::java::rules as java_rules;
use crate::node::walker::parse_node_file;
use crate::node::rules as node_rules;
use walkdir::WalkDir;

fn is_go_file(path: &str)   -> bool { path.ends_with(".go") }
fn is_java_file(path: &str) -> bool { path.ends_with(".java") }
fn is_node_file(path: &str) -> bool {
    path.ends_with(".ts")
    || path.ends_with(".tsx")
    || path.ends_with(".js")
    || path.ends_with(".mjs")
    || path.ends_with(".cjs")
}

/// Scan a single file for observalog violations.
pub fn scan_file(path: &str) -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    if is_go_file(path) {
        let file = parse_go_file(path)?;
        return Ok(go_rules::check_all_rules(&file));
    }
    if is_java_file(path) {
        let file = parse_java_file(path)?;
        return Ok(java_rules::check_all_rules(path, &file.methods));
    }
    if is_node_file(path) {
        let file = parse_node_file(path)?;
        return Ok(node_rules::check_all_rules(path, &file.functions));
    }
    Ok(Vec::new())
}

/// Scan a directory recursively for Go, Java, and Node.js/TypeScript files.
pub fn scan_directory(dir: &str) -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path().to_str().unwrap_or("");
        if is_go_file(path) || is_java_file(path) || is_node_file(path) {
            if let Ok(file_findings) = scan_file(path) {
                findings.extend(file_findings);
            }
        }
    }

    Ok(findings)
}

/// Scan specific files (e.g., from git diff).
pub fn scan_files(files: &[String]) -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();
    for file in files {
        if let Ok(file_findings) = scan_file(file) {
            findings.extend(file_findings);
        }
    }
    Ok(findings)
}

/// Determine exit code based on findings.
pub fn exit_code(findings: &[Finding]) -> i32 {
    use crate::finding::Severity;

    let has_errors = findings.iter().any(|f| f.severity == Severity::Error);
    let has_warns  = findings.iter().any(|f| f.severity == Severity::Warn);

    if has_errors {
        1 // Error: block merge
    } else if has_warns {
        2 // Warning: allow merge
    } else {
        0 // All clear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_no_findings() {
        let findings = vec![];
        assert_eq!(exit_code(&findings), 0);
    }

    #[test]
    fn test_unknown_extension_returns_empty() {
        // .py files are not yet supported — should return empty, not error
        let result = scan_file("main.py");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
