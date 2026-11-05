//! Song-wide header edits: a measure's tempo, repeats, section marker and fermata, and
//! the time and key signatures and feel, which hold from their measure up to the next
//! change. Every track keeps a copy of the measure's signatures, so all copies are
//! changed together and restored together.

use std::ops::Range;

use gscore_gp::{
    Duration as GpDuration, KeySignature as GpKeySignature, Marker as GpMarker, Song as GpSong,
    TimeSignature as GpTimeSignature,
};

use super::{Change, ChangeKind};
use crate::duration::TimeSignature;
use crate::error::ModelError;
use crate::marks::{Feel, Fermata};
use crate::metadata::MAX_TEMPO;
use crate::playback::Repeat;
use crate::position::MeasureIndex;
use crate::song::Song;
use crate::style::TrackColor;

/// Most repeats Guitar Pro stores for one section (it counts the first pass separately).
const MAX_REPEATS: u8 = 127;

/// Longest section title accepted (characters).
pub const MAX_SECTION_TITLE_CHARS: usize = 256;

impl Song {
    /// Sets the tempo a measure starts at (quarter notes per minute).
    pub fn set_measure_tempo(
        &mut self,
        measure: MeasureIndex,
        tempo: u32,
    ) -> Result<Change, ModelError> {
        if !(1..=MAX_TEMPO).contains(&tempo) {
            return Err(ModelError::Invalid(format!(
                "tempo {tempo} (supported: 1 to {MAX_TEMPO} BPM)"
            )));
        }
        let tempo = i16::try_from(tempo)
            .map_err(|_| ModelError::Invalid(format!("tempo {tempo} does not fit")))?;
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.tempo;
            header.tempo = i32::from(tempo);
            // The first measure's tempo is the song's initial tempo too; keep them equal so
            // the settings window and playback agree.
            let previous_initial = (measure.get() == 0).then(|| {
                let previous = inner.tempo;
                inner.tempo = tempo;
                previous
            });
            Ok(Change {
                kind: ChangeKind::MeasureTempo {
                    measure,
                    previous,
                    previous_initial,
                },
            })
        })
    }

    /// Sets a measure's repeat signs and alternative endings (the same for every track).
    /// `end_repeats` is how many times the section is played again: `Some(1)` is a plain
    /// repeat (two passes).
    pub fn set_repeat(
        &mut self,
        measure: MeasureIndex,
        repeat: Repeat,
    ) -> Result<Change, ModelError> {
        let close = match repeat.end_repeats {
            Some(repeats) if (1..=MAX_REPEATS).contains(&repeats) => i8::try_from(repeats)
                .map_err(|_| ModelError::Invalid(format!("{repeats} repeats do not fit")))?,
            Some(repeats) => {
                return Err(ModelError::Invalid(format!(
                    "{repeats} repeats (supported: 1 to {MAX_REPEATS}, or none)"
                )));
            }
            None => -1,
        };
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = (
                header.repeat_open,
                header.repeat_close,
                header.repeat_alternative,
            );
            header.repeat_open = repeat.start;
            header.repeat_close = close;
            header.repeat_alternative = repeat.endings;
            Ok(Change {
                kind: ChangeKind::Repeat { measure, previous },
            })
        })
    }

    /// Sets whether a measure is in free time (no metronome), the same for every track.
    pub fn set_free_time(
        &mut self,
        measure: MeasureIndex,
        free_time: bool,
    ) -> Result<Change, ModelError> {
        let change = self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.free_time;
            header.free_time = free_time;
            Ok(Change {
                kind: ChangeKind::FreeTime { measure, previous },
            })
        })?;
        self.touch_all_measures();
        Ok(change)
    }

    /// Sets whether a measure ends with a double barline (the same for every track).
    pub fn set_double_bar(
        &mut self,
        measure: MeasureIndex,
        double_bar: bool,
    ) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.double_bar;
            header.double_bar = double_bar;
            Ok(Change {
                kind: ChangeKind::DoubleBar { measure, previous },
            })
        })
    }

    /// Sets the feel (triplet feel) from `measure` on: the measure and the following ones
    /// that had its feel, up to the next measure with a feel of its own (see [`reach`]).
    /// One undoable change.
    pub fn set_feel(&mut self, measure: MeasureIndex, feel: Feel) -> Result<Change, ModelError> {
        let measures = reach(self, measure, |header| header.triplet_feel.clone())?;
        self.each_measure(measures, |song, at| song.set_feel_at(at, feel))
    }

    /// Sets one measure's feel, in the header and (at measure 1) the song's initial feel.
    fn set_feel_at(&mut self, measure: MeasureIndex, feel: Feel) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.triplet_feel.clone();
            header.triplet_feel = feel.to_gp();
            let previous_initial = (measure.get() == 0).then(|| {
                let previous = inner.triplet_feel.clone();
                inner.triplet_feel = feel.to_gp();
                previous
            });
            Ok(Change {
                kind: ChangeKind::Feel {
                    measure,
                    previous,
                    previous_initial,
                },
            })
        })
    }

    /// Sets or clears a measure's fermata (one fermata per measure for now).
    pub fn set_fermata(
        &mut self,
        measure: MeasureIndex,
        fermata: Option<Fermata>,
    ) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.fermatas.clone();
            header.fermatas = match fermata {
                Some(fermata) => vec![(fermata.to_gp().to_owned(), "0/1".to_owned())],
                None => Vec::new(),
            };
            Ok(Change {
                kind: ChangeKind::Fermata { measure, previous },
            })
        })
    }

    /// Sets the time signature from `measure` on: the measure and the following ones that
    /// had its signature, up to the next measure with a signature of its own (see
    /// [`reach`]). One undoable change.
    pub fn set_time_signature(
        &mut self,
        measure: MeasureIndex,
        numerator: u8,
        denominator: u16,
    ) -> Result<Change, ModelError> {
        let signature = TimeSignature::new(numerator, denominator)?;
        let gp = gp_time_signature(&signature)?;
        let measures = reach(self, measure, |header| {
            (
                header.time_signature.numerator,
                header.time_signature.denominator.value,
            )
        })?;
        let change = self.each_measure(measures, |song, at| song.set_time_signature_at(at, &gp))?;
        // The measures' layouts depend on their signature: lay them out again.
        self.touch_all_measures();
        Ok(change)
    }

    /// Sets one measure's time signature, in the header and in every track's copy of it.
    fn set_time_signature_at(
        &mut self,
        measure: MeasureIndex,
        gp: &GpTimeSignature,
    ) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let previous_measures = measures_time_signatures(inner, measure)?;
            let header = header_mut(inner, measure)?;
            let previous_header = header.time_signature.clone();
            header.time_signature = gp.clone();
            for track in &mut inner.tracks {
                if let Some(slot) = track.measures.get_mut(measure.get()) {
                    slot.time_signature = gp.clone();
                }
            }
            Ok(Change {
                kind: ChangeKind::TimeSignature {
                    measure,
                    previous_header,
                    previous_measures,
                },
            })
        })
    }

    /// Sets the key signature from `measure` on: the measure and the following ones that
    /// had its key, up to the next measure with a key of its own (see [`reach`]). One
    /// undoable change.
    pub fn set_key_signature(
        &mut self,
        measure: MeasureIndex,
        key: i8,
        minor: bool,
    ) -> Result<Change, ModelError> {
        if !(-7..=7).contains(&key) {
            return Err(ModelError::Invalid(format!(
                "key signature {key} (supported: -7 to 7 accidentals)"
            )));
        }
        let measures = reach(self, measure, |header| header.key_signature.clone())?;
        self.each_measure(measures, |song, at| {
            song.set_key_signature_at(at, key, minor)
        })
    }

    /// Sets one measure's key signature, in the header and in every track's copy of it.
    fn set_key_signature_at(
        &mut self,
        measure: MeasureIndex,
        key: i8,
        minor: bool,
    ) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let previous_measures = measures_key_signatures(inner, measure)?;
            let header = header_mut(inner, measure)?;
            let previous_header = header.key_signature.clone();
            let signature = GpKeySignature {
                key,
                is_minor: minor,
            };
            header.key_signature = signature.clone();
            for track in &mut inner.tracks {
                if let Some(slot) = track.measures.get_mut(measure.get()) {
                    slot.key_signature = signature.clone();
                }
            }
            Ok(Change {
                kind: ChangeKind::KeySignature {
                    measure,
                    previous_header,
                    previous_measures,
                },
            })
        })
    }

    /// Sets a measure's section marker, or removes it when `title` is empty.
    pub fn set_section(
        &mut self,
        measure: MeasureIndex,
        title: &str,
        color: TrackColor,
    ) -> Result<Change, ModelError> {
        let title = title.trim();
        if title.chars().count() > MAX_SECTION_TITLE_CHARS {
            return Err(ModelError::Invalid(format!(
                "the section title is longer than {MAX_SECTION_TITLE_CHARS} characters"
            )));
        }
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = header.marker.clone();
            header.marker = if title.is_empty() {
                None
            } else {
                Some(GpMarker {
                    title: title.to_owned(),
                    color: i32::try_from(color.rgb()).unwrap_or(0),
                })
            };
            Ok(Change {
                kind: ChangeKind::Section { measure, previous },
            })
        })
    }
}

