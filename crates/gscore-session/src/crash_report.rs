//! Panic hook that writes a crash report (message, location, thread, backtrace) to a log file.
//!
//! This is the last line of defence from AGENTS.md "Containment": it records diagnostics so a
//! crash can be fixed, and then hands over to the previous hook (which prints to stderr).
//! The hook itself must never panic, so every fallible step is handled and ignored on failure.

use std::backtrace::Backtrace;
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Application directory name used inside the platform's state/log directory.
const APP_DIR: &str = "guitarscore";

/// Installs a panic hook that writes one crash report file per panic into `dir`.
///
/// The previously installed hook (by default: print the message to stderr) still runs
/// afterwards. Call this once, early in `main`.
pub fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format_report(info, &Backtrace::force_capture());
        match write_report(&dir, &report) {
            // Also runs for panics that are caught and contained (for example in the file
            // reader), so this reports an internal error rather than claiming a crash.
            Ok(path) => note(&format!(
                "internal error, report written to {}",
                path.display()
            )),
            Err(e) => note(&format!(
                "internal error, could not write a report to {}: {e}",
                dir.display()
            )),
        }
        previous(info);
    }));
}

/// The per-user directory for crash reports, or `None` if the environment does not say
/// where the home directory is.
///
/// Linux and other Unix: `$XDG_STATE_HOME/guitarscore/crashes`, defaulting to
/// `~/.local/state/guitarscore/crashes`. macOS: `~/Library/Logs/guitarscore`.
/// Windows: `%LOCALAPPDATA%\guitarscore\crashes`.
pub fn default_crash_dir() -> Option<PathBuf> {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        env_dir("LOCALAPPDATA").map(|d| d.join(APP_DIR).join("crashes"))
    } else if cfg!(target_os = "macos") {
        env_dir("HOME").map(|d| d.join("Library").join("Logs").join(APP_DIR))
    } else {
        env_dir("XDG_STATE_HOME")
            .or_else(|| env_dir("HOME").map(|h| h.join(".local").join("state")))
            .map(|d| d.join(APP_DIR).join("crashes"))
    }
}

/// Renders the report text. Separate from I/O so it can be tested.
fn format_report(info: &PanicHookInfo<'_>, backtrace: &Backtrace) -> String {
    let message = info
        .payload_as_str()
        .unwrap_or("<non-string panic payload>");
    let location = info.location().map_or_else(
        || "<unknown>".to_owned(),
        |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
    );
    let thread = std::thread::current();
    format!(
        "GuitarScore crash report\n\
         version: {}\n\
         time (unix seconds): {}\n\
         thread: {}\n\
         location: {location}\n\
         message: {message}\n\
         \n\
         backtrace:\n{backtrace}\n",
        gscore_version::VERSION,
        unix_time().as_secs(),
        thread.name().unwrap_or("<unnamed>"),
    )
}

/// Writes `report` to a new file in `dir` (created if needed) and returns its path.
///
/// Writes to a temporary name first and renames, so a half-written report is never left
/// under the final name.
fn write_report(dir: &Path, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let now = unix_time();
    let name = format!(
        "crash-{}-{:09}-{}.log",
        now.as_secs(),
        now.subsec_nanos(),
        std::process::id()
    );
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.tmp"));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(report.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Time since the Unix epoch; zero if the clock is before 1970 (never fails).
fn unix_time() -> std::time::Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// Prints to stderr without panicking if stderr is closed (unlike `eprintln!`).
fn note(message: &str) {
    let _ = writeln!(std::io::stderr(), "guitarscore: {message}");
}
