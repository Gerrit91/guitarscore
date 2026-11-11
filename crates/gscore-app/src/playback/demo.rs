//! The phrase the Sound chain window plays to try a sound: an open low string, a strummed
//! chord, palm-muted eighths and a hammered lick on a guitar; a walking line with a
//! muted run on a bass. Made directly as a [`Performance`], so no song is needed.

use std::sync::Arc;

use gscore_model::{InstrumentKind, TrackIndex, TrackSound};
use gscore_play::{
    Attack, PPQ, Performance, PerformanceTrack, PlayedNote, Sequence, StringKind, StrokeDirection,
    TempoMap, TrackEngine,
};

/// The phrase's tempo.
const BPM: u32 = 100;
/// A guitar in E standard and a bass in E, string 1 (the highest) first.
const GUITAR: [u8; 6] = [64, 59, 55, 50, 45, 40];
const BASS: [u8; 4] = [43, 38, 33, 28];

/// One note: `string`, `fret`, from `beat` for `beats` (quarters), how it is struck.
struct Step {
    string: u8,
    fret: u8,
    beat: f32,
    beats: f32,
    attack: Attack,
    palm_mute: bool,
    stroke: Option<StrokeDirection>,
}

const fn step(string: u8, fret: u8, beat: f32, beats: f32) -> Step {
    Step {
        string,
        fret,
        beat,
        beats,
        attack: Attack::Picked,
        palm_mute: false,
        stroke: None,
    }
}

/// An E major chord struck down, string 6 first, a few milliseconds apart: the preview's
/// first (and only) chord, so a tone is heard at once without waiting through a phrase.
const fn strum(string: u8, fret: u8) -> Step {
    Step {
        stroke: Some(StrokeDirection::Down),
        ..step(string, fret, (6 - string) as f32 * 0.03, 2.0)
    }
}

const GUITAR_PHRASE: [Step; 6] = [
    strum(6, 0),
    strum(5, 2),
    strum(4, 2),
    strum(3, 1),
    strum(2, 0),
    strum(1, 0),
];

const BASS_PHRASE: [Step; 1] = [step(4, 0, 0.0, 2.0)];

/// The phrase for `kind`, ready to play.
pub(super) fn phrase(kind: StringKind, track: usize) -> Sequence {
    let (strings, steps, program): (&[u8], &[Step], u8) = match kind {
        StringKind::Bass => (&BASS, &BASS_PHRASE, 33),
        StringKind::Nylon => (&GUITAR, &GUITAR_PHRASE, 24),
        StringKind::Steel => (&GUITAR, &GUITAR_PHRASE, 25),
        StringKind::Electric => (&GUITAR, &GUITAR_PHRASE, 27),
    };
    let quarter = f32::from(u16::try_from(PPQ).unwrap_or(960));
    let tick = |beats: f32| (beats * quarter).max(0.0) as u64;
    let notes: Vec<PlayedNote> = steps
        .iter()
        .map(|step| {
            let open = strings
                .get(usize::from(step.string.saturating_sub(1)))
                .copied()
                .unwrap_or(40);
            PlayedNote {
                track: 0,
                string: step.string,
                fret: Some(step.fret),
                key: open.saturating_add(step.fret),
                velocity: if step.attack == Attack::Legato {
                    76
                } else {
                    95
                },
                start: tick(step.beat),
                end: tick(step.beat + step.beats),
                onset: tick(step.beat),
                attack: step.attack,
                stroke: step.stroke,
                palm_mute: step.palm_mute,
                dead: false,
                harmonic: None,
                let_ring: false,
                // A palm mute is damped by the string itself, not stopped dead.
                damped: false,
                grace: false,
                pitch: None,
                fade: None,
            }
        })
        .collect();
    let end = notes
        .iter()
        .map(|note| note.end)
        .max()
        .unwrap_or(0)
        .saturating_add(tick(2.0));
    let mut performance = Performance {
        tracks: vec![PerformanceTrack {
            // Played as this track of the song, so it is heard with that track's tone.
            track: TrackIndex::new(track),
            name: "String sound".to_owned(),
            sound: TrackSound {
                program,
                volume: 100,
                pan: 64,
                reverb: 0,
                chorus: 0,
                percussion: false,
            },
            kind: InstrumentKind::Strings,
            strings: strings.to_vec(),
            engine: TrackEngine::Modelled(kind),
        }],
        notes,
        tempo: TempoMap::new(BPM),
        measures: Vec::new(),
        end,
        skipped_notes: 0,
        by_start: Vec::new(),
    };
    performance.index_by_start();
    Sequence::from_performance(Arc::new(performance))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn every_kind_has_a_phrase_on_its_strings() {
        for kind in crate::playback::STRING_KINDS {
            let sequence = phrase(kind, 0);
            let performance = &sequence.performance;
            assert_eq!(performance.tracks[0].engine, TrackEngine::Modelled(kind));
            assert!(!performance.notes.is_empty());
            let strings = performance.tracks[0].strings.len();
            assert!(
                performance
                    .notes
                    .iter()
                    .all(|n| usize::from(n.string) <= strings)
            );
            assert!(sequence.end > performance.notes.iter().map(|n| n.end).max().unwrap());
            assert_eq!(performance.by_start.len(), performance.notes.len());
        }
    }
}