impl Song {
    /// Runs `edit` on each measure of `measures` and combines the changes into one
    /// (a batch when there are several, undone in reverse order).
    fn each_measure(
        &mut self,
        measures: Range<usize>,
        mut edit: impl FnMut(&mut Song, MeasureIndex) -> Result<Change, ModelError>,
    ) -> Result<Change, ModelError> {
        let mut changes = Vec::new();
        for index in measures {
            match edit(self, MeasureIndex::new(index)) {
                Ok(change) => changes.push(change),
                Err(error) => {
                    // Leave the song as it was: undo the measures already changed.
                    for change in changes.iter().rev() {
                        self.revert(change)?;
                    }
                    return Err(error);
                }
            }
        }
        if changes.len() == 1 {
            return changes
                .pop()
                .ok_or_else(|| ModelError::Invalid("no change".to_owned()));
        }
        Ok(Change::batch(changes))
    }
}

/// The measures a signature or feel set at `measure` reaches, as Guitar Pro applies
/// them: `measure` itself and the following measures that carried the same value as it,
/// up to the next measure whose value differs (where a change was written already).
fn reach<T: PartialEq>(
    song: &Song,
    measure: MeasureIndex,
    value: impl Fn(&gscore_gp::MeasureHeader) -> T,
) -> Result<Range<usize>, ModelError> {
    let headers = &song.inner().measure_headers;
    let start = measure.get();
    let first = headers.get(start).ok_or_else(|| {
        ModelError::Invalid(format!(
            "{measure}: the song has {} measure(s)",
            headers.len()
        ))
    })?;
    let old = value(first);
    let end = headers
        .iter()
        .enumerate()
        .skip(start.saturating_add(1))
        .find(|(_, header)| value(header) != old)
        .map_or(headers.len(), |(index, _)| index);
    Ok(start..end)
}

