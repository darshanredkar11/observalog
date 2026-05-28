use crate::finding::Finding;
use crate::go::walker::parse_go_file;
use crate::go::rules;
use walkdir::WalkDir;

/// Scan a single file for observalog violations.
pub fn scan_file(path: &str) -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    if !path.ends_with(".go") {
        return Ok(Vec::new());
    }

    let file = parse_go_file(path)?;
    let findings = rules::check_all_rules(&file);

    Ok(findings)
}

/// Scan a directory recursively for Go files.
pub fn scan_directory(dir: &str) -> Result<Vec<Finding>, Box<dyn std::error::Error>> {
    let mut findings = Vec::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("go"))
    {
        if let Ok(file_findings) = scan_file(entry.path().to_str().unwrap_or("")) {
            findings.extend(file_findings);
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
    let has_warns = findings.iter().any(|f| f.severity == Severity::Warn);

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
}
