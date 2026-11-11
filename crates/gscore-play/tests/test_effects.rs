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
    AccentKind, BeatEffect, BeatIndex, BeatPosition, BendKind, BendPoint, BendShape, Fret, Grace,
    GraceSpeed, MeasureIndex, NoteEffect, SlideKind, Song, StringNumber, Stroke, StrokeDirection,
    TrackIndex, TremoloSpeed, Trill, TrillSpeed, Vibrato, VoiceIndex, WhammyShape,
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
        .set_beat_effect(
            at(0),
            BeatEffect::BrushStroke(Some(Stroke::brush(StrokeDirection::Down))),
        )
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

#[test]
fn a_drum_note_never_leaves_the_kits_channel() {
    // A staccato hit on a drum track (a short crash, as in a band's song) used to borrow a
    // melodic channel like a damped guitar note: there it was no drum, the mixer never
    // heard it, and the next borrower's pan landed on the drum strip.
    let mut song = two_beats();
    let _ = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Staccato(true))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let kit = sequence.tracks[0].channel;
    let channels: Vec<_> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOn { channel, .. } => Some(channel),
            _ => None,
        })
        .collect();
    assert!(!channels.is_empty());
    assert!(
        channels.iter().all(|&channel| channel == kit),
        "{channels:?}"
    );
}

/// Pitch bends on channels other than the track's, in order.
fn borrowed_bends(sequence: &Sequence) -> Vec<(u64, u16)> {
    let base = sequence.tracks[0].channel;
    sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::PitchBend { channel, value } if channel != base => Some((event.tick, value)),
            _ => None,
        })
        .collect()
}

fn points(points: &[(u8, i8)]) -> Vec<BendPoint> {
    points
        .iter()
        .map(|&(position, value)| BendPoint { position, value })
        .collect()
}

#[test]
fn a_drawn_bend_is_followed_point_by_point() {
    // Bend, release, bend again over a half note.
    let mut song = two_beats();
    let shape = BendShape::new(BendKind::Up, points(&[(0, 0), (4, 4), (8, 0), (12, 4)])).unwrap();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Bend(Some(shape)))
        .unwrap();
    let bends = borrowed_bends(&Sequence::new(&song, None).unwrap());
    let half = 2 * u64::from(PPQ);
    let value_near = |share: f64| {
        let tick = (share * half as f64) as u64;
        bends
            .iter()
            .rfind(|(at, _)| *at <= tick)
            .map(|&(_, value)| value)
            .unwrap()
    };
    let whole_step = 8192 + 2730;
    assert!(
        value_near(1.0 / 3.0).abs_diff(whole_step) < 250,
        "up at a third"
    );
    assert!(
        value_near(2.0 / 3.0).abs_diff(8192) < 250,
        "back down at two thirds"
    );
    assert!(
        value_near(0.99).abs_diff(whole_step) < 250,
        "up again at the end"
    );
}

#[test]
fn a_whammy_dive_bends_every_note_of_the_beat() {
    let mut song =
        testing::song_with_notes(&[(2, &[TestNote::new(6, 3), TestNote::new(4, 2)])]).unwrap();
    let dive = WhammyShape::new(points(&[(0, 0), (6, -8), (12, -8)])).unwrap();
    let _ = song
        .set_beat_effect(at(0), BeatEffect::Whammy(Some(dive)))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let dived: std::collections::BTreeSet<_> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            // Two whole steps down, with the six-semitone range: well below center.
            MidiEvent::PitchBend { channel, value } if channel != base && value < 8192 - 5000 => {
                Some(channel)
            }
            _ => None,
        })
        .collect();
    assert_eq!(dived.len(), 2, "both notes dive");
    assert_eq!(sequence.degraded_pitch_bends, 0);
}

#[test]
fn an_excerpt_of_a_tied_beat_strikes_its_notes() {
    // A half note tied into the next: the second beat alone still sounds.
    let mut song = two_beats();
    let _ = song
        .set_note(at(1), string(6), Fret::new(3).unwrap())
        .unwrap();
    let _ = song.set_note_tie(at(1), string(6), true).unwrap();
    let full = note_ons(&Sequence::new(&song, None).unwrap());
    assert_eq!(
        full.iter().filter(|&&(_, key, _)| key == 43).count(),
        1,
        "in the song the tie continues the note"
    );
    let excerpt = Sequence::beat_excerpt(&song, at(1)).unwrap().unwrap();
    assert!(
        note_ons(&excerpt)
            .iter()
            .any(|&(tick, key, _)| tick == 0 && key == 43)
    );
}

