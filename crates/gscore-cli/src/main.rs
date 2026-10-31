use gscore_cli::run_cli;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(e) = run_cli(args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}