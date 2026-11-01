//! Conversion between [`Song`] and the Guitar Pro song data in [`crate::gp`].
//!
//! **Only `gscore-io` may use this module** (enforced by `cargo xtask layers`). Everything
//! else works with the wrapper API, so the wrapped model can be replaced later.

use crate::error::ModelError;
use crate::song::Song;

/// Wraps parsed Guitar Pro song data after checking the model's invariants.
pub fn from_gp(song: crate::gp::Song) -> Result<Song, ModelError> {
    Song::from_inner(song)
}

/// The wrapped Guitar Pro song data, for serialization.
pub fn as_gp(song: &Song) -> &crate::gp::Song {
    song.inner()
}
