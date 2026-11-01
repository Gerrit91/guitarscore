//! Error type of the gscore-model crate.

use std::fmt;

/// A song violates an invariant, or a computation on it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// The song holds a value the editor cannot represent. The message says what and where.
    Invalid(String),
    /// Arithmetic on durations overflowed (only possible with absurd tuplets or lengths).
    Overflow(&'static str),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Invalid(msg) => write!(f, "invalid song: {msg}"),
            ModelError::Overflow(what) => write!(f, "arithmetic overflow while computing {what}"),
        }
    }
}

impl std::error::Error for ModelError {}
