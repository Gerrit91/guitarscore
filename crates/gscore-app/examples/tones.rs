//! `cargo xtask tones`: the built-in tone library's import tool. The work lives in
//! `gscore_app::tone_tool`, next to the types it edits; the xtask only shells out to this
//! example so it stays dependency-free.

// The example only uses `gscore-app`; the crate's other dependencies are not its business.
#![allow(unused_crate_dependencies)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match gscore_app::tone_tool::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tones: {message}");
            ExitCode::FAILURE
        }
    }
}