/// The header of `measure`, or an error naming what is missing.
fn header_mut(
    song: &mut GpSong,
    measure: MeasureIndex,
) -> Result<&mut gscore_gp::MeasureHeader, ModelError> {
    let count = song.measure_headers.len();
    song.measure_headers
        .get_mut(measure.get())
        .ok_or_else(|| ModelError::Invalid(format!("{measure}: the song has {count} measure(s)")))
}

/// Each track's time signature copy for `measure`.
fn measures_time_signatures(
    song: &GpSong,
    measure: MeasureIndex,
) -> Result<Vec<GpTimeSignature>, ModelError> {
    song.tracks
        .iter()
        .map(|track| {
            track
                .measures
                .get(measure.get())
                .map(|m| m.time_signature.clone())
                .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))
        })
        .collect()
}

/// Each track's key signature copy for `measure`.
fn measures_key_signatures(
    song: &GpSong,
    measure: MeasureIndex,
) -> Result<Vec<GpKeySignature>, ModelError> {
    song.tracks
        .iter()
        .map(|track| {
            track
                .measures
                .get(measure.get())
                .map(|m| m.key_signature.clone())
                .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))
        })
        .collect()
}

/// The Guitar Pro time signature for a checked [`TimeSignature`].
pub(super) fn gp_time_signature(signature: &TimeSignature) -> Result<GpTimeSignature, ModelError> {
    let numerator = i8::try_from(signature.numerator())
        .map_err(|_| ModelError::Invalid(format!("{} beats do not fit", signature.numerator())))?;
    let denominator = GpDuration {
        value: signature.denominator(),
        ..GpDuration::default()
    };
    // Guitar Pro stores how beats are beamed; two eighths per group is a safe default.
    let beams = vec![2; usize::from(signature.numerator())];
    Ok(GpTimeSignature {
        numerator,
        denominator,
        beams,
    })
}
