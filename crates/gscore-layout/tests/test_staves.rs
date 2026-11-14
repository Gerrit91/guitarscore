//! A track's staves switch: notation and tab, tab only, or notation only.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{ColorToken, DrawOp, Fonts, Glyph, Layout, LayoutOptions, Point, layout_track};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{Fret, MeasureIndex, Song, StringNumber, TrackIndex, TrackStaves};

fn song_with(staves: TrackStaves) -> Song {
    let mut song = testing::song(&[GUITAR_6], 2).unwrap();
    let at = gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    };
    let _note = song
        .set_note(at, StringNumber::new(2).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _staves = song.set_track_staves(TrackIndex::new(0), staves).unwrap();
    song
}

fn layout(staves: TrackStaves) -> Layout {
    let song = song_with(staves);
    let fonts = Fonts::bundled().unwrap();
    layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap()
}

/// Fret numbers on the page.
fn fret_numbers(layout: &Layout) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Text {
                    color: ColorToken::FretNumber,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn each_choice_shows_its_staves() {
    let both = layout(TrackStaves::NOTATION_AND_TAB);
    let tab = layout(TrackStaves::TAB_ONLY);
    let notation = layout(TrackStaves::NOTATION_ONLY);

    assert!(fret_numbers(&both) > 0);
    assert_eq!(fret_numbers(&tab), fret_numbers(&both));
    assert_eq!(fret_numbers(&notation), 0, "no tablature at all");
    // Each single staff takes less room than both.
    assert!(tab.height < both.height);
    assert!(notation.height < both.height);

    // With notation only, the cursor rows lie on the notation staff.
    let system = &notation.places.systems[0];
    assert!((system.tab_top - system.top).abs() < 1e-4);
    assert_eq!(system.strings, 6);
    let both_system = &both.places.systems[0];
    assert!(system.tab_bottom() < both_system.tab_top);
}

/// A tie within a measure draws its curve on the slash staff too.
#[test]
fn slash_notation_shows_a_tie_within_the_measure() {
    let mut song = testing::song(&[GUITAR_6], 1).unwrap();
    let at = |beat| gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(beat),
    };
    let _ = song
        .set_note(at(0), StringNumber::new(1).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(1), StringNumber::new(1).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note_tie(at(1), StringNumber::new(1).unwrap(), true)
        .unwrap();
    let _ = song.set_track_staves(TrackIndex::new(0), TrackStaves::SLASH_ONLY);
    let laid = slash_layout(&song);
    let curves = laid
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Curve { .. }))
        .count();
    assert!(curves >= 1, "the tied slashes are joined by a curve");
}

/// A tie across a barline reaches the next measure's first slash.
#[test]
fn slash_notation_shows_a_tie_across_a_barline() {
    let mut song = testing::song(&[GUITAR_6], 2).unwrap();
    let at = |measure, beat| gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(beat),
    };
    let _ = song
        .set_note(
            at(0, 3),
            StringNumber::new(1).unwrap(),
            Fret::new(5).unwrap(),
        )
        .unwrap();
    let _ = song
        .set_note(
            at(1, 0),
            StringNumber::new(1).unwrap(),
            Fret::new(5).unwrap(),
        )
        .unwrap();
    let _ = song
        .set_note_tie(at(1, 0), StringNumber::new(1).unwrap(), true)
        .unwrap();
    let _ = song.set_track_staves(TrackIndex::new(0), TrackStaves::SLASH_ONLY);
    let laid = slash_layout(&song);
    let boundary = laid.places.measures[1].left;
    let crossing = laid
        .ops
        .iter()
        .filter(|op| match op {
            DrawOp::Curve { from, to, .. } => from.x < boundary && to.x > boundary,
            _ => false,
        })
        .count();
    assert!(
        crossing >= 1,
        "the tie crosses the barline on the slash staff"
    );
}

