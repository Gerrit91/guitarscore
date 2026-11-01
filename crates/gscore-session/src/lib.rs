//! The open document and everything that keeps the user's work safe on disk: crash reports
//! (a panic hook that logs diagnostics to a file), and later safe-save (milestone M3) and
//! the recovery journal and autosave (milestone M7; AGENTS.md "Never lose work").

mod crash_report;

pub use crash_report::{default_crash_dir, install_panic_hook};
