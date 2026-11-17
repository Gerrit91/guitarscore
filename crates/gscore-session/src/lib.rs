//! The open document and everything that keeps the user's work safe on disk: opening files,
//! crash reports (a panic hook that logs diagnostics to a file), safe saving, the user's
//! settings files (the keymap), and the recovery journal with crash recovery (AGENTS.md
//! "Never lose work").

// The files and locks are dropped on purpose before a rename or a removal (Windows will
// not rename or delete an open file). In the browser `std::fs::File` is a stub without a
// destructor, so there clippy sees the drops as doing nothing.
#![cfg_attr(target_arch = "wasm32", allow(clippy::drop_non_drop))]

mod config;
mod crash_report;
mod export;
mod open;
mod recovery;
mod save;
#[cfg(target_arch = "wasm32")]
mod web;

pub use config::{
    KEYMAP_FILE, LoadedJson, LoadedKeymap, SETTINGS_FILE, default_config_dir,
    default_settings_path, load_json, load_keymap, load_user_keymap, save_json,
};
pub use crash_report::{default_crash_dir, install_panic_hook};
pub use export::{ExportReport, export};
pub use gscore_io::FileFormat;
pub use open::{OpenError, open, open_bytes, open_with_bytes, open_with_progress};
pub use recovery::{
    FoundSession, Journal, Recovered, RecoveryOptions, SNAPSHOT_EVERY, SNAPSHOT_INTERVAL,
    default_recovery_dir, discard, find_sessions, recover,
};
pub use save::{SAVE_FORMATS, SaveError, SaveOptions, SaveReport, backup_path, save, save_bytes};
#[cfg(target_arch = "wasm32")]
pub use web::{WebKey, load_web_json, load_web_keymap, save_web_json};
