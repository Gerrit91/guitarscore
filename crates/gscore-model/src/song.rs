//! [`Song`]: the document, wrapping the Guitar Pro song data.

use crate::error::ModelError;
use crate::issues::{self, Issue};
use crate::position::{BeatPosition, TrackIndex};
use crate::validate;
use crate::views::{BeatView, TrackView};

/// A complete score: metadata and tracks.
///
/// Created by `gscore-io` when a file is opened, or with [`Song::default`] for a new,
/// empty document. Always satisfies the invariants checked on creation.
#[derive(Debug, Clone, Default)]
pub struct Song {
    inner: gscore_gp::Song,
}

impl Song {
    /// Wraps Guitar Pro song data after checking the invariants.
    pub(crate) fn from_inner(inner: gscore_gp::Song) -> Result<Self, ModelError> {
        let song = Song { inner };
        validate::check(&song)?;
        Ok(song)
    }

    pub(crate) fn inner(&self) -> &gscore_gp::Song {
        &self.inner
    }

    /// The title (may be empty, may be any Unicode text).
    pub fn title(&self) -> &str {
        &self.inner.name
    }

    /// The subtitle (may be empty).
    pub fn subtitle(&self) -> &str {
        &self.inner.subtitle
    }

    /// The artist (may be empty).
    pub fn artist(&self) -> &str {
        &self.inner.artist
    }

    /// The album (may be empty).
    pub fn album(&self) -> &str {
        &self.inner.album
    }

    /// Who wrote the lyrics (may be empty).
    pub fn lyricist(&self) -> &str {
        &self.inner.words
    }

    /// Who wrote the music (may be empty).
    pub fn composer(&self) -> &str {
        &self.inner.author
    }

    /// The copyright notice (may be empty).
    pub fn copyright(&self) -> &str {
        &self.inner.copyright
    }

    /// Who wrote the tablature (may be empty).
    pub fn tab_author(&self) -> &str {
        &self.inner.writer
    }

    /// Performance instructions (may be empty).
    pub fn instructions(&self) -> &str {
        &self.inner.instructions
    }

    /// Free-text notes ("notices" in Guitar Pro), one entry per line; empty if none.
    pub fn notes(&self) -> impl Iterator<Item = &str> {
        self.inner.notice.iter().map(String::as_str)
    }

    /// The initial tempo in quarter notes per minute (always positive).
    pub fn tempo(&self) -> u32 {
        // Validated positive on creation, so the absolute value is the value.
        u32::from(self.inner.tempo.unsigned_abs())
    }

    /// Number of measures; every track has exactly this many.
    pub fn measure_count(&self) -> usize {
        self.inner.measure_headers.len()
    }

    /// Number of tracks.
    pub fn track_count(&self) -> usize {
        self.inner.tracks.len()
    }

    /// All tracks, in order.
    pub fn tracks(&self) -> impl ExactSizeIterator<Item = TrackView<'_>> {
        self.inner
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| TrackView::new(TrackIndex::new(i), t))
    }

    /// One track, or `None` if the index is out of range.
    pub fn track(&self, index: TrackIndex) -> Option<TrackView<'_>> {
        self.inner
            .tracks
            .get(index.get())
            .map(|t| TrackView::new(index, t))
    }

    /// The beat at `position`, or `None` if any part of it is out of range.
    pub fn beat(&self, position: BeatPosition) -> Option<BeatView<'_>> {
        self.track(position.track)?
            .measure(position.measure)?
            .voice(position.voice)?
            .beat(position.beat)
    }

    /// Problems that do not prevent opening or editing: measures that are not completely
    /// filled, notes on strings the track lacks, several notes on one string, odd frets.
    /// Empty if there are none.
    pub fn issues(&self) -> Vec<Issue> {
        issues::find(self)
    }
}
