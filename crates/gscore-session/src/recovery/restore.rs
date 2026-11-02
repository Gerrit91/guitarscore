//! Finding the recovery folders a crash left behind, and restoring a document from one.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gscore_edit::{Recorded, TabEditor};
use gscore_io::{FileFormat, detect_format, import_guitar_pro_from_bytes};
use gscore_model::TrackIndex;

use super::{
    BASE, JOURNAL, LOCK, META, SAVED, SNAPSHOT_INFO, SNAPSHOT_SONG, checksum, journal_entries,
};

/// A document that was not closed normally and has edits to recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundSession {
    /// The recovery folder.
    pub dir: PathBuf,
    /// The file the document came from, if it had one.
    pub source: Option<PathBuf>,
    /// Edits since the document was opened or last saved.
    pub edits: usize,
    /// When the journal was last written.
    pub modified: Option<SystemTime>,
}

/// A recovered document.
#[derive(Debug)]
pub struct Recovered {
    /// The editor with the song and its undo history (from the journal; only since the
    /// snapshot if the snapshot had to be used). Its applied commands are not taken yet:
    /// record them in the new document's journal.
    pub editor: TabEditor,
    /// The file the document came from, if it had one.
    pub source: Option<PathBuf>,
    /// The format of that file (decides whether Save writes in place).
    pub format: FileFormat,
    /// The bytes the editor's commands apply to: the new journal's base.
    pub base: Vec<u8>,
    /// Journal entries replayed.
    pub replayed: usize,
    /// Journal entries that could not be used (damaged at the end, or failing to replay).
    pub lost: usize,
    /// Whether the snapshot was the starting point (the undo history before it is gone).
    pub from_snapshot: bool,
}

/// Recovery folders in `root` that no running GuitarScore holds and that have edits to
/// recover, newest first. Folders with nothing to recover (closed right after a save) are
/// removed. A missing `root` has none.
pub fn find_sessions(root: &Path) -> Vec<FoundSession> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<FoundSession> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir())
        .filter_map(|dir| {
            // Held by a running GuitarScore (or unreadable): not ours to touch.
            let _lock = lock(&dir)?;
            let (entries, _) = read_journal(&dir);
            let edits = edits_since_save(&entries);
            if edits == 0 {
                let _ = std::fs::remove_dir_all(&dir);
                return None;
            }
            let modified = std::fs::metadata(dir.join(JOURNAL))
                .and_then(|m| m.modified())
                .ok();
            Some(FoundSession {
                source: read_source(&dir),
                dir,
                edits,
                modified,
            })
        })
        .collect();
    found.sort_by_key(|session| std::cmp::Reverse(session.modified));
    found
}

/// Restores the document of `session`: the journal replayed on the file as it was
/// opened, or, if that fails, on the last snapshot. Whichever recovers more is used.
pub fn recover(session: &FoundSession) -> Result<Recovered, String> {
    let dir = &session.dir;
    let _lock = lock(dir)
        .ok_or_else(|| format!("{} is in use by another GuitarScore window", dir.display()))?;
    let base = std::fs::read(dir.join(BASE))
        .map_err(|e| format!("cannot read the recovery base in {}: {e}", dir.display()))?;
    let source = read_source(dir);
    // The format decides whether Save writes in place. If the base is damaged, the file
    // name tells; failing that, a format that makes Save ask for a file (Save As).
    let format = detect_format(&base)
        .ok()
        .or_else(|| source.as_deref().and_then(format_of_name))
        .unwrap_or(FileFormat::Gpx);
    let (entries, damaged) = read_journal(dir);
    let commands: Vec<&String> = entries.iter().filter(|e| e.as_str() != SAVED).collect();
    let damaged_count = usize::from(damaged);

    let from_base = replay(&base, &commands);
    let from_snapshot = read_snapshot(dir).map(|(bytes, covered)| {
        // Entries before the snapshot are in it; count only the commands after.
        let skip = entries
            .iter()
            .take(covered)
            .filter(|e| e.as_str() != SAVED)
            .count();
        let rest: Vec<&String> = commands.iter().skip(skip).copied().collect();
        (replay(&bytes, &rest), bytes, skip)
    });

    let total = commands.len();
    match (from_base, from_snapshot) {
        (Ok((editor, replayed)), _) if replayed == total => Ok(Recovered {
            editor,
            source: source.clone(),
            format,
            base,
            replayed,
            lost: damaged_count,
            from_snapshot: false,
        }),
        (base_result, Some((Ok((editor, replayed)), bytes, skip)))
            if base_result.as_ref().map_or(true, |(_, from_base)| {
                skip.saturating_add(replayed) > *from_base
            }) =>
        {
            let replayed = skip.saturating_add(replayed);
            Ok(Recovered {
                editor,
                source: source.clone(),
                format,
                base: bytes,
                replayed,
                lost: total.saturating_sub(replayed).saturating_add(damaged_count),
                from_snapshot: true,
            })
        }
        (Ok((editor, replayed)), _) => Ok(Recovered {
            editor,
            source: source.clone(),
            format,
            base,
            replayed,
            lost: total.saturating_sub(replayed).saturating_add(damaged_count),
            from_snapshot: false,
        }),
        (Err(error), _) => Err(error),
    }
}

