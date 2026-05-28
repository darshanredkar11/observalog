use regex::Regex;
use std::fs;

/// A function/method extracted from a Node.js / TypeScript file.
#[derive(Debug, Clone)]
pub struct NodeFunction {
    pub name:       String,
    pub signature:  String,
    pub body:       String,
    pub line_start: usize,
    pub line_end:   usize,
}

#[derive(Debug, Clone)]
pub struct NodeFile {
    pub path:      String,
    pub content:   String,
    pub functions: Vec<NodeFunction>,
}

pub fn parse_node_file(path: &str) -> Result<NodeFile, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let functions = extract_functions(&content);
    Ok(NodeFile { path: path.to_string(), content, functions })
}

fn extract_functions(content: &str) -> Vec<NodeFunction> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Match function declarations:
    //   function foo(               – standard declaration
    //   async function foo(         – async declaration
    //   export function foo(        – exported declaration
    //   export async function foo(  – exported async declaration
    //   const foo = (...) =>        – arrow function assigned to const
    //   const foo = async (...) =>  – async arrow
    //   foo(...) {                  – method in a class body
    let fn_pat = Regex::new(
        r"(?x)
          ^\s*
          (?:export\s+)?
          (?:async\s+)?
          function\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(
          |
          ^\s*(?:export\s+)?const\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*(?:async\s*)?\(
          |
          ^\s*(?:public\s+|private\s+|protected\s+)?(?:async\s+)?([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\([^)]*\)\s*(?::\s*[\w<>\[\]|&,\s]+)?\s*\{
        "
    ).unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fn_pat.captures(line) {
            // Extract whichever capture group matched
            let name = caps.get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
                .map(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string();

            // Skip control-flow keywords
            if matches!(name.as_str(), "if" | "for" | "while" | "switch" | "catch"
                        | "return" | "constructor" | "class" | "interface" | "type") {
                continue;
            }

            let (body, body_end) = extract_brace_body(&lines, i);
            functions.push(NodeFunction {
                name,
                signature: line.to_string(),
                body,
                line_start: i + 1,
                line_end:   body_end,
            });
        }
    }

    functions
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
    fn test_extract_named_function() {
        let code = r#"
import * as log from 'observalog';

export async function handleSave(req: Request): Promise<void> {
    log.info('doc.storage.saved', 'Saved', { outcome: 'success' });
}
"#;
        let fns = extract_functions(code);
        assert!(fns.iter().any(|f| f.name == "handleSave"), "should find handleSave");
    }

    #[test]
    fn test_extract_arrow_function() {
        let code = r#"
const processEvent = async (event: Event) => {
    log.info('auth.jwt.validated', 'ok', {});
};
"#;
        let fns = extract_functions(code);
        assert!(fns.iter().any(|f| f.name == "processEvent"), "should find processEvent");
    }
}
