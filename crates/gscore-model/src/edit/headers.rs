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
use crate::metadata::MAX_TEMPO;
use crate::position::MeasureIndex;
use crate::song::Song;
use crate::style::TrackColor;

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
