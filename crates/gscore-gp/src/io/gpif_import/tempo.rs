//! Tempo changes inside a bar. GP7 writes every tempo change as a `Tempo` automation at a
//! position in its bar (a fraction from 0); the legacy model keeps a bar's tempo on its
//! header and a change later in the bar on the beat that starts there (a mix table change,
//! as GP3-5 store every tempo change). GuitarScore addition: such changes were folded into
//! the bar's tempo, so a written ritardando lost all but its last step.

use crate::io::Song;
use crate::model::{Beat, BeatStatus, MeasureHeader, MixTableChange, MixTableItem, Voice};

/// How close (a fraction of the bar) a beat must start to a change's position to take it.
const SLACK: f64 = 1e-3;

/// Where each beat of `voice` starts, in ticks from the start of its bar; `None` for a beat
/// that takes no time (a grace beat or a placeholder).
pub(crate) fn beat_starts(voice: &Voice) -> Vec<Option<u32>> {
    let mut at: u32 = 0;
    voice
        .beats
        .iter()
        .map(|beat| {
            if beat.grace || beat.status == BeatStatus::Empty {
                return None;
            }
            let start = at;
            at = at.saturating_add(beat.duration.time().unwrap_or(0));
            Some(start)
        })
        .collect()
}

/// A bar's length in ticks, from its time signature.
pub(crate) fn bar_length(header: &MeasureHeader) -> u32 {
    let signature = &header.time_signature;
    let beat = signature.denominator.time().unwrap_or(0);
    u32::try_from(signature.numerator)
        .unwrap_or(0)
        .saturating_mul(beat)
}

/// The tempo a beat's mix table changes to, if it changes it.
pub(crate) fn beat_tempo(beat: &Beat) -> Option<u16> {
    beat.effect
        .mix_table_change
        .as_ref()
        .and_then(|mix| mix.tempo.as_ref())
        .map(|tempo| tempo.value)
        .filter(|&tempo| tempo > 0)
}

/// Puts each `(bar, position, tempo)` change on the first beat that starts there: in the
/// first voice of the first track that has one (an empty voice has none). A change no beat
/// starts at is left out.
pub(super) fn place_mid_bar_tempos(song: &mut Song, changes: &[(usize, f64, u16)]) {
    for &(bar, position, tempo) in changes {
        // At the very end of a bar (Guitar Pro writes 1.0): the next bar's tempo, unless it
        // has its own.
        if position >= 1.0 - SLACK {
            if let Some(next) = song.measure_headers.get_mut(bar.saturating_add(1))
                && next.tempo <= 0
            {
                next.tempo = i32::from(tempo);
            }
            continue;
        }
        let Some(header) = song.measure_headers.get(bar) else {
            continue;
        };
        let length = f64::from(bar_length(header));
        let target = position * length;
        let beat = song.tracks.iter_mut().find_map(|track| {
            let voice = track.measures.get_mut(bar)?.voices.first_mut()?;
            let starts = beat_starts(voice);
            let index = starts.iter().position(|start| {
                start.is_some_and(|start| (f64::from(start) - target).abs() < SLACK * length)
            })?;
            voice.beats.get_mut(index)
        });
        if let Some(beat) = beat {
            let mix = beat
                .effect
                .mix_table_change
                .get_or_insert_with(MixTableChange::default);
            mix.tempo = Some(MixTableItem {
                value: tempo,
                ..MixTableItem::default()
            });
        }
    }
}

/// The tempo changes on beats after the start of their bar, as `(bar, position, tempo)`:
/// from every track and voice, one per bar and position.
pub(crate) fn mid_bar_tempos(song: &Song) -> Vec<(usize, f64, u16)> {
    let mut changes: Vec<(usize, f64, u16)> = Vec::new();
    for track in &song.tracks {
        for (bar, measure) in track.measures.iter().enumerate() {
            let Some(header) = song.measure_headers.get(bar) else {
                continue;
            };
            let length = f64::from(bar_length(header));
            if length <= 0.0 {
                continue;
            }
            for voice in &measure.voices {
                for (beat, start) in voice.beats.iter().zip(beat_starts(voice)) {
                    let (Some(tempo), Some(start)) = (beat_tempo(beat), start) else {
                        continue;
                    };
                    if start == 0 {
                        continue;
                    }
                    let position = f64::from(start) / length;
                    let known = changes.iter().any(|&(other_bar, other, _)| {
                        other_bar == bar && (other - position).abs() < SLACK
                    });
                    if !known {
                        changes.push((bar, position, tempo));
                    }
                }
            }
        }
    }
    changes.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    changes
}
