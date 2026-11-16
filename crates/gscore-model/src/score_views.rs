//! Score views: named choices of which tracks a score shows, in what order, with which
//! staves, and whether a track's empty staves are left out (the strings' part, the
//! percussion part, ...). GuitarScore's own: the Guitar Pro formats have no field for them,
//! so the app saves them in the song's notes. Guitar Pro's one choice, which tracks are
//! shown when all are shown together (the eye in its track list), is each track's
//! [`crate::TrackView::is_visible`].
//!
//! A view refers to its tracks by a key each track keeps while the song is open, so a view
//! follows its tracks when they move, forgets a deleted one, and has it back when the
//! deletion is undone. Outside the model a view's tracks are plain track indices.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::position::TrackIndex;
use crate::style::TrackStaves;

/// The most views a song keeps.
pub const MAX_SCORE_VIEWS: usize = 64;
/// The longest view name, in characters.
pub const MAX_VIEW_NAME_CHARS: usize = 60;

/// One score view, with its tracks as indices into the song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreView {
    /// What the view is called ("Strings", "Percussion", ...).
    pub name: String,
    /// The tracks shown, top to bottom.
    pub tracks: Vec<ViewTrack>,
    /// Whether a track is left out of a line where it has no note (the first line shows
    /// every track of the view).
    pub hide_empty_staves: bool,
}

/// One track of a view and the staves it shows there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewTrack {
    pub track: TrackIndex,
    pub staves: TrackStaves,
}

/// A view as the song keeps it: tracks by their key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredView {
    pub(crate) name: String,
    pub(crate) tracks: Vec<(u64, TrackStaves)>,
    pub(crate) hide_empty_staves: bool,
}

/// The source of track keys, shared by all songs so a key is never reused.
static NEXT_TRACK_KEY: AtomicU64 = AtomicU64::new(1);

/// A key no track has had before.
pub(crate) fn new_track_key() -> u64 {
    NEXT_TRACK_KEY.fetch_add(1, Ordering::Relaxed)
}

/// Gives every track of `song` a key, keeping the keys when each track has one of its own
/// (a copy of an open song), else giving all new ones.
pub(crate) fn key_tracks(song: &mut gscore_gp::Song) {
    let mut keys: Vec<u64> = song.tracks.iter().map(|track| track.session_key).collect();
    keys.sort_unstable();
    let before = keys.len();
    keys.dedup();
    if keys.len() == before && !keys.contains(&0) {
        return;
    }
    for track in &mut song.tracks {
        track.session_key = new_track_key();
    }
}
