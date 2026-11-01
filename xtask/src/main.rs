//! Project tasks for GuitarScore, run as `cargo xtask <task>` (alias in `.cargo/config.toml`).

mod cmd;
mod layers;
mod source_scan;
mod wasm;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <task> [options]

Tasks:
  layers [--skip-wasm]  Check crate layering, restricted dependencies, workspace lints,
                        and that the core crates build for wasm32-unknown-unknown
  ci [--skip-wasm]      Run the full definition-of-done: fmt, clippy, test, layers
  corpus                Run the opt-in corpus tests (not implemented yet)";

/// Error type of every task: a message for the person running it.
type TaskResult = Result<(), String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("xtask: error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> TaskResult {
    let root = workspace_root()?;
    let (task, rest) = match args.split_first() {
        Some((task, rest)) => (task.as_str(), rest),
        None => return Err(format!("no task given\n\n{USAGE}")),
    };
    match task {
        "layers" => layers::run(&root, parse_skip_wasm(rest)?),
        "ci" => ci(&root, parse_skip_wasm(rest)?),
        "corpus" => Err("`corpus` is not implemented yet (planned for M2)".to_owned()),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown task `{other}`\n\n{USAGE}")),
    }
}

/// Parses the only option the tasks take. Anything else is an error, never ignored.
fn parse_skip_wasm(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--skip-wasm" => Ok(true),
        other => Err(format!("unexpected arguments {other:?}\n\n{USAGE}")),
    }
}

/// The workspace root is the parent of this crate's directory.
fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{} has no parent directory", manifest_dir.display()))
}

/// The definition of done from AGENTS.md section 6, in order, stopping at the first failure.
fn ci(root: &std::path::Path, skip_wasm: bool) -> TaskResult {
    let steps: &[&[&str]] = &[
        &["fmt", "--all", "--", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        &["test", "--workspace"],
    ];
    for step in steps {
        println!("==> cargo {}", step.join(" "));
        cmd::run_status(&cmd::cargo(), step, root)?;
    }
    println!("==> cargo xtask layers");
    layers::run(root, skip_wasm)
}
