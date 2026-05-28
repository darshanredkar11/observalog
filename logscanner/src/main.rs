use clap::Parser;
use logscanner::{scan_files, scan_directory, exit_code};
use std::process;

#[derive(Parser, Debug)]
#[command(name = "logscanner")]
#[command(about = "Static analyzer for observalog logging compliance in Go, Java, and Node.js/TypeScript services")]
struct Args {
    /// Files to scan (for CI mode)
    #[arg(long)]
    files: Option<Vec<String>>,

    /// Directory to scan recursively
    #[arg(long)]
    dir: Option<String>,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

fn main() {
    let args = Args::parse();

    let findings = if let Some(files) = &args.files {
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
        eprintln!("Provide --files or --dir");
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