fn slash_layout(song: &Song) -> Layout {
    let fonts = Fonts::bundled().unwrap();
    layout_track(song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap()
}

/// A tab-only staff still shows the tuplets the notation staff would carry.
#[test]
fn tab_only_shows_tuplet_numbers() {
    let song = common::open("basic/note_lengths.gp");
    let layout = common::layout_tab_only(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for tuplet in ["3", "5", "6", "7"] {
        assert!(
            texts.contains(&tuplet),
            "missing tuplet {tuplet:?} in the tab: {texts:?}"
        );
    }
}

/// A single staff shown as several rows (standard notation and TAB) is joined by a bracket
/// at the system's start (a thick bar with a curved wing at each end); one row gets none.
#[test]
fn a_staff_shown_as_notation_and_tab_gets_a_bracket() {
    // The bracket's bar (its two ends) and where its wings stand.
    type Bracket = (Vec<(Point, Point)>, Vec<(Glyph, Point)>);
    let bracket = |layout: &Layout| -> Bracket {
        // A thick barline is as thick as the bar, but stands inside the measures.
        let first_left = layout.places.measures[0].left;
        let bars = layout
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Line {
                    from,
                    to,
                    thickness,
                    color: ColorToken::Barline,
                } if (*thickness - 0.5).abs() < 1e-6
                    && (from.x - to.x).abs() < 1e-6
                    && from.x < first_left =>
                {
                    Some((*from, *to))
                }
                _ => None,
            })
            .collect();
        let wings = layout
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Glyph {
                    glyph: glyph @ (Glyph::BracketTop | Glyph::BracketBottom),
                    at,
                    ..
                } => Some((*glyph, *at)),
                _ => None,
            })
            .collect();
        (bars, wings)
    };
    let both = layout(TrackStaves::NOTATION_AND_TAB);
    let (bars, wings) = bracket(&both);
    let systems = both.places.systems.len();
    assert_eq!(bars.len(), systems, "one bar per system: {bars:?}");
    assert_eq!(wings.len(), 2 * systems, "two wings per system: {wings:?}");
    let (from, to) = bars[0];
    assert!(to.y - from.y > 8.0, "the bar spans both rows");
    // The wings sit at the bar's ends, starting at its left edge.
    let (top, top_at) = wings[0];
    let (bottom, bottom_at) = wings[1];
    assert_eq!((top, bottom), (Glyph::BracketTop, Glyph::BracketBottom));
    assert!((top_at.y - from.y).abs() < 1e-6 && (bottom_at.y - to.y).abs() < 1e-6);
    assert!((top_at.x - (from.x - 0.25)).abs() < 1e-6);
    // The bar stands left of the line that joins the rows.
    let joining = both.ops.iter().any(|op| {
        matches!(op, DrawOp::Line { from: start, to: end, color: ColorToken::Barline, .. }
            if (start.x - end.x).abs() < 1e-6
                && start.x > from.x + 0.25
                && (start.y - from.y).abs() < 1e-6
                && (end.y - to.y).abs() < 1e-6)
    });
    assert!(joining, "the joining line stands right of the bracket");
    // One row alone has no bracket.
    for staves in [TrackStaves::TAB_ONLY, TrackStaves::NOTATION_ONLY] {
        let (bars, wings) = bracket(&layout(staves));
        assert!(bars.is_empty() && wings.is_empty());
    }
}

