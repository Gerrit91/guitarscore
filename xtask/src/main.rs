//! Project tasks for GuitarScore, run as `cargo xtask <task>` (alias in `.cargo/config.toml`).

mod cdp;
mod cmd;
mod layers;
mod sha256;
mod source_scan;
mod wasm;
mod web;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <task> [options]

Tasks:
  layers [--skip-wasm]  Check crate layering, restricted dependencies, workspace lints,
                        and that the core crates build for wasm32-unknown-unknown
  ci [--skip-wasm]      Run the full definition-of-done: fmt, clippy, test, layers
  bench [FILE...]       Time opening, layout and editing in a release build and check the
                        performance budgets. Files: the arguments, else GSCORE_BENCH_FILES
                        (separated by ':'), else the files in testdata/basic
  web [--out DIR] [--debug] [--no-wasm-opt] [--smoke]
      [--soundfont PATH | --no-soundfont]
                        Build the browser app (WebAssembly) and assemble the static site
                        (index.html, guitarscore.js, guitarscore_bg.wasm, assets/,
                        manifest.json, sw.js) in dist/web. Ships GeneralUser GS 2.0.3
                        (downloaded once, checked) unless --soundfont PATH or
                        --no-soundfont is given; --smoke checks it in headless Chromium
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
        "bench" => bench(&root, rest),
        "web" => web::run(&root, rest),
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

/// `gscore-cli bench --check` in a release build on the given files, the files in
/// `GSCORE_BENCH_FILES`, or the test files.
fn bench(root: &std::path::Path, files: &[String]) -> TaskResult {
    let mut files: Vec<String> = files.to_vec();
    if files.is_empty()
        && let Some(listed) = std::env::var_os("GSCORE_BENCH_FILES")
    {
        files = std::env::split_paths(&listed)
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty())
            .collect();
    }
    if files.is_empty() {
        let dir = root.join("testdata/basic");
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        files = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "gp"))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        files.sort();
    }
    let mut args = vec![
        "run",
        "--release",
        "--quiet",
        "--package",
        "gscore-cli",
        "--",
        "bench",
        "--check",
    ];
    args.extend(files.iter().map(String::as_str));
    cmd::run_status(&cmd::cargo(), &args, root)
}
