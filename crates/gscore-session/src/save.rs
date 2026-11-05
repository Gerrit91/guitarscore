//! Saving a song to a file without risking the user's work (AGENTS.md "Saving").
//!
//! The order matters:
//! 1. Only formats whose round trips pass may be written ([`SAVE_FORMATS`]).
//! 2. The song is serialized and read back in memory; anything the format would lose is
//!    refused unless the caller allows it ([`SaveOptions::allow_losses`]). Nothing has
//!    touched the disk yet.
//! 3. The bytes go to a temporary file next to the target, which is flushed to disk.
//! 4. The first time an existing file is overwritten, it is copied to `<name>.bak`; an
//!    existing backup is kept as it is, so it always holds the original.
//! 5. The temporary file atomically replaces the target, and the directory is flushed.
//! 6. The saved file is read back from disk and compared with the bytes written.
//!
//! A failure before step 5 leaves the target untouched and removes the temporary file.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use gscore_io::{FileFormat, IoError, export_to_bytes, import_guitar_pro_from_bytes};
use gscore_model::{Difference, Song, differences, settings_differences};

/// Formats that can be saved. Others are refused until their round trips pass (see
/// `docs/gaps.md`).
pub const SAVE_FORMATS: &[FileFormat] = &[FileFormat::Gp];

/// How to save.
#[derive(Debug, Clone, Copy)]
pub struct SaveOptions {
    /// The format to write.
    pub format: FileFormat,
    /// Save even if the format cannot keep everything; the losses are reported.
    pub allow_losses: bool,
}

/// What a successful save did.
#[derive(Debug, Clone)]
pub struct SaveReport {
    /// The file that was written.
    pub path: PathBuf,
    /// The backup created by this save, if it was the first time `path` was overwritten.
    pub backup: Option<PathBuf>,
    /// What the format could not keep (only with [`SaveOptions::allow_losses`]).
    pub losses: Vec<Difference>,
}

