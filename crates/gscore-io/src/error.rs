//! Error type of the gscore-io crate.

use std::fmt;

use gscore_model::ModelError;

/// Everything that can go wrong while converting between bytes and a `Song`.
#[derive(Debug)]
pub enum IoError {
    /// The Guitar Pro reader (`gscore-gp`) rejected the input or panicked while handling it.
    GuitarProError(String),
    /// The file format is not supported, or the file is too large.
    UnsupportedFormat(String),
    /// The file parsed, but the song breaks the model's invariants.
    InvalidSong(ModelError),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::GuitarProError(msg) => write!(f, "GuitarPro error: {msg}"),
            IoError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {msg}"),
            IoError::InvalidSong(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IoError::InvalidSong(err) => Some(err),
            _ => None,
        }
    }
}
