//! Beat effects (pick and brush strokes, tapping, slapping and popping, fade-ins) and the
//! notes' articulations are drawn: on the tab, on the notation staff, and on a notation
//! staff shown without a tab.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{DrawOp, Fonts, Glyph, Layout, LayoutOptions, layout_track};
use gscore_model::testing::{TestNote, song_with_notes};
use gscore_model::{
    AccentKind, BeatEffect, BeatIndex, BeatPosition, Harmonic, HarmonicKind, Legato, MeasureIndex,
    NoteEffect, SlapKind, SlideKind, Song, StringNumber, StrokeDirection, TrackIndex, TrackStaves,
    VoiceIndex,
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

/// Eight eighth notes, each with effects of its own:
/// 0 a down pick stroke on a chord with a down brush and an accent;
/// 1 an up pick stroke, staccato, vibrato; 2 tapping, ghost note; 3 slapping, heavy accent,
/// trill;
/// 4 popping, tremolo picking, natural harmonic; 5 a fade-in, a hammer-on to 6;
/// 6 a shift slide to 7; 7 an up brush, a slide out downwards.
fn effects_song() -> Song {
    let chord = [
        TestNote::new(1, 0),
        TestNote::new(2, 1),
        TestNote::new(3, 2),
    ];
    let single = [TestNote::new(2, 3)];
    let beats: Vec<(u16, &[TestNote<'_>])> = vec![
        (8, &chord),
        (8, &single),
        (8, &single),
        (8, &single),
        (8, &single),
        (8, &single),
        (8, &single),
        (8, &chord),
    ];
    let mut song = song_with_notes(&beats).unwrap();
    let beat = |song: &mut Song, index: usize, effect: BeatEffect| {
        let _change = song.set_beat_effect(at(index), effect).unwrap();
    };
    beat(
        &mut song,
        0,
        BeatEffect::PickStroke(Some(StrokeDirection::Down)),
    );
    beat(
        &mut song,
        0,
        BeatEffect::BrushStroke(Some(StrokeDirection::Down)),
    );
    beat(
        &mut song,
        1,
        BeatEffect::PickStroke(Some(StrokeDirection::Up)),
    );
    beat(&mut song, 2, BeatEffect::Slap(Some(SlapKind::Tapping)));
    beat(&mut song, 3, BeatEffect::Slap(Some(SlapKind::Slapping)));
    beat(&mut song, 4, BeatEffect::Slap(Some(SlapKind::Popping)));
    beat(&mut song, 5, BeatEffect::FadeIn(true));
    beat(
        &mut song,
        7,
        BeatEffect::BrushStroke(Some(StrokeDirection::Up)),
    );
    let note = |song: &mut Song, index: usize, on: u8, effect: NoteEffect| {
        let _change = song.set_note_effect(at(index), string(on), effect).unwrap();
    };
    note(
        &mut song,
        0,
        1,
        NoteEffect::Accent(Some(AccentKind::Normal)),
    );
    note(&mut song, 1, 2, NoteEffect::Staccato(true));
    note(
        &mut song,
        1,
        2,
        NoteEffect::Vibrato(Some(gscore_model::Vibrato::Slight)),
    );
    note(
        &mut song,
        3,
        2,
        NoteEffect::Trill(Some(gscore_model::Trill {
            fret: Some(4),
            speed: gscore_model::TrillSpeed::ThirtySecond,
        })),
    );
    note(&mut song, 2, 2, NoteEffect::Ghost(true));
    note(&mut song, 3, 2, NoteEffect::Accent(Some(AccentKind::Heavy)));
    note(
        &mut song,
        4,
        2,
        NoteEffect::TremoloPicking(Some(gscore_model::TremoloSpeed::Eighth)),
    );
    note(
        &mut song,
        4,
        2,
        NoteEffect::Harmonic(Some(Harmonic {
            kind: HarmonicKind::Natural,
            node: None,
        })),
    );
    note(&mut song, 5, 2, NoteEffect::Hammer(true));
    note(
        &mut song,
        6,
        2,
        NoteEffect::Slide(SlideKind::ShiftSlideTo, true),
    );
    note(
        &mut song,
        7,
        2,
        NoteEffect::Slide(SlideKind::OutDownwards, true),
    );
    song
}

fn layout_with(song: &Song, staves: TrackStaves) -> Layout {
    let mut song = song.clone();
    let _change = song.set_track_staves(TrackIndex::new(0), staves).unwrap();
    let fonts = Fonts::bundled().unwrap();
    let layout =
        layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap();
    // `GSCORE_EFFECTS_SVG=<dir>` writes the pages, to look at them.
    if let Some(dir) = std::env::var_os("GSCORE_EFFECTS_SVG") {
        let svg = gscore_layout::to_svg(
            &layout,
            &gscore_layout::Theme::LIGHT,
            LayoutOptions::default().staff_space_mm,
        );
        let path = std::path::Path::new(&dir).join(format!("effects_{staves:?}.svg"));
        std::fs::write(path, svg).unwrap();
    }
    layout
}

fn glyph_count(layout: &Layout, wanted: Glyph) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Glyph { glyph, .. } if *glyph == wanted))
        .count()
}

