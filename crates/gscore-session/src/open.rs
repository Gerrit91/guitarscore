//! Opening a Guitar Pro file from disk: a bounded read, then the import.

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use gscore_io::{Imported, IoError, MAX_INPUT_BYTES, import_guitar_pro_from_bytes};

/// Why a file could not be opened.
#[derive(Debug)]
pub enum OpenError {
    /// The file could not be read.
    Read {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file is larger than the importer accepts.
    TooLarge {
        /// The file.
        path: PathBuf,
        /// Its size in bytes (at least what was seen).
        size: u64,
    },
    /// The content is not a song the importer can read.
    Import {
        /// The file.
        path: PathBuf,
        /// What the importer reported.
        error: IoError,
    },
    /// The user cancelled the open while the file was being read (or right after).
    Cancelled,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            OpenError::TooLarge { path, size } => write!(
                f,
                "{} is {size} bytes; the limit is {MAX_INPUT_BYTES}",
                path.display()
            ),
            OpenError::Import { path, error } => {
                write!(f, "cannot import {}: {error}", path.display())
            }
            OpenError::Cancelled => f.write_str("opening was cancelled"),
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OpenError::Read { source, .. } => Some(source),
            OpenError::Import { error, .. } => Some(error),
            OpenError::TooLarge { .. } | OpenError::Cancelled => None,
        }
    }
}

/// Reads and imports the Guitar Pro file at `path`. Files larger than the importer accepts
/// are refused before reading, and the read itself is capped, so a file that grows in the
/// meantime cannot make this allocate without bound.
pub fn open(path: &Path) -> Result<Imported, OpenError> {
    open_with_bytes(path).map(|(imported, _)| imported)
}

/// [`open`], also returning the file's bytes (the recovery journal keeps them as the base
/// its commands apply to).
pub fn open_with_bytes(path: &Path) -> Result<(Imported, Vec<u8>), OpenError> {
    open_with_progress(path, |_, _| {}, || false)
}

/// Imports bytes that were read elsewhere (a browser file picker, a drop): the same size
/// limit and import as [`open`], with `name` only used in messages.
pub fn open_bytes(name: &Path, bytes: Vec<u8>) -> Result<(Imported, Vec<u8>), OpenError> {
    let limit = u64::try_from(MAX_INPUT_BYTES).unwrap_or(u64::MAX);
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit {
        return Err(OpenError::TooLarge {
            path: name.to_path_buf(),
            size,
        });
    }
    let imported = import_guitar_pro_from_bytes(&bytes).map_err(|error| OpenError::Import {
        path: name.to_path_buf(),
        error,
    })?;
    Ok((imported, bytes))
}

/// Bytes read at a time while reporting progress.
const READ_CHUNK: usize = 64 * 1024;

/// [`open_with_bytes`], reporting how many bytes of `total` have been read (the import
/// itself cannot report progress) and checking `cancelled` between chunks, before and after
/// the import. A cancelled open leaves the file untouched.
pub fn open_with_progress<P, C>(
    path: &Path,
    mut progress: P,
    cancelled: C,
) -> Result<(Imported, Vec<u8>), OpenError>
where
    P: FnMut(u64, u64),
    C: Fn() -> bool,
{
    let read_error = |source| OpenError::Read {
        path: path.to_path_buf(),
        source,
    };
    let limit = u64::try_from(MAX_INPUT_BYTES).unwrap_or(u64::MAX);
    let too_large = |size| OpenError::TooLarge {
        path: path.to_path_buf(),
        size,
    };
    let size = std::fs::metadata(path).map_err(read_error)?.len();
    if size > limit {
        return Err(too_large(size));
    }
    if cancelled() {
        return Err(OpenError::Cancelled);
    }
    let mut file = std::fs::File::open(path).map_err(read_error)?;
    let capacity = usize::try_from(size.min(limit)).unwrap_or(0);
    let mut bytes: Vec<u8> = Vec::with_capacity(capacity);
    let mut chunk = [0_u8; READ_CHUNK];
    loop {
        if cancelled() {
            return Err(OpenError::Cancelled);
        }
        let read = file.read(&mut chunk).map_err(read_error)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(chunk.get(..read).unwrap_or_default());
        let done = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if done > limit {
            return Err(too_large(done));
        }
        progress(done, size);
    }
    progress(size, size);
    if cancelled() {
        return Err(OpenError::Cancelled);
    }
    let imported = import_guitar_pro_from_bytes(&bytes).map_err(|error| OpenError::Import {
        path: path.to_path_buf(),
        error,
    })?;
    if cancelled() {
        return Err(OpenError::Cancelled);
    }
    Ok((imported, bytes))
}
