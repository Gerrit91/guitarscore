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
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OpenError::Read { source, .. } => Some(source),
            OpenError::Import { error, .. } => Some(error),
            OpenError::TooLarge { .. } => None,
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
    let file = std::fs::File::open(path).map_err(read_error)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > limit {
        return Err(too_large(read));
    }
    let imported = import_guitar_pro_from_bytes(&bytes).map_err(|error| OpenError::Import {
        path: path.to_path_buf(),
        error,
    })?;
    Ok((imported, bytes))
}