fn texts(layout: &Layout) -> Vec<String> {
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Arrowheads: the brush arrows (the only triangles in this song).
fn arrowheads(layout: &Layout) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Polygon { points, .. } if points.len() == 3))
        .count()
}

#[test]
fn beat_effects_are_drawn_on_the_tab() {
    let layout = layout_with(&effects_song(), TrackStaves::TabOnly);
    assert_eq!(glyph_count(&layout, Glyph::DownStroke), 1);
    assert_eq!(glyph_count(&layout, Glyph::UpStroke), 1);
    let texts = texts(&layout);
    for letter in ["T", "S", "P"] {
        assert!(texts.iter().any(|t| t == letter), "{letter} in {texts:?}");
    }
    assert_eq!(arrowheads(&layout), 2, "two brush arrows");
    // Without notation the tab keeps its own accent signs.
    assert!(texts.iter().any(|t| t == ">"));
}

#[test]
fn the_notation_staff_shows_the_articulations() {
    let layout = layout_with(&effects_song(), TrackStaves::NotationAndTab);
    assert_eq!(
        glyph_count(&layout, Glyph::AccentAbove) + glyph_count(&layout, Glyph::AccentBelow),
        1
    );
    assert_eq!(
        glyph_count(&layout, Glyph::MarcatoAbove) + glyph_count(&layout, Glyph::MarcatoBelow),
        1
    );
    assert_eq!(
        glyph_count(&layout, Glyph::StaccatoAbove) + glyph_count(&layout, Glyph::StaccatoBelow),
        1
    );
    assert_eq!(
        glyph_count(&layout, Glyph::ParenthesisLeft),
        1,
        "ghost note"
    );
    assert_eq!(glyph_count(&layout, Glyph::ParenthesisRight), 1);
    assert_eq!(glyph_count(&layout, Glyph::NoteheadHarmonic), 1);
    assert_eq!(glyph_count(&layout, Glyph::Tremolo3), 1, "on the stem only");
    // The accents moved from the tab to the notation staff.
    assert!(!texts(&layout).iter().any(|t| t == ">" || t == "v"));
    // The pick strokes stay in the beat lane above the tab; Guitar Pro draws the brush
    // arrows on the tab only, not on the notation staff above it.
    assert_eq!(glyph_count(&layout, Glyph::DownStroke), 1);
    assert_eq!(arrowheads(&layout), 2, "the brush arrows on the tab");
}

#[test]
fn a_notation_only_staff_keeps_the_beat_effects() {
    let layout = layout_with(&effects_song(), TrackStaves::NotationOnly);
    assert_eq!(glyph_count(&layout, Glyph::DownStroke), 1);
    assert_eq!(glyph_count(&layout, Glyph::UpStroke), 1);
    let texts = texts(&layout);
    for letter in ["T", "S", "P"] {
        assert!(texts.iter().any(|t| t == letter), "{letter} in {texts:?}");
    }
    assert_eq!(
        arrowheads(&layout),
        2,
        "the brush arrows on the notation staff"
    );
    // The pick strokes sit above the notation staff, not where the tab would be.
    let staff_top = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::DownStroke,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .next()
        .unwrap();
    let first_head = layout
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::NoteheadBlack,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .unwrap();
    assert!(staff_top < first_head, "{staff_top} above {first_head}");
}

#[test]
fn articulations_sit_opposite_the_stem() {
    // A low note has its stem up: its accent goes below the head.
    let low = [TestNote::new(6, 3)];
    let high = [TestNote::new(1, 12)];
    let mut song = song_with_notes(&[(4, &low), (4, &high)]).unwrap();
    for (beat, on) in [(0, 6), (1, 1)] {
        let _change = song
            .set_note_effect(
                at(beat),
                string(on),
                NoteEffect::Accent(Some(AccentKind::Normal)),
            )
            .unwrap();
    }
    let layout = layout_with(&song, TrackStaves::NotationAndTab);
    assert_eq!(glyph_count(&layout, Glyph::AccentBelow), 1);
    assert_eq!(glyph_count(&layout, Glyph::AccentAbove), 1);
}

#[test]
fn a_legato_beat_run_gets_a_slur_on_the_notation_staff() {
    let note = [TestNote::new(2, 3)];
    let mut song = song_with_notes(&[(4, &note), (4, &note), (4, &note)]).unwrap();
    let curves = |song: &Song| {
        layout_with(song, TrackStaves::NotationOnly)
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Curve { .. }))
            .count()
    };
    let before = curves(&song);
    for (beat, origin, destination) in [(0, true, false), (1, true, true), (2, false, true)] {
        let legato = Legato {
            origin,
            destination,
        };
        let _change = song
            .set_beat_effect(at(beat), BeatEffect::Legato(Some(legato)))
            .unwrap();
    }
    assert_eq!(curves(&song), before + 1, "one slur over the run");
}