#[test]
fn a_bent_note_keeps_its_pitch_through_its_release() {
    // A bend up a whole step, held: nothing may pull its release back down.
    let mut song = two_beats();
    let shape = BendShape::new(BendKind::Up, points(&[(0, 0), (6, 4), (12, 4)])).unwrap();
    let _ = song
        .set_note_effect(at(0), string(6), NoteEffect::Bend(Some(shape)))
        .unwrap();
    let excerpt = Sequence::beat_excerpt(&song, at(0)).unwrap().unwrap();
    let off = note_offs(&excerpt)[0].0;
    let bent = borrowed_bends(&excerpt);
    assert!(
        bent.iter().all(|&(tick, _)| tick < off),
        "no bend after the note-off"
    );
    assert!(bent.last().unwrap().1 > 8192 + 2500, "it ends bent");
    // The excerpt runs on for the release, then fades its bent channel and silences it.
    assert!(excerpt.end > off);
    let channel = sequence_channels_bent(&excerpt);
    let fade = excerpt.events.iter().any(|event| {
        matches!(event.event, MidiEvent::Control { channel: c, controller: controller::EXPRESSION, value: 0 } if c == channel)
            && event.tick < excerpt.end
    });
    assert!(fade);
}

/// The channel the excerpt bends on.
fn sequence_channels_bent(sequence: &Sequence) -> gscore_play::Channel {
    let base = sequence.tracks[0].channel;
    sequence
        .events
        .iter()
        .find_map(|event| match event.event {
            MidiEvent::PitchBend { channel, .. } if channel != base => Some(channel),
            _ => None,
        })
        .unwrap()
}

/// A chord of three strings as one whole note.
fn chord_song() -> Song {
    testing::song_with_notes(&[(
        1,
        &[
            TestNote::new(6, 3),
            TestNote::new(5, 2),
            TestNote::new(1, 3),
        ],
    )])
    .unwrap()
}

#[test]
fn a_pick_stroke_strikes_the_chord_quickly_in_its_direction() {
    let plain = note_ons(&Sequence::new(&chord_song(), None).unwrap());
    let stroke = |direction| {
        let mut song = chord_song();
        let _ = song
            .set_beat_effect(at(0), BeatEffect::PickStroke(Some(direction)))
            .unwrap();
        note_ons(&Sequence::new(&song, None).unwrap())
    };
    let first_key = |ons: &[(u64, u8, u8)]| ons.iter().min_by_key(|on| on.0).unwrap().1;
    let down = stroke(StrokeDirection::Down);
    let up = stroke(StrokeDirection::Up);
    // A downstroke starts on the low E string (G2, key 43), an upstroke on the high one.
    assert_eq!(first_key(&down), 43);
    assert_eq!(first_key(&up), 67);
    for ons in [&down, &up] {
        let last = ons.iter().map(|on| on.0).max().unwrap();
        assert!(last > 0 && last <= u64::from(PPQ) / 32, "{ons:?}");
    }
    assert!(
        up.iter().all(|on| on.2 < plain[0].2),
        "an upstroke is softer"
    );
    assert!(down.iter().all(|on| on.2 == plain[0].2));
}

#[test]
fn tapping_is_softer_and_slapping_and_popping_louder() {
    let plain = note_ons(&Sequence::new(&two_beats(), None).unwrap())[0].2;
    let velocity = |kind| {
        let mut song = two_beats();
        let _ = song
            .set_beat_effect(at(0), BeatEffect::Slap(Some(kind)))
            .unwrap();
        note_ons(&Sequence::new(&song, None).unwrap())[0].2
    };
    let tap = velocity(gscore_model::SlapKind::Tapping);
    let slap = velocity(gscore_model::SlapKind::Slapping);
    let pop = velocity(gscore_model::SlapKind::Popping);
    assert!(
        tap < plain && plain < slap && slap < pop,
        "{tap} {plain} {slap} {pop}"
    );
}

