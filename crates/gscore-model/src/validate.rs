//! Invariants every [`Song`] satisfies. Checked once, when a song is created.
//!
//! Only structural problems are errors here: ones that break timing (durations, time
//! signatures, tempo) or navigation (string and measure counts). Everything that real files
//! contain, including odd notes, is reported by [`crate::issues`] instead: refusing to open
//! a user's file is worse than showing a warning.

use crate::error::ModelError;
use crate::position::MeasureIndex;
use crate::song::Song;
use crate::views::{BeatView, MeasureView, TrackView};

/// Most strings a track may have (Guitar Pro allows up to 7, newer versions 8; leave room).
pub(crate) const MAX_STRINGS: usize = 12;

/// Checks all invariants, returning the first violation with its location.
pub(crate) fn check(song: &Song) -> Result<(), ModelError> {
    if song.inner().tempo <= 0 {
        return Err(ModelError::Invalid(format!(
            "tempo {} is not positive",
            song.inner().tempo
        )));
    }
    for track in song.tracks() {
        check_track(song, &track).map_err(|e| locate(e, &track.index().to_string()))?;
    }
    Ok(())
}

fn check_track(song: &Song, track: &TrackView<'_>) -> Result<(), ModelError> {
    let count = track.string_count();
    if !(1..=MAX_STRINGS).contains(&count) {
        return Err(ModelError::Invalid(format!(
            "{count} strings (supported: 1 to {MAX_STRINGS})"
        )));
    }
    // Strings must be numbered 1, 2, ..., n in order.
    for (expected, tuning) in (1_u8..).zip(track.strings()?) {
        if tuning.string.get() != expected {
            return Err(ModelError::Invalid(format!(
                "tuning lists {} where string {expected} was expected",
                tuning.string
            )));
        }
    }
    if track.measures().len() != song.measure_count() {
        return Err(ModelError::Invalid(format!(
            "{} measures, but the song has {}",
            track.measures().len(),
            song.measure_count()
        )));
    }
    for measure in track.measures() {
        check_measure(&measure).map_err(|e| locate(e, &measure.index().to_string()))?;
    }
    Ok(())
}

fn check_measure(measure: &MeasureView<'_>) -> Result<(), ModelError> {
    measure.time_signature()?;
    for voice in measure.voices() {
        for beat in voice.beats() {
            check_beat(&beat)
                .map_err(|e| locate(e, &format!("{}, {}", voice.index(), beat.index())))?;
        }
    }
    Ok(())
}

fn check_beat(beat: &BeatView<'_>) -> Result<(), ModelError> {
    // Note-level problems (unknown string, odd fret, two notes on one string) occur in real
    // files, e.g. drum and piano tracks mapped onto tab strings, so they are issues instead.
    beat.duration().map(|_| ())
}

/// Checks one measure's own invariants, for an edit that changed only that measure.
pub(crate) fn check_measure_data(
    index: MeasureIndex,
    measure: &gscore_gp::Measure,
) -> Result<(), ModelError> {
    let view = MeasureView::new(index, measure);
    check_measure(&view).map_err(|e| locate(e, &index.to_string()))
}

/// Prefixes the location to an `Invalid` message: "track 2: measure 5: ...".
pub(crate) fn locate(error: ModelError, location: &str) -> ModelError {
    match error {
        ModelError::Invalid(msg) => ModelError::Invalid(format!("{location}: {msg}")),
        other => other,
    }
}
