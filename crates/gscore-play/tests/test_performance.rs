//! The engine-neutral performance: strings, frets, attacks and techniques of every note, as
//! a modelled guitar needs them, and the MIDI sequence derived from the same notes.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use gscore_model::testing::{self, TestNote};
use gscore_model::{
    BeatEffect, BeatIndex, BeatPosition, HarmonicKind, InstrumentKind, MeasureIndex, NoteEffect,
    SlapKind, Song, StringNumber, StrokeDirection, TrackIndex, Trill, TrillSpeed, Vibrato,
    VoiceIndex,
};
use gscore_play::{
    Attack, Click, MidiEvent, PPQ, Performance, PlayedNote, Sequence, count_in, lead_in,
};

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

fn open(relative: &str) -> Song {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative);
    gscore_io::import_guitar_pro_from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .song
}

/// Two quarter notes on string 3: fret 5, then fret 7.
fn two_notes() -> Song {
    testing::song_with_notes(&[(4, &[TestNote::new(3, 5)]), (4, &[TestNote::new(3, 7)])]).unwrap()
}

fn notes(song: &Song) -> Vec<PlayedNote> {
    Performance::new(song, None).unwrap().notes
}

#[test]
fn notes_carry_their_string_fret_and_pitch() {
    let performance = Performance::new(&two_notes(), None).unwrap();
    let track = &performance.tracks[0];
    assert_eq!(track.kind, InstrumentKind::Strings);
    assert_eq!(
        track.strings,
        [64, 59, 55, 50, 45, 40],
        "E standard, string 1 first"
    );
    let quarter = u64::from(PPQ);
    let played: Vec<(u8, Option<u8>, u8, u64, u64, Attack)> = performance
        .notes
        .iter()
        .map(|n| (n.string, n.fret, n.key, n.start, n.end, n.attack))
        .collect();
    assert_eq!(
        played,
        [
            (3, Some(5), 60, 0, quarter, Attack::Picked),
            (3, Some(7), 62, quarter, 2 * quarter, Attack::Picked),
        ]
    );
}

#[test]
fn a_hammer_on_reaches_the_next_note_without_a_pick() {
    let mut song = two_notes();
    let _ = song
        .set_note_effect(at(0), string(3), NoteEffect::Hammer(true))
        .unwrap();
    let played = notes(&song);
    assert_eq!(played[0].attack, Attack::Picked);
    assert_eq!(played[1].attack, Attack::Legato);
    let mut tapped = two_notes();
    let _ = tapped
        .set_note_effect(at(1), string(3), NoteEffect::LeftHandTap(true))
        .unwrap();
    assert_eq!(notes(&tapped)[1].attack, Attack::LeftHandTap);
}

#[test]
fn a_trill_hammers_between_its_two_frets() {
    let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 5)])]).unwrap();
    let _ = song
        .set_note_effect(
            at(0),
            string(3),
            NoteEffect::Trill(Some(Trill {
                fret: Some(7),
                speed: TrillSpeed::Sixteenth,
            })),
        )
        .unwrap();
    let played = notes(&song);
    let frets: Vec<Option<u8>> = played.iter().map(|n| n.fret).collect();
    assert_eq!(frets, [Some(5), Some(7), Some(5), Some(7)]);
    let attacks: Vec<Attack> = played.iter().map(|n| n.attack).collect();
    assert_eq!(
        attacks,
        [
            Attack::Picked,
            Attack::Legato,
            Attack::Legato,
            Attack::Legato
        ]
    );
}