#[test]
fn a_fade_in_swells_on_its_own_channel() {
    let mut song = two_beats();
    let _ = song
        .set_beat_effect(at(0), BeatEffect::FadeIn(true))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let faded = sequence
        .events
        .iter()
        .find_map(|event| match event.event {
            MidiEvent::NoteOn {
                channel, key: 43, ..
            } => Some(channel),
            _ => None,
        })
        .unwrap();
    assert_ne!(faded, base, "the fade-in plays on a borrowed channel");
    let expression: Vec<(u64, u8)> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::Control {
                channel,
                controller: controller::EXPRESSION,
                value,
            } if channel == faded => Some((event.tick, value)),
            _ => None,
        })
        .collect();
    // The last value at the note's start is silence, then it rises to full over the half
    // note.
    let at_start = expression
        .iter()
        .rev()
        .find(|(tick, _)| *tick == 0)
        .unwrap();
    assert_eq!(at_start.1, 0, "{expression:?}");
    let half = 2 * u64::from(PPQ);
    assert!(
        expression
            .iter()
            .any(|&(tick, value)| tick == half && value == 127)
    );
    // The next note, without a fade-in, is at full volume on the track's channel.
    let next = note_ons(&sequence)[1];
    assert_eq!(next.0, half);
}

#[test]
fn an_arpeggio_rolls_the_chord_over_an_eighth() {
    let mut song = chord_song();
    let _ = song
        .set_beat_effect(
            at(0),
            BeatEffect::Arpeggio(Some(Stroke::arpeggio(StrokeDirection::Down))),
        )
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    let last = ons.iter().map(|&(tick, ..)| tick).max().unwrap();
    assert!(last > 0 && last <= u64::from(PPQ) / 2, "{ons:?}");
    let first = ons.iter().min_by_key(|on| on.0).unwrap();
    assert_eq!(first.1, 43, "a roll down starts on the low E string");
}

#[test]
fn a_stroke_uses_its_own_speed() {
    let mut song = chord_song();
    let stroke = Stroke {
        direction: StrokeDirection::Down,
        speed: 16,
        on_beat: true,
    };
    let _ = song
        .set_beat_effect(at(0), BeatEffect::BrushStroke(Some(stroke)))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    let last = ons.iter().map(|&(tick, ..)| tick).max().unwrap();
    assert_eq!(last, u64::from(PPQ) / 4, "a 16th-note spread: {ons:?}");
}

#[test]
fn a_stroke_can_end_on_the_beat() {
    // A short chord on the second half note, so there is room before it; the roll is
    // longer than the chord's notes, which must not hold its last string back.
    let mut song = testing::song_with_notes(&[
        (2, &[TestNote::new(1, 0)]),
        (
            16,
            &[
                TestNote::new(6, 3),
                TestNote::new(5, 2),
                TestNote::new(1, 3),
            ],
        ),
    ])
    .unwrap();
    let stroke = Stroke {
        direction: StrokeDirection::Down,
        speed: 8,
        on_beat: false,
    };
    let _ = song
        .set_beat_effect(at(1), BeatEffect::Arpeggio(Some(stroke)))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    let beat = 2 * u64::from(PPQ);
    let chord: Vec<u64> = ons
        .iter()
        .map(|&(tick, ..)| tick)
        .filter(|&tick| tick > 0)
        .collect();
    assert_eq!(chord.len(), 3, "{ons:?}");
    let last = chord.iter().copied().max().unwrap();
    assert_eq!(last, beat, "the last string lands on the beat: {ons:?}");
    let first = chord.iter().copied().min().unwrap();
    assert_eq!(
        first,
        beat - u64::from(PPQ) / 2,
        "the first string falls an eighth before it: {ons:?}"
    );
}

#[test]
fn a_stroke_ending_on_the_first_beat_of_the_song_has_no_room_and_lands_together() {
    let mut song = chord_song();
    let stroke = Stroke {
        direction: StrokeDirection::Up,
        speed: 8,
        on_beat: false,
    };
    let _ = song
        .set_beat_effect(at(0), BeatEffect::Arpeggio(Some(stroke)))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    assert_eq!(ons.len(), 3, "{ons:?}");
    assert!(ons.iter().all(|&(tick, ..)| tick == 0), "{ons:?}");
}

