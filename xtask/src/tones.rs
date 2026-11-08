//! `cargo xtask tones`: the built-in tone library's import tool.
//!
//! The work lives in `gscore-app` (the `tones` example), where the tone types and
//! `serde_json` already are; this task only forwards the arguments so `xtask` itself stays
//! dependency-free.

use std::path::Path;

use crate::{TaskResult, cmd};

pub(crate) fn run(root: &Path, args: &[String]) -> TaskResult {
    let mut forwarded: Vec<&str> = vec![
        "run",
        "--quiet",
        "--package",
        "gscore-app",
        "--example",
        "tones",
        "--",
    ];
    forwarded.extend(args.iter().map(String::as_str));
    cmd::run_status(&cmd::cargo(), &forwarded, root)
}
