//! Property tests: random command sequences keep the model valid, and undoing everything
//! (then redoing everything) returns to the same songs.
//!
//! A small deterministic pseudo-random generator keeps the test dependency-free and the
//! failures reproducible: the same seed produces the same sequence.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use common::test_songs;
use gscore_edit::{Command, Editor};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, NoteValue, Pitch, SONG_TEXT_FIELDS, Song,
    StringNumber, TrackIndex, VoiceIndex, differences,
};

/// A tiny linear congruential generator (the constants are Knuth's).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            usize::try_from(self.next()).unwrap_or(0) % bound
        }
    }
}

fn durations() -> [NoteValue; 6] {
    [
        NoteValue::new(1, 0, None).unwrap(),
        NoteValue::new(4, 0, None).unwrap(),
        NoteValue::new(8, 1, None).unwrap(),
        NoteValue::new(16, 2, None).unwrap(),
        NoteValue::new(8, 0, Some((3, 2))).unwrap(),
        NoteValue::new(32, 0, Some((5, 4))).unwrap(),
    ]
}

fn random_command(rng: &mut Rng, song: &Song) -> Command {
    let track = rng.below(song.track_count().saturating_add(2));
    let measure = rng.below(song.measure_count().saturating_add(2));
    let at = BeatPosition {
        track: TrackIndex::new(track),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(rng.below(3)),
        beat: BeatIndex::new(rng.below(8)),
    };
    let string = StringNumber::new(u8::try_from(rng.below(12).saturating_add(1)).unwrap_or(1))
        .unwrap_or_else(|_| StringNumber::new(1).unwrap());
    let fret = Fret::new(u8::try_from(rng.below(24)).unwrap_or(0))
        .unwrap_or_else(|_| Fret::new(0).unwrap());
    let all = durations();
    let duration = all[rng.below(all.len())];
    let any_track = TrackIndex::new(track);
    let pitch = Pitch::new(u8::try_from(rng.below(128)).unwrap_or(0)).unwrap();
    let field = SONG_TEXT_FIELDS[rng.below(SONG_TEXT_FIELDS.len())];
    match rng.below(18) {
        0 => Command::SetFret { at, string, fret },
        1 => Command::ClearNote { at, string },
        2 => Command::SetDuration { at, duration },
        3 => Command::InsertBeat { at, duration },
        4 => Command::DeleteBeat { at },
        5 => Command::InsertMeasure {
            measure: MeasureIndex::new(measure),
        },
        6 => Command::DeleteMeasure {
            measure: MeasureIndex::new(measure),
        },
        7 => Command::SetTrackProgram {
            track: any_track,
            program: u8::try_from(rng.below(128)).unwrap_or(0),
        },
        8 => Command::SetTrackDrums {
            track: any_track,
            drums: rng.below(2) == 1,
        },
        9 => Command::SetTrackCapo {
            track: any_track,
            capo: u8::try_from(rng.below(12)).unwrap_or(0),
        },
        10 => Command::SetTrackStrings {
            track: any_track,
            strings: rng.below(12).saturating_add(1),
        },
        11 => Command::SetTrackStringPitch {
            track: any_track,
            string,
            pitch,
        },
        12 => Command::SetSongText {
            field,
            text: format!("value {}", rng.below(1000)),
        },
        13 => Command::SetSystemBreak {
            track: any_track,
            measure: MeasureIndex::new(measure),
            kind: [
                gscore_model::SystemBreak::None,
                gscore_model::SystemBreak::Line,
                gscore_model::SystemBreak::Page,
            ][rng.below(3)],
        },
        14 => Command::SetBarsPerSystem {
            track: any_track,
            bars: u8::try_from(rng.below(40)).ok().filter(|bars| *bars > 0),
        },
        15 => Command::SetPageSetup {
            setup: gscore_model::PageSetup {
                width_mm: u16::try_from(rng.below(1200)).unwrap_or(210),
                scale_percent: u16::try_from(rng.below(250)).unwrap_or(100),
                ..gscore_model::PageSetup::default()
            },
        },
        16 => Command::SetScoreLineBreak {
            measure: MeasureIndex::new(measure),
            on: rng.below(2) == 0,
        },
        _ => Command::SetTempo {
            tempo: u32::try_from(rng.below(1000).saturating_add(1)).unwrap_or(120),
        },
    }
}

#[test]
fn random_command_sequences_keep_invariants_and_undo_completely() {
    let mut rng = Rng(0x5EED_2026);
    let mut successes = 0_usize;
    for (name, original) in test_songs() {
        for round in 0..8 {
            let mut editor = Editor::new(original.clone());
            for _ in 0..64 {
                let command = random_command(&mut rng, editor.song());
                if editor.execute(command).is_ok() {
                    successes = successes.saturating_add(1);
                }
                // The model validates every edit; collecting issues must never panic.
                let _ = editor.song().issues();
            }

            let edited = editor.song().clone();
            while editor.can_undo() {
                editor.undo().unwrap();
            }
            assert!(
                differences(&original, editor.song()).is_empty(),
                "{name}, round {round}: undoing everything did not restore the song"
            );

            while editor.can_redo() {
                editor.redo().expect("redo after undo");
            }
            assert!(
                differences(&edited, editor.song()).is_empty(),
                "{name}, round {round}: redoing everything did not return to the edited song"
            );
        }
    }
    assert!(successes > 0, "no random command ever succeeded");
}
