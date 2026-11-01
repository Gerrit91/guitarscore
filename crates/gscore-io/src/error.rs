//! Error type of the gscore-io crate.

use std::fmt;

/// Everything that can go wrong while converting between bytes and a `Song`.
#[derive(Debug)]
pub enum IoError {
    /// The guitarpro crate rejected the input or panicked while handling it.
    GuitarProError(String),
    /// The file format is not supported.
    UnsupportedFormat(String),
    /// The feature is not implemented yet.
    Unsupported(&'static str),
    /// The file parsed, but holds values the model cannot represent.
    ParseError(String),
    /// The song could not be serialized.
    WriteError(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::GuitarProError(msg) => write!(f, "GuitarPro error: {msg}"),
            IoError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {msg}"),
            IoError::Unsupported(what) => write!(f, "Not supported yet: {what}"),
            IoError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            IoError::WriteError(msg) => write!(f, "Write error: {msg}"),
        }
    }
}

impl std::error::Error for IoError {}
