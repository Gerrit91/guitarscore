//! Tests for the note and beat effect edits: every variant sets, reads back, and undoes.

use crate::effects::{
    AccentKind, BeatEffect, Bend, BendKind, BendPoint, BendShape, Harmonic, HarmonicKind, Legato,
    NoteEffect, SlapKind, SlideKind, StrokeDirection, Vibrato, Whammy, WhammyShape,
};
use crate::interop::from_gp;
use crate::position::{
    BeatIndex, BeatPosition, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
};
use crate::test_support::{beat, one_measure};
use crate::views::{BeatView, NoteView};
use crate::{Song, differences};

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

fn point(position: u8, value: i8) -> BendPoint {
    BendPoint { position, value }
}

/// A one-measure song with one quarter-note beat: fret 7 on string 1.
fn song_with_note() -> Song {
    from_gp(one_measure(vec![beat(4, &[(1, 7)])])).unwrap()
}

fn only_note(song: &Song) -> NoteView<'_> {
    song.beat(at(0)).unwrap().notes().next().unwrap()
}

fn only_beat(song: &Song) -> BeatView<'_> {
    song.beat(at(0)).unwrap()
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(
        differences(before, song).is_empty(),
        "{:?}",
        differences(before, song)
    );
}

/// Applies a note effect, checks it with `check`, and undoes it.
fn round_trip_note(
    effect: NoteEffect,
    check: impl Fn(NoteView<'_>),
    undone: impl Fn(NoteView<'_>),
) {
    let mut song = song_with_note();
    let before = song.clone();
    let change = song.set_note_effect(at(0), string(1), effect).unwrap();
    check(only_note(&song));
    song.revert(&change).unwrap();
    undone(only_note(&song));
    assert_unchanged(&before, &song);
}

/// Applies a beat effect, checks it with `check`, and undoes it.
fn round_trip_beat(
    effect: BeatEffect,
    check: impl Fn(BeatView<'_>),
    undone: impl Fn(BeatView<'_>),
) {
    let mut song = song_with_note();
    let before = song.clone();
    let change = song.set_beat_effect(at(0), effect).unwrap();
    check(only_beat(&song));
    song.revert(&change).unwrap();
    undone(only_beat(&song));
    assert_unchanged(&before, &song);
}

#[test]
fn hammer_on_toggles() {
    round_trip_note(
        NoteEffect::Hammer(true),
        |note| assert!(note.is_hammer()),
        |note| assert!(!note.is_hammer()),
    );
}

#[test]
fn bend_shape_is_written_read_back_and_removed() {
    let shape = BendShape::new(BendKind::Up, vec![point(0, 0), point(6, 4), point(12, 3)]).unwrap();
    let mut song = song_with_note();
    let before = song.clone();

    let change = song
        .set_note_effect(at(0), string(1), NoteEffect::Bend(Some(shape.clone())))
        .unwrap();
    assert_eq!(only_note(&song).bend_shape(), Some(shape.clone()));
    assert_eq!(
        only_note(&song).bend(),
        Some(Bend {
            kind: BendKind::Up,
            quarter_tones: 4,
        })
    );

    let cleared = song
        .set_note_effect(at(0), string(1), NoteEffect::Bend(None))
        .unwrap();
    assert_eq!(only_note(&song).bend_shape(), None);

    song.revert(&cleared).unwrap();
    assert_eq!(only_note(&song).bend_shape(), Some(shape));
    song.revert(&change).unwrap();
    assert_eq!(only_note(&song).bend_shape(), None);
    assert_unchanged(&before, &song);
}

#[test]
fn a_release_bend_reads_back_as_a_release() {
    let shape = BendShape::new(BendKind::Release, vec![point(0, 4), point(12, 0)]).unwrap();
    let mut song = song_with_note();
    let change = song
        .set_note_effect(at(0), string(1), NoteEffect::Bend(Some(shape.clone())))
        .unwrap();
    assert_eq!(only_note(&song).bend_shape(), Some(shape));
    let bend = only_note(&song).bend().unwrap();
    assert_eq!(bend.kind, BendKind::Release);
    assert_eq!(bend.quarter_tones, 4);
    song.revert(&change).unwrap();
}

#[test]
fn the_bend_shape_helper_makes_a_simple_bend() {
    let shape = BendShape::up(4).unwrap();
    assert_eq!(
        shape.points,
        vec![point(0, 0), point(crate::BEND_MAX_POSITION, 4)]
    );
    assert!(BendShape::up(49).is_err());
}

#[test]
fn slides_are_added_and_removed_without_touching_the_others() {
    let mut song = song_with_note();
    let before = song.clone();

    let shift = song
        .set_note_effect(
            at(0),
            string(1),
            NoteEffect::Slide(SlideKind::ShiftSlideTo, true),
        )
        .unwrap();
    assert_eq!(
        only_note(&song).slides().collect::<Vec<_>>(),
        vec![SlideKind::ShiftSlideTo]
    );

    let out = song
        .set_note_effect(
            at(0),
            string(1),
            NoteEffect::Slide(SlideKind::OutDownwards, true),
        )
        .unwrap();
    assert_eq!(
        only_note(&song).slides().collect::<Vec<_>>(),
        vec![SlideKind::ShiftSlideTo, SlideKind::OutDownwards]
    );

    let removed = song
        .set_note_effect(
            at(0),
            string(1),
            NoteEffect::Slide(SlideKind::ShiftSlideTo, false),
        )
        .unwrap();
    assert_eq!(
        only_note(&song).slides().collect::<Vec<_>>(),
        vec![SlideKind::OutDownwards]
    );

    song.revert(&removed).unwrap();
    song.revert(&out).unwrap();
    song.revert(&shift).unwrap();
    assert!(only_note(&song).slides().next().is_none());
    assert_unchanged(&before, &song);
}

#[test]
fn harmonic_node_is_stored_as_an_offset_and_read_back_as_the_node() {
    let harmonic = Harmonic {
        kind: HarmonicKind::Pinch,
        node: Some(19),
    };
    round_trip_note(
        NoteEffect::Harmonic(Some(harmonic)),
        move |note| assert_eq!(note.harmonic(), Some(harmonic)),
        |note| assert_eq!(note.harmonic(), None),
    );
}

#[test]
fn a_natural_harmonic_uses_the_written_fret_as_its_node() {
    let harmonic = Harmonic {
        kind: HarmonicKind::Natural,
        node: Some(7),
    };
    let mut song = song_with_note();
    let change = song
        .set_note_effect(at(0), string(1), NoteEffect::Harmonic(Some(harmonic)))
        .unwrap();
    assert_eq!(only_note(&song).harmonic(), Some(harmonic));
    song.revert(&change).unwrap();
}

#[test]
fn ghost_note_toggles() {
    round_trip_note(
        NoteEffect::Ghost(true),
        |note| assert!(note.is_ghost()),
        |note| assert!(!note.is_ghost()),
    );
}

#[test]
fn accents_set_normal_heavy_and_none() {
    let mut song = song_with_note();
    let before = song.clone();

    let normal = song
        .set_note_effect(
            at(0),
            string(1),
            NoteEffect::Accent(Some(AccentKind::Normal)),
        )
        .unwrap();
    assert_eq!(only_note(&song).accent(), Some(AccentKind::Normal));
    assert!(only_note(&song).is_accented());

    let heavy = song
        .set_note_effect(
            at(0),
            string(1),
            NoteEffect::Accent(Some(AccentKind::Heavy)),
        )
        .unwrap();
    assert_eq!(only_note(&song).accent(), Some(AccentKind::Heavy));

    let cleared = song
        .set_note_effect(at(0), string(1), NoteEffect::Accent(None))
        .unwrap();
    assert_eq!(only_note(&song).accent(), None);
    assert!(!only_note(&song).is_accented());

    song.revert(&cleared).unwrap();
    assert_eq!(only_note(&song).accent(), Some(AccentKind::Heavy));
    song.revert(&heavy).unwrap();
    assert_eq!(only_note(&song).accent(), Some(AccentKind::Normal));
    song.revert(&normal).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn palm_mute_toggles() {
    round_trip_note(
        NoteEffect::PalmMute(true),
        |note| assert!(note.is_palm_muted()),
        |note| assert!(!note.is_palm_muted()),
    );
}

#[test]
fn let_ring_toggles() {
    round_trip_note(
        NoteEffect::LetRing(true),
        |note| assert!(note.is_let_ring()),
        |note| assert!(!note.is_let_ring()),
    );
}

#[test]
fn staccato_toggles() {
    round_trip_note(
        NoteEffect::Staccato(true),
        |note| assert!(note.is_staccato()),
        |note| assert!(!note.is_staccato()),
    );
}

#[test]
fn trill_toggles() {
    round_trip_note(
        NoteEffect::Trill(true),
        |note| assert!(note.has_trill()),
        |note| assert!(!note.has_trill()),
    );
}

#[test]
fn tremolo_picking_toggles() {
    round_trip_note(
        NoteEffect::TremoloPicking(true),
        |note| assert!(note.has_tremolo_picking()),
        |note| assert!(!note.has_tremolo_picking()),
    );
}

#[test]
fn vibrato_set_slight_wide_and_none() {
    let mut song = song_with_note();
    let before = song.clone();

    let slight = song
        .set_note_effect(at(0), string(1), NoteEffect::Vibrato(Some(Vibrato::Slight)))
        .unwrap();
    assert_eq!(only_note(&song).vibrato(), Some(Vibrato::Slight));
    assert!(only_note(&song).has_vibrato());

    let wide = song
        .set_note_effect(at(0), string(1), NoteEffect::Vibrato(Some(Vibrato::Wide)))
        .unwrap();
    assert_eq!(only_note(&song).vibrato(), Some(Vibrato::Wide));

    let cleared = song
        .set_note_effect(at(0), string(1), NoteEffect::Vibrato(None))
        .unwrap();
    assert_eq!(only_note(&song).vibrato(), None);

    song.revert(&cleared).unwrap();
    assert_eq!(only_note(&song).vibrato(), Some(Vibrato::Wide));
    song.revert(&wide).unwrap();
    assert_eq!(only_note(&song).vibrato(), Some(Vibrato::Slight));
    song.revert(&slight).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn pick_stroke_sets_and_clears() {
    round_trip_beat(
        BeatEffect::PickStroke(Some(StrokeDirection::Down)),
        |beat| assert_eq!(beat.pick_stroke(), Some(StrokeDirection::Down)),
        |beat| assert_eq!(beat.pick_stroke(), None),
    );
}

#[test]
fn brush_stroke_sets_and_clears() {
    round_trip_beat(
        BeatEffect::BrushStroke(Some(StrokeDirection::Up)),
        |beat| assert_eq!(beat.brush_stroke(), Some(StrokeDirection::Up)),
        |beat| assert_eq!(beat.brush_stroke(), None),
    );
}

#[test]
fn slap_kinds_set_and_clear() {
    let mut song = song_with_note();
    let before = song.clone();

    let tapping = song
        .set_beat_effect(at(0), BeatEffect::Slap(Some(SlapKind::Tapping)))
        .unwrap();
    assert_eq!(only_beat(&song).slap(), Some(SlapKind::Tapping));

    let popping = song
        .set_beat_effect(at(0), BeatEffect::Slap(Some(SlapKind::Popping)))
        .unwrap();
    assert_eq!(only_beat(&song).slap(), Some(SlapKind::Popping));

    let cleared = song.set_beat_effect(at(0), BeatEffect::Slap(None)).unwrap();
    assert_eq!(only_beat(&song).slap(), None);

    song.revert(&cleared).unwrap();
    assert_eq!(only_beat(&song).slap(), Some(SlapKind::Popping));
    song.revert(&popping).unwrap();
    assert_eq!(only_beat(&song).slap(), Some(SlapKind::Tapping));
    song.revert(&tapping).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn fade_in_toggles() {
    round_trip_beat(
        BeatEffect::FadeIn(true),
        |beat| assert!(beat.has_fade_in()),
        |beat| assert!(!beat.has_fade_in()),
    );
}

#[test]
fn whammy_shape_is_written_read_back_and_removed() {
    let shape = WhammyShape::new(vec![point(0, 0), point(6, -4), point(12, 0)]).unwrap();
    let mut song = song_with_note();
    let before = song.clone();

    let change = song
        .set_beat_effect(at(0), BeatEffect::Whammy(Some(shape.clone())))
        .unwrap();
    assert_eq!(only_beat(&song).whammy_shape(), Some(shape.clone()));
    assert_eq!(
        only_beat(&song).whammy_bar(),
        Some(Whammy { quarter_tones: -4 })
    );
    assert!(only_beat(&song).has_whammy_bar());

    let cleared = song
        .set_beat_effect(at(0), BeatEffect::Whammy(None))
        .unwrap();
    assert_eq!(only_beat(&song).whammy_shape(), None);

    song.revert(&cleared).unwrap();
    assert_eq!(only_beat(&song).whammy_shape(), Some(shape));
    song.revert(&change).unwrap();
    assert_eq!(only_beat(&song).whammy_shape(), None);
    assert_unchanged(&before, &song);
}

#[test]
fn legato_sets_and_clears() {
    let legato = Legato {
        origin: true,
        destination: false,
    };
    round_trip_beat(
        BeatEffect::Legato(Some(legato)),
        move |beat| assert_eq!(beat.legato(), Some(legato)),
        |beat| assert_eq!(beat.legato(), None),
    );
}

#[test]
fn beat_vibrato_set_slight_wide_and_none() {
    let mut song = song_with_note();
    let before = song.clone();

    let slight = song
        .set_beat_effect(at(0), BeatEffect::Vibrato(Some(Vibrato::Slight)))
        .unwrap();
    assert_eq!(only_beat(&song).vibrato(), Some(Vibrato::Slight));

    let wide = song
        .set_beat_effect(at(0), BeatEffect::Vibrato(Some(Vibrato::Wide)))
        .unwrap();
    assert_eq!(only_beat(&song).vibrato(), Some(Vibrato::Wide));

    let cleared = song
        .set_beat_effect(at(0), BeatEffect::Vibrato(None))
        .unwrap();
    assert_eq!(only_beat(&song).vibrato(), None);

    song.revert(&cleared).unwrap();
    assert_eq!(only_beat(&song).vibrato(), Some(Vibrato::Wide));
    song.revert(&wide).unwrap();
    assert_eq!(only_beat(&song).vibrato(), Some(Vibrato::Slight));
    song.revert(&slight).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn invalid_shapes_are_refused_and_change_nothing() {
    let mut song = song_with_note();
    let before = song.clone();

    let invalid_bends = [
        vec![point(13, 0)],
        vec![point(0, 0), point(0, 2)],
        vec![point(6, 0), point(3, 2)],
        vec![point(0, 49)],
        vec![point(0, -49)],
        Vec::new(),
        (0_u8..=16).map(|position| point(position, 0)).collect(),
    ];
    for points in invalid_bends {
        assert!(
            BendShape::new(BendKind::Up, points.clone()).is_err(),
            "{points:?} must be an invalid shape"
        );
        let effect = NoteEffect::Bend(Some(BendShape {
            kind: BendKind::Up,
            points,
        }));
        assert!(
            song.set_note_effect(at(0), string(1), effect).is_err(),
            "an invalid bend must be refused"
        );
        assert_unchanged(&before, &song);
    }

    let invalid_whammy = WhammyShape {
        points: vec![point(13, 0)],
    };
    assert!(WhammyShape::new(invalid_whammy.points.clone()).is_err());
    assert!(
        song.set_beat_effect(at(0), BeatEffect::Whammy(Some(invalid_whammy)))
            .is_err()
    );
    assert_unchanged(&before, &song);

    // A harmonic node too far from the fretted note does not fit the stored offset.
    let harmonic = Harmonic {
        kind: HarmonicKind::Pinch,
        node: Some(200),
    };
    assert!(
        song.set_note_effect(at(0), string(1), NoteEffect::Harmonic(Some(harmonic)))
            .is_err()
    );
    assert_unchanged(&before, &song);
}

#[test]
fn a_missing_note_string_or_beat_is_refused_and_changes_nothing() {
    let mut song = song_with_note();
    let before = song.clone();

    assert!(
        song.set_note_effect(at(0), string(9), NoteEffect::Hammer(true))
            .is_err(),
        "the guitar has 6 strings"
    );
    assert!(
        song.set_note_effect(at(0), string(4), NoteEffect::Hammer(true))
            .is_err(),
        "there is no note on string 4"
    );
    assert!(
        song.set_note_effect(at(9), string(1), NoteEffect::Hammer(true))
            .is_err(),
        "there is no beat 9"
    );
    assert!(
        song.set_note_effect(
            BeatPosition {
                track: TrackIndex::new(9),
                ..at(0)
            },
            string(1),
            NoteEffect::Hammer(true),
        )
        .is_err()
    );
    assert!(
        song.set_beat_effect(at(9), BeatEffect::FadeIn(true))
            .is_err()
    );
    assert_unchanged(&before, &song);
}

#[test]
fn toggling_one_note_effect_leaves_the_others_intact() {
    let mut song = song_with_note();
    let before = song.clone();

    let palm = song
        .set_note_effect(at(0), string(1), NoteEffect::PalmMute(true))
        .unwrap();
    let hammer = song
        .set_note_effect(at(0), string(1), NoteEffect::Hammer(true))
        .unwrap();
    assert!(only_note(&song).is_palm_muted());
    assert!(only_note(&song).is_hammer());

    song.revert(&hammer).unwrap();
    assert!(only_note(&song).is_palm_muted());
    assert!(!only_note(&song).is_hammer());

    song.revert(&palm).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn toggling_one_beat_effect_leaves_the_others_intact() {
    let mut song = song_with_note();
    let before = song.clone();

    let fade = song
        .set_beat_effect(at(0), BeatEffect::FadeIn(true))
        .unwrap();
    let stroke = song
        .set_beat_effect(at(0), BeatEffect::PickStroke(Some(StrokeDirection::Up)))
        .unwrap();
    assert!(only_beat(&song).has_fade_in());
    assert_eq!(only_beat(&song).pick_stroke(), Some(StrokeDirection::Up));

    song.revert(&stroke).unwrap();
    assert!(only_beat(&song).has_fade_in());
    assert_eq!(only_beat(&song).pick_stroke(), None);

    song.revert(&fade).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn a_grace_note_is_set_changed_and_removed() {
    use crate::{Fret, Grace, GraceSpeed};
    let slow = Grace {
        fret: Fret::new(3).unwrap(),
        speed: GraceSpeed::Slow,
        on_beat: true,
        dead: false,
    };
    round_trip_note(
        NoteEffect::Grace(Some(slow)),
        move |note| assert_eq!(note.grace(), Some(slow)),
        |note| assert_eq!(note.grace(), None),
    );
    // Changing the speed keeps the stored loudness of the grace note.
    let mut song = song_with_note();
    let _ = song
        .set_note_effect(at(0), string(1), NoteEffect::Grace(Some(slow)))
        .unwrap();
    let fast = Grace {
        speed: GraceSpeed::Fast,
        ..slow
    };
    let _ = song
        .set_note_effect(at(0), string(1), NoteEffect::Grace(Some(fast)))
        .unwrap();
    assert_eq!(only_note(&song).grace(), Some(fast));
    let _ = song
        .set_note_effect(at(0), string(1), NoteEffect::Grace(None))
        .unwrap();
    assert_eq!(only_note(&song).grace(), None);
}
