//! Why layout failed.

use std::fmt;

use gscore_model::{ModelError, StaffIndex, TrackIndex};

/// A layout failure. Layout never panics; it returns one of these instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// The song has no such track.
    NoSuchTrack(TrackIndex),
    /// The track has no such staff.
    NoSuchStaff(TrackIndex, StaffIndex),
    /// The track cannot be shown as tablature yet.
    Unsupported(String),
    /// A value read from the song was invalid.
    Model(ModelError),
    /// The music font's metadata is unusable.
    Font(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::NoSuchTrack(track) => write!(f, "the song has no {track}"),
            LayoutError::NoSuchStaff(track, staff) => {
                write!(f, "{track} has no {staff}")
            }
            LayoutError::Unsupported(what) => write!(f, "not supported yet: {what}"),
            LayoutError::Model(e) => write!(f, "{e}"),
            LayoutError::Font(e) => write!(f, "music font: {e}"),
        }
    }
}

impl std::error::Error for LayoutError {}

impl From<ModelError> for LayoutError {
    fn from(error: ModelError) -> Self {
        LayoutError::Model(error)
    }
}
