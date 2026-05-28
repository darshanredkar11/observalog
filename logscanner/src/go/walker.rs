use regex::Regex;
use std::fs;

#[derive(Debug, Clone)]
pub struct GoFunction {
    pub name: String,
    pub signature: String,
    pub body: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone)]
pub struct GoFile {
    pub path: String,
    pub content: String,
    pub functions: Vec<GoFunction>,
}

/// Parse a Go file and extract function definitions.
pub fn parse_go_file(path: &str) -> Result<GoFile, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let functions = extract_functions(&content);

    Ok(GoFile {
        path: path.to_string(),
        content,
        functions,
    })
}

fn extract_functions(content: &str) -> Vec<GoFunction> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Simple regex to find function signatures: func Name(...) or func (r Receiver) Name(...)
    let func_pattern = Regex::new(r"^\s*func\s+(\(.*?\))?\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = func_pattern.captures(line) {
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unknown").to_string();

            // Find function body (from line i until next function or end of file)
            let mut body_end = i + 1;
            let mut brace_count = 0;
            let mut in_body = false;

            for (j, l) in lines.iter().enumerate().skip(i) {
                for ch in l.chars() {
                    match ch {
                        '{' => {
                            in_body = true;
                            brace_count += 1;
                        }
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
                if in_body && brace_count == 0 {
                    break;
                }
            }

            let signature = line.to_string();
            let body = lines[i..body_end.min(lines.len())].join("\n");

            functions.push(GoFunction {
                name,
                signature,
                body,
                line_start: i + 1,
                line_end: body_end,
            });
        }
    }

    functions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let code = r#"
func Hello() {
    fmt.Println("hello")
}
"#;
        let funcs = extract_functions(code);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "Hello");
    }
}
