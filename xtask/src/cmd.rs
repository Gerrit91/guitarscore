//! Running external programs (`cargo`, `rustc`) and turning failures into messages.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// The `cargo` that invoked us (set by cargo for `cargo run`), falling back to `PATH`.
pub(crate) fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// The `rustc` cargo would use, falling back to `PATH`.
pub(crate) fn rustc() -> OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

/// Runs `program args` in `dir` and returns its stdout. Fails if it cannot start or exits non-zero.
pub(crate) fn run_capture(program: &OsString, args: &[&str], dir: &Path) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", describe(program, args)))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed ({}):\n{}",
            describe(program, args),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| format!("{} printed invalid UTF-8: {e}", describe(program, args)))
}

/// Runs `program args` in `dir` with inherited stdio. Fails if it cannot start or exits non-zero.
pub(crate) fn run_status(program: &OsString, args: &[&str], dir: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("cannot run {}: {e}", describe(program, args)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} failed ({status})", describe(program, args)))
    }
}

fn describe(program: &OsString, args: &[&str]) -> String {
    format!("`{} {}`", program.to_string_lossy(), args.join(" "))
}
