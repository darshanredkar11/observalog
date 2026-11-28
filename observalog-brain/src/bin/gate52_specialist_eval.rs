use serde_json::Value;
use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read input: {error}");
        std::process::exit(2);
    }
    let events: Vec<Value> = match serde_json::from_str(&input) {
        Ok(events) => events,
        Err(error) => {
            eprintln!("invalid event JSON: {error}");
            std::process::exit(2);
        }
    };
    let hypotheses = observalog_brain::lite::gate52_specialists::analyze(&events);
    if let Err(error) = serde_json::to_writer(io::stdout(), &hypotheses) {
        eprintln!("failed to write hypotheses: {error}");
        std::process::exit(2);
    }
}
