//! Whole-beat and whole-note switches the edit palette uses: making a beat a rest and
//! tying a note to the one before it.

use std::collections::HashSet;

use gscore_gp::{BeatStatus, NoteType};

use super::{Change, ChangeKind};
use crate::error::ModelError;
use crate::position::{BeatIndex, BeatPosition, MeasureIndex, StringNumber};
use crate::song::Song;
use crate::views::NoteKind;

impl Song {
    /// Sets the text written above the beat (Guitar Pro's beat text); an empty text removes
    /// it. The text is what is shown, not a section title.
    pub fn set_beat_text(&mut self, at: BeatPosition, text: &str) -> Result<Change, ModelError> {
        if text.chars().count() > crate::techniques::MAX_BEAT_TEXT_CHARS {
            return Err(ModelError::Invalid(format!(
                "the beat text is longer than {} characters",
                crate::techniques::MAX_BEAT_TEXT_CHARS
            )));
        }
        let previous = self
            .beat(at)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no such beat")))?
            .text()
            .to_owned();
        let text = text.to_owned();
        self.edit_measure(at.track, at.measure, move |measure| {
            beat_mut(measure, at)?.text = text;
            Ok(Change {
                kind: ChangeKind::BeatText { at, previous },
            })
        })
    }

    /// Makes the beat a rest: its notes go and its status becomes a rest. The length and
    /// the beat's own effects stay. Undo puts the notes back.
    pub fn set_beat_rest(&mut self, at: BeatPosition) -> Result<Change, ModelError> {
        let previous_status = self
            .beat(at)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no such beat")))?
            .inner
            .status
            .clone();
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let notes = std::mem::take(&mut beat.notes);
            beat.status = BeatStatus::Rest;
            Ok(Change {
                kind: ChangeKind::BeatRest {
                    at,
                    previous_status,
                    notes,
                },
            })
        })
    }

    /// Makes the note on `string` a dead (muted, "x") note, or a normal note again.
    pub fn set_note_dead(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        dead: bool,
    ) -> Result<Change, ModelError> {
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let note = beat
                .notes
                .iter_mut()
                .find(|note| u8::try_from(note.string).ok() == Some(string.get()))
                .ok_or_else(|| ModelError::Invalid(format!("{at} has no {string}")))?;
            let previous = note.kind.clone();
            note.kind = if dead {
                NoteType::Dead
            } else {
                NoteType::Normal
            };
            Ok(Change {
                kind: ChangeKind::NoteDead {
                    at,
                    string,
                    previous,
                },
            })
        })
    }

    /// The notes of the beat that continue a note of the previous beat: their strings.
    fn tieable_strings(&self, at: BeatPosition) -> Option<HashSet<u8>> {
        let previous = previous_beat(self, at)?;
        let strings: HashSet<u8> = self
            .beat(previous)?
            .notes()
            .filter_map(|note| note.string().ok().map(|string| string.get()))
            .collect();
        (!strings.is_empty()).then_some(strings)
    }

    /// Whether every note of the beat that can continue the previous beat is already tied.
    /// `None` when no note of the beat has a note on its string in the previous beat.
    pub fn beat_tie_state(&self, at: BeatPosition) -> Option<bool> {
        let strings = self.tieable_strings(at)?;
        let mut found = false;
        let mut all_tied = true;
        for note in self.beat(at)?.notes() {
            let Ok(string) = note.string() else {
                continue;
            };
            if !strings.contains(&string.get()) {
                continue;
            }
            found = true;
            if note.kind() != NoteKind::Tie {
                all_tied = false;
            }
        }
        found.then_some(all_tied)
    }

    /// Ties every note of the beat to the note on its string in the previous beat, or
    /// removes those ties. A chord gets one tie per string that the previous beat plays.
    pub fn set_beat_tie(&mut self, at: BeatPosition, tie: bool) -> Result<Change, ModelError> {
        let strings = self.tieable_strings(at).ok_or_else(|| {
            ModelError::Invalid(format!("{at}: no note to tie to the previous beat"))
        })?;
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let mut changes = Vec::new();
            for note in &mut beat.notes {
                let Some(string) = u8::try_from(note.string)
                    .ok()
                    .and_then(|number| StringNumber::new(number).ok())
                else {
                    continue;
                };
                if !strings.contains(&string.get()) {
                    continue;
                }
                let previous = note.kind.clone();
                let wanted = if tie { NoteType::Tie } else { NoteType::Normal };
                if previous == wanted {
                    continue;
                }
                note.kind = wanted;
                changes.push(Change {
                    kind: ChangeKind::NoteTie {
                        at,
                        string,
                        previous,
                    },
                });
            }
            Ok(Change {
                kind: ChangeKind::Batch { changes },
            })
        })
    }

    /// Ties the note on `string` to the previous one (or removes the tie). A tie needs the
    /// note to exist; whether it makes musical sense is the user's call.
    pub fn set_note_tie(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        tie: bool,
    ) -> Result<Change, ModelError> {
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let note = beat
                .notes
                .iter_mut()
                .find(|note| u8::try_from(note.string).ok() == Some(string.get()))
                .ok_or_else(|| ModelError::Invalid(format!("{at} has no {string}")))?;
            let previous = note.kind.clone();
            note.kind = if tie { NoteType::Tie } else { NoteType::Normal };
            Ok(Change {
                kind: ChangeKind::NoteTie {
                    at,
                    string,
                    previous,
                },
            })
        })
    }
}

/// The beat before `at` in the same voice, also across a barline; `Empty` placeholders
/// are skipped.
fn previous_beat(song: &Song, at: BeatPosition) -> Option<BeatPosition> {
    let track = song.track(at.track)?;
    if at.beat.get() > 0 {
        let voice = track.measure(at.measure)?.voice(at.voice)?;
        // A hostile command may carry a huge beat index: never search past the voice.
        let count = voice.beats().len().min(at.beat.get());
        let index = (0..count).rev().find(|&index| {
            voice
                .beat(BeatIndex::new(index))
                .is_some_and(|beat| beat.kind() != crate::views::BeatKind::Empty)
        })?;
        return Some(BeatPosition {
            beat: BeatIndex::new(index),
            ..at
        });
    }
    let previous = at.measure.get().checked_sub(1)?;
    let measure = track.measure(MeasureIndex::new(previous))?;
    let voice = measure.voice(at.voice)?;
    let index = (0..voice.beats().len()).rev().find(|&index| {
        voice
            .beat(BeatIndex::new(index))
            .is_some_and(|beat| beat.kind() != crate::views::BeatKind::Empty)
    })?;
    Some(BeatPosition {
        measure: MeasureIndex::new(previous),
        beat: BeatIndex::new(index),
        ..at
    })
}

/// The beat at `at` in a measure being edited.
fn beat_mut(
    measure: &mut gscore_gp::Measure,
    at: BeatPosition,
) -> Result<&mut gscore_gp::Beat, ModelError> {
    let voices = measure.voices.len();
    let voice = measure
        .voices
        .get_mut(at.voice.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the measure has {voices} voice(s)")))?;
    let beats = voice.beats.len();
    voice
        .beats
        .get_mut(at.beat.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the voice has {beats} beat(s)")))
}
