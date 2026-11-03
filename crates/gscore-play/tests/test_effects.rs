//! Note and beat effects in playback (vibrato, ghost notes, accents, palm mute, let ring,
//! brush strokes) and the one-beat excerpt the app plays for a palette click.

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
    AccentKind, BeatEffect, BeatIndex, BeatPosition, MeasureIndex, NoteEffect, Song, StringNumber,
    StrokeDirection, TrackIndex, Vibrato, VoiceIndex,
};
use gscore_play::{MidiEvent, PPQ, Sequence, controller};

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

/// Note-ons as `(tick, key, velocity)`.
fn note_ons(sequence: &Sequence) -> Vec<(u64, u8, u8)> {
    sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOn { key, velocity, .. } => Some((event.tick, key, velocity)),
            _ => None,
        })
        .collect()
}

/// Note-offs as `(tick, key)`.
fn note_offs(sequence: &Sequence) -> Vec<(u64, u8)> {
    sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOff { key, .. } => Some((event.tick, key)),
            _ => None,
        })
        .collect()
}

/// A half note on string 6 and a half note on string 5 (fret 3 each).
fn two_beats() -> Song {
    testing::song_with_notes(&[(2, &[TestNote::new(6, 3)]), (2, &[TestNote::new(5, 3)])]).unwrap()
}

#[test]
fn a_vibrato_shakes_the_pitch_on_a_borrowed_channel() {
    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Vibrato(Some(Vibrato::Wide)))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let bends: Vec<u16> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::PitchBend { channel, value } if channel != base => Some(value),
            _ => None,
        })
        .collect();
    assert!(bends.iter().any(|&value| value > 8192 + 100), "{bends:?}");
    assert!(bends.iter().any(|&value| value < 8192 - 100), "{bends:?}");
    // The plain second note stays on the track's channel.
    let channels: Vec<_> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOn { channel, .. } => Some(channel),
            _ => None,
        })
        .collect();
    assert_ne!(channels[0], base);
    assert_eq!(channels[1], base);
}

#[test]
fn ghost_notes_are_softer_and_accents_louder() {
    let plain = note_ons(&Sequence::new(&two_beats(), None).unwrap());
    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Ghost(true))
        .unwrap();
    let _ = song
        .set_note_effect(
            at(1),
            string(5),
            NoteEffect::Accent(Some(AccentKind::Heavy)),
        )
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    assert!(ons[0].2 < plain[0].2);
    assert!(ons[1].2 > plain[1].2);
}

#[test]
fn palm_mute_shortens_and_let_ring_holds() {
    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::PalmMute(true))
        .unwrap();
    let offs = note_offs(&Sequence::new(&song, None).unwrap());
    assert!(offs[0].0 <= u64::from(PPQ) / 2);

    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::LetRing(true))
        .unwrap();
    let offs = note_offs(&Sequence::new(&song, None).unwrap());
    // The next note is on another string: the first rings past its own end.
    assert!(
        offs.iter()
            .any(|&(tick, key)| key == 43 && tick > 2 * u64::from(PPQ))
    );
}

#[test]
fn a_brush_stroke_spreads_the_chord() {
    let mut song = testing::song_with_notes(&[(
        1,
        &[
            TestNote::new(6, 3),
            TestNote::new(5, 2),
            TestNote::new(1, 3),
        ],
    )])
    .unwrap();
    let _ = song
        .set_beat_effect(at(0), BeatEffect::BrushStroke(Some(StrokeDirection::Down)))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    let ticks: Vec<u64> = ons.iter().map(|&(tick, ..)| tick).collect();
    assert_eq!(ticks.iter().filter(|&&tick| tick == 0).count(), 1);
    assert!(ticks.iter().all(|&tick| tick <= u64::from(PPQ) / 8));
}

#[test]
fn a_beat_excerpt_plays_one_beat_from_the_start() {
    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(1), string(5), NoteEffect::Vibrato(Some(Vibrato::Slight)))
        .unwrap();
    let excerpt = Sequence::beat_excerpt(&song, at(1)).unwrap().unwrap();
    let ons = note_ons(&excerpt);
    assert_eq!(ons.len(), 1);
    assert_eq!(ons[0].0, 0);
    assert_eq!(ons[0].1, 48, "string 5, fret 3");
    assert!(excerpt.end >= 2 * u64::from(PPQ) - 1);
    assert!(
        excerpt.events.iter().any(
            |event| matches!(event.event, MidiEvent::PitchBend { value, .. } if value != 8192)
        )
    );
    // A muted track is not heard.
    let mut muted = song.clone();
    let _ = muted.set_track_mute(TrackIndex::new(0), true).unwrap();
    assert!(Sequence::beat_excerpt(&muted, at(1)).unwrap().is_none());
}

#[test]
fn staccato_stops_dead_on_its_own_channel() {
    let mut song = two_beats();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Staccato(true))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let (on_tick, channel) = sequence
        .events
        .iter()
        .find_map(|event| match event.event {
            MidiEvent::NoteOn {
                channel, key: 43, ..
            } => Some((event.tick, channel)),
            _ => None,
        })
        .unwrap();
    assert_eq!(on_tick, 0);
    assert_ne!(channel, base, "a damped note borrows a channel");
    let off = note_offs(&sequence)
        .into_iter()
        .find(|&(_, key)| key == 43)
        .unwrap()
        .0;
    // Half of the half note, at most an eighth: the release is cut there.
    assert_eq!(off, u64::from(PPQ) / 2);
    let cut = sequence.events.iter().any(|event| {
        event.tick == off
            && matches!(
                event.event,
                MidiEvent::Control { channel: c, controller: controller::EXPRESSION, value: 0 }
                    if c == channel
            )
    });
    assert!(cut, "the expression drops to zero at the note's end");
    assert_eq!(sequence.degraded_pitch_bends, 0);
}