/// Removes the recovery folder of `session` (after it was recovered into a new journal,
/// or when the user does not want it). Refused while another GuitarScore holds it.
pub fn discard(session: &FoundSession) -> Result<(), String> {
    let lock = lock(&session.dir).ok_or_else(|| {
        format!(
            "{} is in use by another GuitarScore window",
            session.dir.display()
        )
    })?;
    drop(lock);
    std::fs::remove_dir_all(&session.dir)
        .map_err(|e| format!("cannot remove {}: {e}", session.dir.display()))
}

/// The folder's lock, if no running GuitarScore holds it. A folder without a lock file
/// (a crash before it was created) counts as free.
fn lock(dir: &Path) -> Option<Option<File>> {
    let path = dir.join(LOCK);
    let Ok(file) = File::options().read(true).write(true).open(&path) else {
        return Some(None);
    };
    match file.try_lock() {
        Ok(()) => Some(Some(file)),
        Err(TryLockError::WouldBlock) => None,
        // Locking unsupported here (some network file systems): treat it as free.
        Err(TryLockError::Error(_)) => Some(None),
    }
}

/// The journal's entries, and whether its end was damaged.
fn read_journal(dir: &Path) -> (Vec<String>, bool) {
    match std::fs::read(dir.join(JOURNAL)) {
        Ok(bytes) => journal_entries(&String::from_utf8_lossy(&bytes)),
        Err(_) => (Vec::new(), false),
    }
}

/// Commands after the last save.
fn edits_since_save(entries: &[String]) -> usize {
    entries
        .iter()
        .rev()
        .take_while(|entry| entry.as_str() != SAVED)
        .count()
}

/// The file the document came from, from the metadata.
fn read_source(dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dir.join(META)).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("source="))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// The snapshot's bytes and how many journal entries it includes, if it is whole.
fn read_snapshot(dir: &Path) -> Option<(Vec<u8>, usize)> {
    let info = std::fs::read_to_string(dir.join(SNAPSHOT_INFO)).ok()?;
    let value = |key: &str| {
        info.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::to_owned)
    };
    let covered: usize = value("entries=")?.parse().ok()?;
    let bytes = std::fs::read(dir.join(SNAPSHOT_SONG)).ok()?;
    (checksum(&bytes) == value("checksum=")?).then_some((bytes, covered))
}

/// An editor for the song in `bytes` with `commands` replayed, and how many replayed. A
/// command that does not parse or apply ends the replay (later ones depend on it).
fn replay(bytes: &[u8], commands: &[&String]) -> Result<(TabEditor, usize), String> {
    let song = import_guitar_pro_from_bytes(bytes)
        .map_err(|e| format!("cannot read the recovery base: {e}"))?
        .song;
    let mut editor = TabEditor::new(song, TrackIndex::new(0));
    let mut replayed = 0_usize;
    for line in commands {
        let Ok(Some(entry)) = Recorded::parse(line) else {
            break;
        };
        if editor.replay(entry).is_err() {
            break;
        }
        replayed = replayed.saturating_add(1);
    }
    Ok((editor, replayed))
}

/// The format a file name's extension stands for.
fn format_of_name(path: &Path) -> Option<FileFormat> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    [
        FileFormat::Gp,
        FileFormat::Gpx,
        FileFormat::Gp5,
        FileFormat::Gp4,
        FileFormat::Gp3,
    ]
    .into_iter()
    .find(|format| format.extension() == extension)
}