#[test]
fn techniques_are_marked_on_the_notes() {
    let mut song = testing::song_with_notes(&[
        (4, &[TestNote::new(6, 3), TestNote::new(5, 5)]),
        (4, &[TestNote::new(3, 12)]),
        (4, &[TestNote::new(3, 5)]),
    ])
    .unwrap();
    for on in [5, 6] {
        let _ = song
            .set_note_effect(at(0), string(on), NoteEffect::PalmMute(true))
            .unwrap();
    }
    let _ = song
        .set_beat_effect(at(0), BeatEffect::PickStroke(Some(StrokeDirection::Up)))
        .unwrap();
    let _ = song
        .set_note_effect(
            at(1),
            string(3),
            NoteEffect::Harmonic(Some(gscore_model::Harmonic {
                kind: HarmonicKind::Natural,
                node: None,
            })),
        )
        .unwrap();
    let _ = song
        .set_beat_effect(at(2), BeatEffect::Slap(Some(SlapKind::Popping)))
        .unwrap();
    let played = notes(&song);
    assert!(played[..2].iter().all(|n| n.palm_mute && n.damped));
    assert!(
        played[..2]
            .iter()
            .all(|n| n.stroke == Some(StrokeDirection::Up))
    );
    assert_eq!(played[2].harmonic, Some(HarmonicKind::Natural));
    assert_eq!(played[3].attack, Attack::Popped);
}

#[test]
fn a_bend_reaches_its_target_in_cents() {
    let song = testing::song_with_notes(&[(2, &[TestNote::new(3, 7).with_bend(4)])]).unwrap();
    let played = notes(&song);
    let shape = played[0].pitch.unwrap();
    assert!(shape.cents_at(0).abs() < 1e-9, "starts unbent");
    let held = shape.cents_at(u64::from(PPQ) + 10);
    assert!(
        (held - 200.0).abs() < 1e-9,
        "a whole step after a beat: {held}"
    );
    assert_eq!(shape.reach_cents(), 200);
}

#[test]
fn a_vibrato_swings_around_the_pitch() {
    let mut song = testing::song_with_notes(&[(2, &[TestNote::new(3, 7)])]).unwrap();
    let _ = song
        .set_note_effect(at(0), string(3), NoteEffect::Vibrato(Some(Vibrato::Wide)))
        .unwrap();
    let shape = notes(&song)[0].pitch.unwrap();
    let swings: Vec<f64> = (0..u64::from(PPQ)).map(|t| shape.cents_at(t)).collect();
    let high = swings.iter().copied().fold(f64::MIN, f64::max);
    let low = swings.iter().copied().fold(f64::MAX, f64::min);
    assert!(high > 90.0 && low < -90.0, "{low}..{high}");
}

/// Every note of the performance is one note-on of the MIDI sequence, at the same time and
/// pitch: both engines play the same notes.
#[test]
fn the_midi_sequence_plays_the_notes_of_its_performance() {
    for name in ["ornaments", "advanced_ornaments", "multi_staff"] {
        let song = open(&format!("basic/{name}.gp"));
        let sequence = Sequence::new(&song, None).unwrap();
        let mut from_notes: Vec<(u64, u8, u8)> = sequence
            .performance
            .notes
            .iter()
            .map(|n| (n.start, n.key, n.velocity))
            .collect();
        let mut from_midi: Vec<(u64, u8, u8)> = sequence
            .events
            .iter()
            .filter_map(|e| match e.event {
                MidiEvent::NoteOn { key, velocity, .. } => Some((e.tick, key, velocity)),
                _ => None,
            })
            .collect();
        from_notes.sort_unstable();
        from_midi.sort_unstable();
        assert_eq!(from_notes, from_midi, "{name}");
        assert_eq!(sequence.performance.end, sequence.end);
    }
}

#[test]
fn an_excerpt_and_a_lead_in_move_their_notes_with_their_events() {
    let song = two_notes();
    let excerpt = Sequence::beat_excerpt(&song, at(1)).unwrap().unwrap();
    let notes = &excerpt.performance.notes;
    assert_eq!(notes.len(), 1);
    assert_eq!(
        (notes[0].start, notes[0].onset, notes[0].fret),
        (0, 0, Some(7))
    );

    let sequence = Sequence::new(&song, None).unwrap();
    let clicks = count_in(&song, at(1), 1, Click::default()).unwrap();
    assert!(clicks.performance.notes.is_empty());
    let quarter = u64::from(PPQ);
    let led = lead_in(&sequence, &clicks, quarter);
    let moved: Vec<(u64, Option<u8>)> = led
        .performance
        .notes
        .iter()
        .map(|n| (n.start, n.fret))
        .collect();
    assert_eq!(
        moved,
        [(clicks.end, Some(7))],
        "the first note is before the start"
    );
    assert_eq!(led.performance.end, led.end);
}
