//! Never lose work (AGENTS.md "Never lose work"): a recovery journal per open document, and
//! crash recovery on the next start.
//!
//! Each open document gets a folder in the recovery directory:
//!
//! - `lock`: held open and locked while the document is open. A crashed process loses
//!   its lock, which is how a crash is told apart from another running GuitarScore.
//! - `meta`: the file the document came from (`source=<path>`), replaced atomically.
//! - `base`: the file's bytes as opened; the journal's commands apply to them.
//! - `journal`: one entry per line, `<checksum> <entry>`, appended as the user edits: a
//!   command in the command language ([`gscore_edit::Recorded::to_line`], including
//!   `undo` and `redo`, and `merge` before a slider change that continued the undo step
//!   before it), or `!saved` after a save. A line whose checksum does not match (a write
//!   cut short by a crash) ends the journal.
//! - `snapshot.gp` and `snapshot`: the song every [`SNAPSHOT_EVERY`] entries (or
//!   [`SNAPSHOT_INTERVAL`]), as Guitar Pro 7, and how many journal entries it includes;
//!   written to temporary files and renamed into place, so a snapshot is whole or absent.
//!
//! Recovery replays the journal on the base, which also restores the undo history; if
//! that fails, it starts from the snapshot. Autosave never touches the user's file: all of
//! this lives in the recovery directory. A document closed normally removes its folder.
//!
//! All writing happens on a background thread ([`Journal`]); the UI only queues entries.

mod journal;
mod restore;

use std::path::PathBuf;
use std::time::Duration;

pub use journal::Journal;
pub use restore::{FoundSession, Recovered, discard, find_sessions, recover};

/// A snapshot is written after this many journal entries ...
pub const SNAPSHOT_EVERY: usize = 200;
/// ... or this long after the last one, if anything changed since.
pub const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);

const LOCK: &str = "lock";
const META: &str = "meta";
const BASE: &str = "base";
const JOURNAL: &str = "journal";
const SNAPSHOT_SONG: &str = "snapshot.gp";
const SNAPSHOT_INFO: &str = "snapshot";
/// The journal entry written after a save.
const SAVED: &str = "!saved";

/// The per-user recovery directory, or `None` if the environment does not say where the
/// home directory is: next to the crash reports (Linux and other Unix:
/// `$XDG_STATE_HOME/guitarscore/recovery`, defaulting to `~/.local/state/...`; macOS:
/// `~/Library/Application Support/guitarscore/recovery`; Windows:
/// `%LOCALAPPDATA%\guitarscore\recovery`).
pub fn default_recovery_dir() -> Option<PathBuf> {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let app = if cfg!(windows) {
        env_dir("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        env_dir("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        env_dir("XDG_STATE_HOME")
            .or_else(|| env_dir("HOME").map(|h| h.join(".local").join("state")))
    };
    app.map(|d| d.join("guitarscore").join("recovery"))
}

/// The checksum of a journal entry: 64-bit FNV-1a, as 16 hex digits. It detects a line
/// cut short or garbled by a crash; it is not a security measure.
fn checksum(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// One journal line for `entry`.
fn journal_line(entry: &str) -> String {
    format!("{} {entry}\n", checksum(entry.as_bytes()))
}

/// The entries of a journal, up to the first line that is damaged (a crash in the middle of
/// a write), and whether such a line was found.
fn journal_entries(text: &str) -> (Vec<String>, bool) {
    let mut entries = Vec::new();
    for line in text.split_inclusive('\n') {
        let Some(line) = line.strip_suffix('\n') else {
            // The last line was cut short.
            return (entries, true);
        };
        match line.split_once(' ') {
            Some((sum, entry)) if checksum(entry.as_bytes()) == sum => {
                entries.push(entry.to_owned());
            }
            _ => return (entries, true),
        }
    }
    (entries, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_lines_read_back_and_damage_ends_the_journal() {
        let mut text = String::new();
        for entry in [
            "set_fret measure=1 beat=1 string=1 fret=5",
            "undo",
            "!saved",
        ] {
            text.push_str(&journal_line(entry));
        }
        assert_eq!(
            journal_entries(&text),
            (
                vec![
                    "set_fret measure=1 beat=1 string=1 fret=5".to_owned(),
                    "undo".to_owned(),
                    "!saved".to_owned()
                ],
                false
            )
        );
        // Cut in the middle of the last line: the first two remain.
        let cut = &text[..text.len() - 3];
        assert_eq!(journal_entries(cut).0.len(), 2);
        assert!(journal_entries(cut).1);
        // A changed character in the first line ends the journal there.
        let garbled = text.replacen("fret=5", "fret=6", 1);
        assert_eq!(journal_entries(&garbled), (Vec::new(), true));
        assert_eq!(journal_entries(""), (Vec::new(), false));
    }
}
