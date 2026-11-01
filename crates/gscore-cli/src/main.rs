//! `gscore-cli` binary: thin wrapper around [`gscore_cli::run_cli`].

// The lint checks each target separately; the dependencies are used by the library target.
#![allow(unused_crate_dependencies)]

use std::process::ExitCode;

fn main() -> ExitCode {
    gscore_cli::install_crash_reporting();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match gscore_cli::run_cli(&args) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("Error: {message}");
            ExitCode::FAILURE
        }
    }
}