#[test]
fn a_rasgueado_strikes_the_chord_three_times_softer_each_time() {
    let mut song = chord_song();
    let _ = song
        .set_beat_effect(at(0), BeatEffect::Rasgueado(true))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    assert_eq!(ons.len(), 9, "three strings struck three times");
    let mut ticks: Vec<u64> = ons.iter().map(|&(tick, ..)| tick).collect();
    ticks.sort_unstable();
    ticks.dedup();
    assert_eq!(ticks.len(), 3, "{ticks:?}");
    assert!(ticks[2] <= u64::from(PPQ) / 8, "{ticks:?}");
    let soft = |tick: u64| ons.iter().find(|on| on.0 == tick).unwrap().2;
    assert!(soft(ticks[2]) < soft(ticks[1]) && soft(ticks[1]) < soft(ticks[0]));
    // Each strike stops before the next one on its string, and none rings past the beat.
    let sequence = Sequence::new(&song, None).unwrap();
    let offs = note_offs(&sequence);
    for &(tick, key, _) in &ons {
        let off = offs
            .iter()
            .filter(|&&(off, k)| k == key && off > tick)
            .map(|&(off, _)| off)
            .min()
            .unwrap();
        let next_on = ons
            .iter()
            .filter(|&&(on, k, _)| k == key && on > tick)
            .map(|&(on, ..)| on)
            .min();
        if let Some(next_on) = next_on {
            assert!(
                off <= next_on,
                "{key}: off {off} after the next strike {next_on}"
            );
        }
        assert!(off <= 4 * u64::from(PPQ), "{key} rings past its whole note");
    }
}

#[test]
fn a_dead_slapped_beat_hits_every_string_shortly_and_softly() {
    let mut song = testing::song_with_notes(&[(1, &[TestNote::new(6, 3)])]).unwrap();
    let _ = song
        .set_beat_effect(at(0), BeatEffect::DeadSlapped(true))
        .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let ons = note_ons(&sequence);
    assert_eq!(ons.len(), 6, "one muted hit per string");
    let offs = note_offs(&sequence);
    for (tick, key, velocity) in &ons {
        assert_eq!(*tick, 0);
        assert_eq!(*velocity, 70);
        let off = offs.iter().find(|&&(_, k)| k == *key).unwrap().0;
        assert!(off <= u64::from(PPQ) / 4, "{key} lasts too long");
    }
}

#[test]
fn a_pick_slide_scrapes_to_the_next_note() {
    let mut song =
        testing::song_with_notes(&[(4, &[TestNote::new(3, 12)]), (4, &[TestNote::new(3, 7)])])
            .unwrap();
    let _ = song
        .set_note_effect(
            at(0),
            string(3),
            NoteEffect::Slide(SlideKind::PickSlideDown, true),
        )
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    // The slide replaces its note with six scraping notes, then the next note plays.
    assert_eq!(ons.len(), 7, "{ons:?}");
    let scrape: Vec<u8> = ons
        .iter()
        .filter(|&&(tick, ..)| tick < u64::from(PPQ) / 2)
        .map(|&(_, key, _)| key)
        .collect();
    assert_eq!(scrape.len(), 6);
    assert!(
        scrape.windows(2).all(|pair| pair[0] >= pair[1]),
        "it scrapes downwards: {scrape:?}"
    );
    assert!(scrape.first() > scrape.last());
}

#[test]
fn a_trill_alternates_with_its_fret_at_its_speed() {
    let mut song = testing::song_with_notes(&[(2, &[TestNote::new(3, 2)])]).unwrap();
    let _ = song
        .set_note_effect(
            at(0),
            string(3),
            NoteEffect::Trill(Some(Trill {
                fret: Some(4),
                speed: TrillSpeed::ThirtySecond,
            })),
        )
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    // A half note at 32nds is sixteen notes, alternating between the note and its trill.
    assert_eq!(ons.len(), 16, "{ons:?}");
    let keys: Vec<u8> = ons.iter().map(|&(_, key, _)| key).collect();
    assert!(keys[0] != keys[1]);
    for pair in keys.windows(2) {
        assert_ne!(pair[0], pair[1], "the trill alternates");
    }
}

#[test]
fn tremolo_picking_repeats_at_its_speed() {
    let repeats = |speed| {
        let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 2)])]).unwrap();
        let _ = song
            .set_note_effect(at(0), string(3), NoteEffect::TremoloPicking(Some(speed)))
            .unwrap();
        note_ons(&Sequence::new(&song, None).unwrap()).len()
    };
    assert_eq!(repeats(TremoloSpeed::Eighth), 2);
    assert_eq!(repeats(TremoloSpeed::Sixteenth), 4);
    assert_eq!(repeats(TremoloSpeed::ThirtySecond), 8);
}

