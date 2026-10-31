//! Error types for gscore-io crate

use std::fmt;

/// Custom error type for I/O operations in gscore-io
#[derive(Debug)]
pub enum IoError {
    /// Error from the guitarpro crate when parsing files
    GuitarProError(String),
    /// Error from standard I/O operations
    IoError(std::io::Error),
    /// Error when a file format is not supported
    UnsupportedFormat(String),
    /// Error when a file cannot be parsed correctly
    ParseError(String),
    /// Error when a file cannot be written correctly
    WriteError(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::GuitarProError(msg) => write!(f, "GuitarPro error: {}", msg),
            IoError::IoError(err) => write!(f, "IO error: {}", err),
            IoError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {}", msg),
            IoError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            IoError::WriteError(msg) => write!(f, "Write error: {}", msg),
        }
    }
}

impl std::error::Error for IoError {}

impl From<std::io::Error> for IoError {
    fn from(error: std::io::Error) -> Self {
        IoError::IoError(error)
    }
}