/// Slash notation is a staff of its own: rhythm slashes on one line, without a clef, and
/// it combines with the other two staves as the top one, joined to them at the start.
#[test]
fn slash_notation_is_a_staff_of_its_own_and_combines() {
    let is_slash = |op: &DrawOp| matches!(op, DrawOp::Line { thickness, .. } if (*thickness - 0.24).abs() < 1e-6);
    let slash = layout(TrackStaves::SLASH_ONLY);
    assert_eq!(fret_numbers(&slash), 0, "no tablature");
    assert!(
        !slash.ops.iter().any(|op| matches!(
            op,
            DrawOp::Glyph {
                glyph: Glyph::GClef,
                ..
            }
        )),
        "the slash staff has no clef"
    );
    assert!(slash.ops.iter().any(is_slash), "a rhythm slash is drawn");

    let tab_and_slash = layout(TrackStaves::TAB_AND_SLASH);
    assert!(fret_numbers(&tab_and_slash) > 0, "the tablature stays");
    assert!(tab_and_slash.height > slash.height);

    let all = layout(TrackStaves::ALL_STAVES);
    assert!(fret_numbers(&all) > 0);
    assert!(all.height > tab_and_slash.height, "notation adds a staff");
    // The slashes are above the notation's clef, and the tab's numbers below it.
    let lowest_slash = all
        .ops
        .iter()
        .filter(|op| is_slash(op))
        .filter_map(|op| match op {
            DrawOp::Line { from, to, .. } => Some(from.y.max(to.y)),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    let clef = all
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::GClef,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .unwrap();
    assert!(lowest_slash < clef - 4.0, "the slash staff is on top");
    // One line at the system's start joins the slash staff to the tab below.
    let lowest_fret = all
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                at,
                color: ColorToken::FretNumber,
                ..
            } => Some(at.y),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    let start = all.ops.iter().find(|op| {
        matches!(op, DrawOp::Line { from, to, color: ColorToken::Barline, .. }
            if (from.x - to.x).abs() < 1e-3
                && from.y.min(to.y) < lowest_slash - 1.0
                && from.y.max(to.y) > lowest_fret)
    });
    assert!(
        start.is_some(),
        "a line joins the slash staff to the others"
    );
}

/// The horizontal screen view puts every measure on one line and widens the page to it.
#[test]
fn a_single_line_layout_is_wider_than_the_page() {
    let song = common::open("basic/note_lengths.gp");
    let fonts = Fonts::bundled().unwrap();
    let endless = |single_line| {
        let options = LayoutOptions {
            single_line,
            ..LayoutOptions::default()
        };
        layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap()
    };
    let broken = endless(false);
    let one_line = endless(true);
    assert!(broken.places.systems.len() > 1, "the score wraps normally");
    assert_eq!(one_line.places.systems.len(), 1, "everything on one line");
    assert!(
        one_line.width > broken.width,
        "the line is as wide as the whole score: {} vs {}",
        one_line.width,
        broken.width
    );
}

/// The layout reports the marks a click can open an editor for: the header's tempo and
/// tunings, the section names and the first bar's time signature.
#[test]
fn the_layout_reports_its_clickable_marks() {
    use gscore_layout::ScoreMarkKind;

    let song = common::open("basic/multi_staff.gp");
    let fonts = Fonts::bundled().unwrap();
    let layout =
        layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap();
    let has = |kind: ScoreMarkKind| layout.marks.iter().any(|mark| mark.kind == kind);
    let first = MeasureIndex::new(0);
    assert!(has(ScoreMarkKind::Tempo { measure: first }));
    assert!(has(ScoreMarkKind::Section { measure: first }));
    assert!(has(ScoreMarkKind::Section {
        measure: MeasureIndex::new(4)
    }));
    assert!(has(ScoreMarkKind::TimeSignature { measure: first }));
    assert!(has(ScoreMarkKind::Tuning {
        track: TrackIndex::new(0)
    }));
    // Every mark has a box, and a point in it finds the mark again.
    for mark in &layout.marks {
        assert!(mark.rect.width > 0.0 && mark.rect.height > 0.0, "{mark:?}");
        let middle = Point::new(
            mark.rect.origin.x + mark.rect.width / 2.0,
            mark.rect.origin.y + mark.rect.height / 2.0,
        );
        assert!(layout.mark_at(middle).is_some());
    }
    // A point beside the page's marks finds none.
    assert_eq!(layout.mark_at(Point::new(-5.0, -5.0)), None);
}
