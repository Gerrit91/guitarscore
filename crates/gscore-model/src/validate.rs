//! Invariants every [`Song`] satisfies. Checked once, when a song is created.
//!
//! Only problems that would make the song unusable for the editor are errors here.
//! Musical oddities that real files contain are reported by [`crate::issues`] instead.

use crate::error::ModelError;
use crate::position::StringNumber;
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
    for (expected, tuning) in (1_u8..).zip(track.tuning()?) {
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
        check_measure(&measure, count).map_err(|e| locate(e, &measure.index().to_string()))?;
    }
    Ok(())
}

fn check_measure(measure: &MeasureView<'_>, string_count: usize) -> Result<(), ModelError> {
    measure.time_signature()?;
    for voice in measure.voices() {
        for beat in voice.beats() {
            check_beat(&beat, string_count)
                .map_err(|e| locate(e, &format!("{}, {}", voice.index(), beat.index())))?;
        }
    }
    Ok(())
}

fn check_beat(beat: &BeatView<'_>, string_count: usize) -> Result<(), ModelError> {
    beat.duration()?;
    let mut used = [false; MAX_STRINGS];
    for note in beat.notes() {
        note.fret()?;
        let string = note.string()?;
        let slot = usize::from(string.get()).saturating_sub(1);
        if usize::from(string.get()) > string_count {
            return Err(ModelError::Invalid(format!(
                "note on {string}, but the track has {string_count} strings"
            )));
        }
        match used.get_mut(slot) {
            Some(taken) if *taken => return Err(two_notes_on(string)),
            Some(taken) => *taken = true,
            None => return Err(two_notes_on(string)),
        }
    }
    Ok(())
}

fn two_notes_on(string: StringNumber) -> ModelError {
    ModelError::Invalid(format!("two notes on {string} in one beat"))
}

/// Prefixes the location to an `Invalid` message: "track 2: measure 5: ...".
fn locate(error: ModelError, location: &str) -> ModelError {
    match error {
        ModelError::Invalid(msg) => ModelError::Invalid(format!("{location}: {msg}")),
        other => other,
    }
}
