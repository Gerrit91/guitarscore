//! `gui`: start the desktop application (`guitarscore`) as its own process.
//!
//! The CLI does not link the GUI (both are top-layer crates and must not depend on each
//! other; the CLI stays free of GUI libraries). It looks for the `guitarscore` executable
//! next to its own, then on `PATH`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::CliError;

/// The application's executable name.
const APP: &str = "guitarscore";

/// Starts the app, opening `file` if given, and returns without waiting for it.
pub(crate) fn run(file: Option<&Path>) -> Result<String, CliError> {
    let program = app_path();
    let mut command = Command::new(&program);
    if let Some(file) = file {
        if !file.exists() {
            return Err(CliError::Failed(format!(
                "{} does not exist",
                file.display()
            )));
        }
        command.arg(file);
    }
    let child = command.spawn().map_err(|e| {
        CliError::Failed(format!(
            "cannot start {}: {e}. Build it with `cargo build -p gscore-app` (or start it \
             directly with `cargo gui`).",
            program.display()
        ))
    })?;
    Ok(format!("Started GuitarScore (process {}).\n", child.id()))
}

/// `guitarscore` next to this executable if it exists there, otherwise the bare name (the
/// system then searches `PATH`).
fn app_path() -> PathBuf {
    let name = format!("{APP}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}
