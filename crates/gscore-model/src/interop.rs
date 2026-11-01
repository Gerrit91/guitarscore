//! Conversion between [`Song`] and the `guitarpro` crate's song.
//!
//! **Only `gscore-io` may use this module** (enforced by `cargo xtask layers`). Everything
//! else works with the wrapper API, so the wrapped model can be replaced later.

use crate::error::ModelError;
use crate::song::Song;

/// Wraps a parsed `guitarpro` song after checking the model's invariants.
pub fn from_guitarpro(song: guitarpro::Song) -> Result<Song, ModelError> {
    Song::from_inner(song)
}

/// The wrapped `guitarpro` song, for serialization.
pub fn as_guitarpro(song: &Song) -> &guitarpro::Song {
    song.inner()
}
