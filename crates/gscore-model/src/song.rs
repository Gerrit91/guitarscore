//! [`Song`]: the document, wrapping the Guitar Pro song data.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::ModelError;
use crate::issues::{self, Issue};
use crate::position::{BeatPosition, MeasureIndex, TrackIndex};
use crate::validate;
use crate::views::{BeatView, TrackView};

/// A complete score: metadata and tracks.
///
/// Created by `gscore-io` when a file is opened, or with [`Song::default`] for a new,
/// empty document. Always satisfies the invariants checked on creation.
#[derive(Debug, Clone, Default)]
pub struct Song {
    inner: gscore_gp::Song,
    /// One stamp per measure; see [`Song::measure_stamp`].
    stamps: Vec<u64>,
}

/// The source of measure stamps, shared by all songs so a stamp is never reused.
static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

/// A stamp no measure had before.
fn new_stamp() -> u64 {
    NEXT_STAMP.fetch_add(1, Ordering::Relaxed)
}

impl Song {
    /// Wraps Guitar Pro song data after checking the invariants.
    pub(crate) fn from_inner(inner: gscore_gp::Song) -> Result<Self, ModelError> {
        let stamps = inner.measure_headers.iter().map(|_| new_stamp()).collect();
        let song = Song { inner, stamps };
        validate::check(&song)?;
        Ok(song)
    }

    pub(crate) fn inner(&self) -> &gscore_gp::Song {
        &self.inner
    }

    /// The wrapped data for an edit that cannot leave the song inconsistent (metadata,
    /// lyrics); prefer [`Song::edit_metadata`] in new code.
    pub(crate) fn inner_mut(&mut self) -> &mut gscore_gp::Song {
        &mut self.inner
    }

    /// A new score: one six-string guitar track with a single 4/4 measure of rests, ready
    /// for note entry.
    pub fn new_score() -> Result<Self, ModelError> {
        let mut song = Song::default();
        let _ = song.insert_track(TrackIndex::new(0))?;
        let _ = song.insert_measure(MeasureIndex::new(0))?;
        Ok(song)
    }

    /// A number that changes whenever measure `measure` may have changed, in any track,
    /// and is never reused: a layout cache can key a measure's layout on it. Measures that
    /// move (a measure inserted before them) keep their stamp. 0 for a missing measure.
    pub fn measure_stamp(&self, measure: MeasureIndex) -> u64 {
        self.stamps.get(measure.get()).copied().unwrap_or(0)
    }

    /// Runs `edit` on a copy of one measure of one track, and swaps the copy in only when
    /// the edit and the measure's invariant check both succeeded (AGENTS.md rule 8:
    /// commands are transactional). Only that measure is copied and checked, so an edit
    /// costs the same in a long song as in a short one.
    pub(crate) fn edit_measure<T>(
        &mut self,
        track: TrackIndex,
        measure: MeasureIndex,
        edit: impl FnOnce(&mut gscore_gp::Measure) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let tracks = self.inner.tracks.len();
        let slot = self
            .inner
            .tracks
            .get_mut(track.get())
            .ok_or_else(|| ModelError::Invalid(format!("{track}: the song has {tracks} track(s)")))?
            .measures
            .get_mut(measure.get())
            .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
        let mut candidate = slot.clone();
        let value = edit(&mut candidate)?;
        validate::check_measure_data(measure, &candidate)
            .map_err(|e| validate::locate(e, &track.to_string()))?;
        *slot = candidate;
        if let Some(stamp) = self.stamps.get_mut(measure.get()) {
            *stamp = new_stamp();
        }
        Ok(value)
    }

    /// Runs `edit` directly on the song data, for edits that insert or remove whole
    /// measures. `edit` must check everything before it changes anything, so that an error
    /// leaves the song untouched; it returns the result and how the measures moved, which
    /// keeps the stamps in step.
    pub(crate) fn edit_structure<T>(
        &mut self,
        edit: impl FnOnce(&mut gscore_gp::Song) -> Result<(T, Moved), ModelError>,
    ) -> Result<T, ModelError> {
        let (value, moved) = edit(&mut self.inner)?;
        match moved {
            Moved::Inserted(at) => {
                let at = at.get().min(self.stamps.len());
                self.stamps.insert(at, new_stamp());
            }
            Moved::Removed(at) => {
                if at.get() < self.stamps.len() {
                    self.stamps.remove(at.get());
                }
            }
        }
        debug_assert_eq!(self.stamps.len(), self.inner.measure_headers.len());
        Ok(value)
    }

    /// Runs `edit` on the song's tracks, for edits that add, remove, duplicate or move a
    /// track. Every measure gets a new stamp afterwards: a layout cache keeps measures per
    /// track index, and after such an edit an index may name another track (several tracks
    /// can be laid out together). `edit` must check everything before it changes anything,
    /// so an error leaves the song untouched.
    pub(crate) fn edit_tracks<T>(
        &mut self,
        edit: impl FnOnce(&mut gscore_gp::Song) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let value = edit(&mut self.inner)?;
        self.touch_all_measures();
        Ok(value)
    }

    /// Runs `edit` directly on the song's measure headers (tempo, signatures, markers).
    /// `edit` must check everything first.
    pub(crate) fn edit_headers<T>(
        &mut self,
        edit: impl FnOnce(&mut gscore_gp::Song) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        edit(&mut self.inner)
    }

    /// Gives every measure a new stamp, after an edit that changes all layouts (a time
    /// signature).
    pub(crate) fn touch_all_measures(&mut self) {
        for stamp in &mut self.stamps {
            *stamp = new_stamp();
        }
    }

    /// Runs `edit` directly on the song's metadata (text fields, tempo), which changes no
    /// measure and no layout stamp. `edit` must check everything first.
    pub(crate) fn edit_metadata<T>(
        &mut self,
        edit: impl FnOnce(&mut gscore_gp::Song) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        edit(&mut self.inner)
    }

    /// Runs `edit` on the song's tracks for settings that do not change any measure's
    /// layout (name, color, mute, solo, volume, pan). `edit` must check everything first.
    pub(crate) fn edit_track_settings<T>(
        &mut self,
        edit: impl FnOnce(&mut gscore_gp::Song) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        edit(&mut self.inner)
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

    /// The key signature in effect at `measure`, or `None` if there is no such measure.
    pub fn key_signature(&self, measure: MeasureIndex) -> Option<crate::style::KeySignature> {
        let header = self.inner.measure_headers.get(measure.get())?;
        Some(crate::style::KeySignature {
            key: header.key_signature.key,
            minor: header.key_signature.is_minor,
        })
    }

    /// The section marker at the start of `measure`, or `None` if there is none. Markers
    /// are stored with the measure headers, so every track shares them.
    pub fn section(&self, measure: MeasureIndex) -> Option<crate::style::Section> {
        let marker = self
            .inner
            .measure_headers
            .get(measure.get())?
            .marker
            .as_ref()?;
        Some(crate::style::Section {
            title: marker.title.clone(),
            color: crate::style::TrackColor::from_gp(marker.color),
        })
    }

    /// Whether any track is soloed. Then only the soloed tracks play.
    pub fn any_soloed(&self) -> bool {
        self.inner.tracks.iter().any(|track| track.solo)
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

/// How a structural edit moved the measures.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Moved {
    /// A measure was inserted at this index in every track.
    Inserted(MeasureIndex),
    /// The measure at this index was removed from every track.
    Removed(MeasureIndex),
}
