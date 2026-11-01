//! The open document and everything that keeps the user's work safe on disk: crash reports
//! (a panic hook that logs diagnostics to a file), and later the recovery journal, autosave
//! and safe-save (AGENTS.md "Never lose work", milestones M5 and M6).

mod crash_report;

pub use crash_report::{default_crash_dir, install_panic_hook};
