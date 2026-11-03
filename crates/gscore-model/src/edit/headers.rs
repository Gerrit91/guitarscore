//! Song-wide header edits at one measure: its tempo, time and key signatures, and its
//! section marker. Every track keeps a copy of the measure's signatures, so all copies are
//! changed together and restored together.

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

    /// Sets a measure's feel (triplet feel), in the header and (at measure 1) the song's
    /// initial feel.
    pub fn set_feel(&mut self, measure: MeasureIndex, feel: Feel) -> Result<Change, ModelError> {
        self.edit_headers(|inner| {
            let header = header_mut(inner, measure)?;
            let previous = (header.triplet_feel.clone(), header.gs_feel.clone());
            header.triplet_feel = feel.to_gp();
            header.gs_feel = feel.gs_id().map(str::to_owned);
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
                Some(fermata) => vec![(fermata.to_gp().to_owned(), "0".to_owned())],
                None => Vec::new(),
            };
            Ok(Change {
                kind: ChangeKind::Fermata { measure, previous },
            })
        })
    }

    /// Sets the time signature of a measure, in the header and in every track's copy of it.
    pub fn set_time_signature(
        &mut self,
        measure: MeasureIndex,
        numerator: u8,
        denominator: u16,
    ) -> Result<Change, ModelError> {
        let signature = TimeSignature::new(numerator, denominator)?;
        let gp = gp_time_signature(&signature)?;
        let change = self.edit_headers(|inner| {
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
        })?;
        // The measure's layout depends on its signature: lay it out again.
        self.touch_all_measures();
        Ok(change)
    }

    /// Sets the key signature of a measure, in the header and in every track's copy of it.
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
