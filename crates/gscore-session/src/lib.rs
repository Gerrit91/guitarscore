//! The open document and everything that keeps the user's work safe on disk: opening files,
//! crash reports (a panic hook that logs diagnostics to a file), safe saving, the user's
//! settings files (the keymap), and the recovery journal with crash recovery (AGENTS.md
//! "Never lose work").

mod config;
mod crash_report;
mod open;
mod recovery;
mod save;

pub use config::{KEYMAP_FILE, LoadedKeymap, default_config_dir, load_keymap, load_user_keymap};
pub use crash_report::{default_crash_dir, install_panic_hook};
pub use gscore_io::FileFormat;
pub use open::{OpenError, open, open_with_bytes};
pub use recovery::{
    FoundSession, Journal, Recovered, SNAPSHOT_EVERY, SNAPSHOT_INTERVAL, default_recovery_dir,
    discard, find_sessions, recover,
};
pub use save::{SAVE_FORMATS, SaveError, SaveOptions, SaveReport, backup_path, save};
