use clap::Parser;
use logscanner::{scan_files, scan_directory, exit_code};
use std::io::{self, BufRead};
use std::process;

#[derive(Parser, Debug)]
#[command(name = "logscanner")]
#[command(about = "Static analyzer for observalog logging compliance in Go, Java, and Node.js/TypeScript services")]
struct Args {
    /// One or more files to scan. Accepts multiple space-separated paths:
    ///   logscanner --files a.go b.go c.ts
    /// Or use shell expansion:
    ///   logscanner --files $(git diff --name-only origin/main...HEAD | grep -E '\.(go|java|ts|js)$')
    #[arg(long, num_args = 1..)]
    files: Option<Vec<String>>,

    /// Directory to scan recursively (scans .go, .java, .ts, .tsx, .js, .mjs)
    #[arg(long)]
    dir: Option<String>,

    /// Read file paths from stdin (one per line). Useful with git diff --name-only | logscanner --stdin
    #[arg(long)]
    stdin: bool,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

fn main() {
    let args = Args::parse();

    let findings = if args.stdin {
        // Read file paths from stdin, one per line — works with:
        //   git diff --name-only origin/main...HEAD | grep -E '\.(go|java|ts)$' | logscanner --stdin
        let stdin = io::stdin();
        let files: Vec<String> = stdin
            .lock()
            .lines()
            .map_while(Result::ok)
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        match scan_files(&files) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error scanning files: {}", e);
                process::exit(1);
            }
        }
    } else if let Some(files) = &args.files {
        match scan_files(files) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error scanning files: {}", e);
                process::exit(1);
            }
        }
    } else if let Some(dir) = &args.dir {
        match scan_directory(dir) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error scanning directory: {}", e);
                process::exit(1);
            }
        }
    } else {
        eprintln!("Provide --files <file>..., --dir <path>, or --stdin");
        process::exit(1);
    };

    // Output findings
    match args.format.as_str() {
        "json" => {
            for finding in &findings {
                println!("{}", serde_json::to_string(finding).unwrap());
            }
        }
        "text" => {
            for finding in &findings {
                println!(
                    "{}:{}:{}: {} [{}] {}",
                    finding.file, finding.line, finding.col, finding.rule, finding.severity, finding.message
                );
            }
        }
        _ => {
            eprintln!("Unknown format: {}", args.format);
            process::exit(1);
        }
    }

    // Exit with appropriate code
    process::exit(exit_code(&findings));
}
