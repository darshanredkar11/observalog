use regex::Regex;
use std::fs;

/// A method extracted from a Java file.
#[derive(Debug, Clone)]
pub struct JavaMethod {
    pub name:       String,
    pub signature:  String,
    pub body:       String,
    pub line_start: usize,
    pub line_end:   usize,
}

#[derive(Debug, Clone)]
pub struct JavaFile {
    pub path:    String,
    pub content: String,
    pub methods: Vec<JavaMethod>,
}

pub fn parse_java_file(path: &str) -> Result<JavaFile, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let methods = extract_methods(&content);
    Ok(JavaFile { path: path.to_string(), content, methods })
}

fn extract_methods(content: &str) -> Vec<JavaMethod> {
    let mut methods = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Match method signatures: optional modifiers + return type + methodName(
    let method_pattern = Regex::new(
        r"^\s*(?:(?:public|private|protected|static|final|synchronized|abstract|override)\s+)*\w[\w<>\[\],\s]*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
    ).unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = method_pattern.captures(line) {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("unknown").to_string();
            // Skip keywords that look like methods
            if matches!(name.as_str(), "if" | "for" | "while" | "switch" | "catch" | "return") {
                continue;
            }

            let (body, body_end) = extract_brace_body(&lines, i);
            methods.push(JavaMethod {
                name,
                signature: line.to_string(),
                body,
                line_start: i + 1,
                line_end:   body_end,
            });
        }
    }

    methods
}

fn extract_brace_body(lines: &[&str], start: usize) -> (String, usize) {
    let mut brace_count = 0i32;
    let mut in_body = false;
    let mut body_end = start + 1;

    for (j, l) in lines.iter().enumerate().skip(start) {
        for ch in l.chars() {
            match ch {
                '{' => { in_body = true; brace_count += 1; }
                '}' => {
                    brace_count -= 1;
                    if in_body && brace_count == 0 {
                        body_end = j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if in_body && brace_count == 0 { break; }
    }

    let body = lines[start..body_end.min(lines.len())].join("\n");
    (body, body_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_java_method() {
        let code = r#"
public class Svc {
    public void handleRequest() {
        ObservaLog.info("doc.storage.saved", "Saved", Map.of());
    }
}
"#;
        let methods = extract_methods(code);
        assert!(methods.iter().any(|m| m.name == "handleRequest"));
    }
}
