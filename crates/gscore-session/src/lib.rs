//! The open document and everything that keeps the user's work safe on disk: opening files,
//! crash reports (a panic hook that logs diagnostics to a file), safe saving, and later the
//! recovery journal and autosave (milestone M8; AGENTS.md "Never lose work").

mod crash_report;
mod open;
mod save;

pub use crash_report::{default_crash_dir, install_panic_hook};
pub use open::{OpenError, open};
pub use save::{SAVE_FORMATS, SaveError, SaveOptions, SaveReport, backup_path, save};
