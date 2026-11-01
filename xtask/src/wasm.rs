//! Checks that the core crates compile for `wasm32-unknown-unknown` (AGENTS.md "Portability").

use std::path::Path;

use crate::{TaskResult, cmd};

const TARGET: &str = "wasm32-unknown-unknown";

pub(crate) fn check(root: &Path, crates: &[&str], skip: bool) -> TaskResult {
    if skip {
        eprintln!(
            "layers: WARNING: wasm check skipped (--skip-wasm); the core crates are NOT verified \
             to build for {TARGET}"
        );
        return Ok(());
    }
    ensure_target_installed(root)?;

    let mut args = vec!["check", "--target", TARGET];
    for name in crates {
        args.extend(["--package", name]);
    }
    println!("layers: checking core crates for {TARGET}");
    cmd::run_status(&cmd::cargo(), &args, root)
}

/// Fails with installation hints if the wasm standard library is missing.
fn ensure_target_installed(root: &Path) -> TaskResult {
    let libdir = cmd::run_capture(
        &cmd::rustc(),
        &["--print", "target-libdir", "--target", TARGET],
        root,
    )?;
    let libdir = Path::new(libdir.trim());
    let installed = std::fs::read_dir(libdir).is_ok_and(|mut entries| entries.next().is_some());
    if installed {
        Ok(())
    } else {
        Err(format!(
            "the {TARGET} standard library is not installed (looked in {}).\n\
             Install it with `rustup target add {TARGET}` (rustup), or your distribution's \
             package (Arch: `pacman -S rust-wasm`).\n\
             To run the other checks without it: `cargo xtask layers --skip-wasm`.",
            libdir.display()
        ))
    }
}