#[test]
fn a_fade_out_and_a_swell_ramp_the_expression() {
    let expression = |effect| {
        let mut song = two_beats();
        let _ = song.set_beat_effect(at(0), effect).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let base = sequence.tracks[0].channel;
        let channel = sequence
            .events
            .iter()
            .find_map(|event| match event.event {
                MidiEvent::NoteOn {
                    channel, key: 43, ..
                } if channel != base => Some(channel),
                _ => None,
            })
            .unwrap();
        let values: Vec<(u64, u8)> = sequence
            .events
            .iter()
            .filter_map(|event| match event.event {
                MidiEvent::Control {
                    channel: c,
                    controller: controller::EXPRESSION,
                    value,
                } if c == channel => Some((event.tick, value)),
                _ => None,
            })
            .collect();
        values
    };
    // The channel setup also writes an expression at tick 0; the fade's own value is the
    // last one there.
    let at_start = |values: &[(u64, u8)]| {
        values
            .iter()
            .rev()
            .find(|&&(tick, _)| tick == 0)
            .map(|&(_, value)| value)
    };
    let out = expression(BeatEffect::FadeOut(true));
    assert_eq!(at_start(&out), Some(127));
    assert_eq!(out.last().map(|&(_, value)| value), Some(0));
    let swell = expression(BeatEffect::VolumeSwell(true));
    assert_eq!(at_start(&swell), Some(0));
    assert_eq!(swell.last().map(|&(_, value)| value), Some(0));
    assert!(swell.iter().any(|&(_, value)| value == 127), "{swell:?}");
}

#[test]
fn left_hand_taps_and_grace_hammers_are_softer() {
    let plain = note_ons(&Sequence::new(&two_beats(), None).unwrap())[0].2;
    let mut tapped = two_beats();
    let _ = tapped
        .set_note_effect(at(0), string(6), NoteEffect::LeftHandTap(true))
        .unwrap();
    assert!(note_ons(&Sequence::new(&tapped, None).unwrap())[0].2 < plain);

    let mut hammered = two_beats();
    let grace = Grace {
        fret: Fret::new(1).unwrap(),
        speed: GraceSpeed::Normal,
        on_beat: false,
        dead: false,
        hammer: true,
    };
    let _ = hammered
        .set_note_effect(at(0), string(6), NoteEffect::Grace(Some(grace)))
        .unwrap();
    let ons = note_ons(&Sequence::new(&hammered, None).unwrap());
    let main = ons.iter().find(|&&(_, key, _)| key == 43).unwrap();
    assert!(main.2 < plain, "the hammered main note is softer: {ons:?}");
}

#[test]
fn a_hammered_note_and_a_legato_beat_are_not_picked() {
    let same_string = || {
        testing::song_with_notes(&[
            (4, &[TestNote::new(3, 2)]),
            (4, &[TestNote::new(3, 4)]),
            (4, &[TestNote::new(3, 5)]),
        ])
        .unwrap()
    };
    let plain = note_ons(&Sequence::new(&same_string(), None).unwrap());
    let mut song = same_string();
    let _ = song
        .set_note_effect(at(0), string(3), NoteEffect::Hammer(true))
        .unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    assert_eq!(ons[0].2, plain[0].2, "the origin is picked");
    assert!(ons[1].2 < plain[1].2, "the hammered note is softer");
    assert_eq!(ons[2].2, plain[2].2, "the one after is picked again");

    let mut song = same_string();
    let legato = |origin, destination| {
        BeatEffect::Legato(Some(gscore_model::Legato {
            origin,
            destination,
        }))
    };
    let _ = song.set_beat_effect(at(0), legato(true, false)).unwrap();
    let _ = song.set_beat_effect(at(1), legato(false, true)).unwrap();
    let ons = note_ons(&Sequence::new(&song, None).unwrap());
    assert_eq!(ons[0].2, plain[0].2);
    assert!(ons[1].2 < plain[1].2);
}

#[test]
fn a_beat_excerpt_keeps_brush_arpeggio_and_rasgueado_spreads() {
    for effect in [
        BeatEffect::BrushStroke(Some(Stroke::brush(StrokeDirection::Down))),
        BeatEffect::Arpeggio(Some(Stroke::arpeggio(StrokeDirection::Down))),
        BeatEffect::Rasgueado(true),
    ] {
        let mut song = chord_song();
        let _ = song.set_beat_effect(at(0), effect.clone()).unwrap();
        let excerpt = Sequence::beat_excerpt(&song, at(0)).unwrap().unwrap();
        let ons = note_ons(&excerpt);
        assert!(
            ons.len() >= 3,
            "{effect:?}: the excerpt keeps the chord: {ons:?}"
        );
        let last = ons.iter().map(|on| on.0).max().unwrap();
        assert!(last > 0, "{effect:?}: the chord is spread: {ons:?}");
    }
}
