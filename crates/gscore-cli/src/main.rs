//! `gscore-cli` binary: thin wrapper around [`gscore_cli::run_cli`].

// The lint checks each target separately; the dependencies are used by the library target.
#![allow(unused_crate_dependencies)]

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    gscore_cli::install_crash_reporting();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match gscore_cli::run_cli(&args) {
        Ok(output) => {
            // Ignore write errors (e.g. a closed pipe in `gscore-cli info x | head`).
            let _ = std::io::stdout().write_all(output.as_bytes());
            ExitCode::SUCCESS
        }
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "Error: {err}");
            ExitCode::from(err.exit_code())
        }
    }
}
