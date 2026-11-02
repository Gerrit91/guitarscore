//! Track tuning edits: the capo, the number of strings and one string's open pitch.
//!
//! These are per-track settings, so they use [`Song::edit_track_settings`] and keep every
//! measure's layout stamp; the layout cache keys a track's layout on its string count,
//! tuning and capo, which makes it lay the affected measures out again.

use super::super::{Change, ChangeKind, MAX_STRINGS, track_mut};
use crate::error::ModelError;
use crate::position::{Pitch, StringNumber, TrackIndex};
use crate::song::Song;

/// Semitones between neighbouring strings when a track grows a lower one: the standard
/// fourth of guitar and bass tunings.
const FOURTH: i8 = 5;
/// The lowest string's pitch of a track that somehow has none (a validated song never
/// does), used as the starting point for added strings.
const FALLBACK_LOWEST: i8 = 40;

impl Song {
    /// Sets the capo fret (0 for no capo, up to 127).
    pub fn set_track_capo(&mut self, at: TrackIndex, capo: u8) -> Result<Change, ModelError> {
        if capo > 127 {
            return Err(ModelError::Invalid(format!(
                "capo {capo} is above the MIDI range"
            )));
        }
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.offset;
            track.offset = i32::from(capo);
            Ok(Change {
                kind: ChangeKind::TrackCapo { at, previous },
            })
        })
    }

    /// Whether the track's string count can become `count`: 1 to [`MAX_STRINGS`], not the
    /// count it has, and no notes left on the strings that would go. Commands use this to
    /// decide whether they are enabled, so the answer always matches [`Self::set_track_strings`].
    pub fn check_track_strings(&self, at: TrackIndex, count: usize) -> Result<(), ModelError> {
        if !(1..=MAX_STRINGS).contains(&count) {
            return Err(ModelError::Invalid(format!(
                "{count} strings (supported: 1 to {MAX_STRINGS})"
            )));
        }
        let track = self
            .track(at)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no such track")))?;
        let current = track.string_count();
        if count == current {
            return Err(ModelError::Invalid(format!(
                "the track already has {current} strings"
            )));
        }
        if count < current {
            let notes = track
                .measures()
                .flat_map(|measure| measure.voices().flat_map(|voice| voice.beats()))
                .flat_map(|beat| beat.notes())
                .filter(|note| {
                    note.string()
                        .ok()
                        .is_some_and(|string| usize::from(string.get()) > count)
                })
                .count();
            if notes > 0 {
                return Err(ModelError::Invalid(format!(
                    "{notes} note(s) are on the strings that would be removed; clear them first"
                )));
            }
        }
        Ok(())
    }

    /// Changes the number of strings, keeping the existing ones. Going down removes the
    /// lowest strings, and is refused while notes are on them; going up adds lower strings
    /// in fourths, the usual way an extended-range instrument is tuned.
    pub fn set_track_strings(
        &mut self,
        at: TrackIndex,
        count: usize,
    ) -> Result<Change, ModelError> {
        self.check_track_strings(at, count)?;
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.strings.clone();
            let previous_has_tuning = track.has_tuning;
            let previous_label = track.tuning_label.clone();
            let current = track.strings.len();
            if count > current {
                let mut pitch = track
                    .strings
                    .last()
                    .map_or(FALLBACK_LOWEST, |(_, pitch)| *pitch);
                for number in current..count {
                    pitch = pitch.saturating_sub(FOURTH);
                    let number = number.saturating_add(1);
                    let number = i8::try_from(number).map_err(|_| {
                        ModelError::Invalid("too many strings for a Guitar Pro track".to_owned())
                    })?;
                    track.strings.push((number, pitch));
                }
            } else {
                track.strings.truncate(count);
            }
            // An explicit string count is a tuning the user set, even for a track whose
            // file had none (a vocal or piano track); the old name no longer fits.
            track.has_tuning = true;
            track.tuning_label = None;
            Ok(Change {
                kind: ChangeKind::TrackStrings {
                    at,
                    previous,
                    previous_has_tuning,
                    previous_label,
                },
            })
        })
    }

    /// Sets the open pitch of one string.
    pub fn set_track_string_pitch(
        &mut self,
        at: TrackIndex,
        string: StringNumber,
        pitch: Pitch,
    ) -> Result<Change, ModelError> {
        let value = i8::try_from(pitch.get())
            .map_err(|_| ModelError::Invalid(format!("pitch {pitch} is above MIDI's range")))?;
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let entry = track
                .strings
                .iter_mut()
                .find(|(number, _)| u8::try_from(*number).ok() == Some(string.get()))
                .ok_or_else(|| ModelError::Invalid(format!("{at} has no {string}")))?;
            let previous = entry.1;
            entry.1 = value;
            let previous_has_tuning = track.has_tuning;
            let previous_label = track.tuning_label.clone();
            // The user gave this track a tuning; a name from the file no longer describes it.
            track.has_tuning = true;
            track.tuning_label = None;
            Ok(Change {
                kind: ChangeKind::TrackStringPitch {
                    at,
                    string,
                    previous,
                    previous_has_tuning,
                    previous_label,
                },
            })
        })
    }
}