/// Why a save did not happen or could not be confirmed.
#[derive(Debug)]
pub enum SaveError {
    /// The format cannot be saved yet.
    UnsupportedFormat(FileFormat),
    /// Serializing the song failed.
    Export(IoError),
    /// The written bytes could not be read back.
    Reread(IoError),
    /// The format would lose content; nothing was written.
    Losses(Vec<Difference>),
    /// A file system operation failed.
    Io {
        /// What was being done, with the path.
        what: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file on disk is not what was written. The target was already replaced; the
    /// backup (if any) still holds the previous version.
    Verification {
        /// The saved file.
        path: PathBuf,
        /// The backup of the previous version, if one exists.
        backup: Option<PathBuf>,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::UnsupportedFormat(format) => write!(
                f,
                "saving as {format} is not supported yet (supported: {})",
                SAVE_FORMATS
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            SaveError::Export(e) => write!(f, "the song could not be written: {e}"),
            SaveError::Reread(e) => write!(f, "the written song could not be read back: {e}"),
            SaveError::Losses(diffs) => write!(
                f,
                "the format cannot keep everything ({} difference(s)); nothing was written",
                diffs.len()
            ),
            SaveError::Io { what, source } => write!(f, "{what}: {source}"),
            SaveError::Verification { path, backup } => {
                write!(
                    f,
                    "{} does not contain what was written; check the disk",
                    path.display()
                )?;
                match backup {
                    Some(backup) => write!(f, " (the previous version is in {})", backup.display()),
                    None => Ok(()),
                }
            }
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SaveError::Export(e) | SaveError::Reread(e) => Some(e),
            SaveError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Saves `song` to `path` (see the module docs for the steps).
pub fn save(song: &Song, path: &Path, options: SaveOptions) -> Result<SaveReport, SaveError> {
    let (bytes, losses) = save_bytes(song, options)?;

    let temp = sibling(path, |name| {
        let mut temp = OsString::from(".");
        temp.push(name);
        temp.push(format!(".gscore-tmp-{}", std::process::id()));
        temp
    })?;
    if let Err(e) = write_synced(&temp, &bytes) {
        // Best effort: the target is untouched either way.
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    let backup = match back_up_once(path) {
        Ok(backup) => backup,
        Err(e) => {
            let _ = fs::remove_file(&temp);
            return Err(e);
        }
    };
    if let Err(source) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(SaveError::Io {
            what: format!("cannot replace {}", path.display()),
            source,
        });
    }
    sync_directory(path);

    let on_disk = fs::read(path).map_err(|source| SaveError::Io {
        what: format!("cannot read back {}", path.display()),
        source,
    })?;
    if on_disk != bytes {
        return Err(SaveError::Verification {
            path: path.to_path_buf(),
            backup,
        });
    }
    Ok(SaveReport {
        path: path.to_path_buf(),
        backup,
        losses,
    })
}

/// The steps of a save that do not touch the disk: serialize, read back and compare, and
/// refuse or report losses. Returns the bytes to write (or download, in a browser) and what
/// the format could not keep.
pub fn save_bytes(
    song: &Song,
    options: SaveOptions,
) -> Result<(Vec<u8>, Vec<Difference>), SaveError> {
    if !SAVE_FORMATS.contains(&options.format) {
        return Err(SaveError::UnsupportedFormat(options.format));
    }
    let bytes = export_to_bytes(song, options.format).map_err(SaveError::Export)?;
    let reread = import_guitar_pro_from_bytes(&bytes).map_err(SaveError::Reread)?;
    let mut losses = differences(song, &reread.song);
    // Color, mute, solo, volume, pan and the program are not part of the musical
    // comparison; the format must keep them too, or the user is told before writing.
    losses.extend(settings_differences(song, &reread.song));
    if !losses.is_empty() && !options.allow_losses {
        return Err(SaveError::Losses(losses));
    }
    Ok((bytes, losses))
}

/// The backup path of `path`: the file name with `.bak` appended (`song.gp.bak`).
pub fn backup_path(path: &Path) -> Result<PathBuf, SaveError> {
    sibling(path, |name| {
        let mut backup = name.to_os_string();
        backup.push(".bak");
        backup
    })
}

/// A file in the same directory as `path`, named by `rename` from `path`'s file name.
pub(crate) fn sibling(
    path: &Path,
    rename: impl FnOnce(&std::ffi::OsStr) -> OsString,
) -> Result<PathBuf, SaveError> {
    let name = path.file_name().ok_or_else(|| SaveError::Io {
        what: format!("{} is not a file path", path.display()),
        source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
    })?;
    Ok(path.with_file_name(rename(name)))
}

/// Writes `bytes` to a new file at `path` and flushes it to the disk.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let io = |what: &str| {
        let what = format!("cannot {what} {}", path.display());
        move |source| SaveError::Io { what, source }
    };
    let mut file = File::create(path).map_err(io("create"))?;
    file.write_all(bytes).map_err(io("write"))?;
    file.sync_all().map_err(io("flush"))
}

/// Copies an existing `path` to its backup, unless a backup already exists. Returns the
/// backup it created.
fn back_up_once(path: &Path) -> Result<Option<PathBuf>, SaveError> {
    if !path.is_file() {
        return Ok(None);
    }
    let backup = backup_path(path)?;
    if backup.exists() {
        return Ok(None);
    }
    fs::copy(path, &backup).map_err(|source| SaveError::Io {
        what: format!("cannot back up {} to {}", path.display(), backup.display()),
        source,
    })?;
    Ok(Some(backup))
}

/// Flushes the directory entry of a rename to disk, where the platform supports it. A
/// failure is not an error: the file itself is already flushed.
fn sync_directory(path: &Path) {
    #[cfg(unix)]
    if let Some(directory) = path.parent() {
        let directory = if directory.as_os_str().is_empty() {
            Path::new(".")
        } else {
            directory
        };
        if let Ok(handle) = File::open(directory) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}
