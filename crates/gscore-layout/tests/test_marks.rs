//! Marks besides the notes: section names, tempo changes, repeat signs, alternative
//! endings and dynamics.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use gscore_layout::{
    ColorToken, DrawOp, Fonts, Glyph, Layout, LayoutCache, LayoutOptions, TextFont, layout_track,
};
use gscore_model::{
    BeatIndex, BeatPosition, Direction, Dynamic, Feel, Fermata, Fret, MeasureIndex, Song,
    StringNumber, TrackColor, TrackIndex, VoiceIndex, testing,
};

fn at(measure: usize, beat: usize) -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn layout(song: &Song) -> Layout {
    let fonts = Fonts::bundled().unwrap();
    layout_track(song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap()
}

fn texts(layout: &Layout) -> Vec<(String, TextFont, f32, f32)> {
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, font, at, .. } => Some((text.clone(), *font, at.x, at.y)),
            _ => None,
        })
        .collect()
}

fn glyph_count(layout: &Layout, glyph: Glyph) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Glyph { glyph: g, .. } if *g == glyph))
        .count()
}

fn note(song: &mut Song, measure: usize, beat: usize) {
    let _ = song
        .set_note(
            at(measure, beat),
            StringNumber::new(1).unwrap(),
            Fret::new(3).unwrap(),
        )
        .unwrap();
}

#[test]
fn a_double_barline_draws_two_lines() {
    let plain = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let mut marked = plain.clone();
    let _ = marked.set_double_bar(MeasureIndex::new(0), true).unwrap();
    let barline_lines_at = |layout: &Layout, x: f32| {
        layout
            .ops
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    DrawOp::Line { from, to, color, .. }
                        if *color == ColorToken::Barline
                            && (from.x - to.x).abs() < 0.01
                            && (from.x - x).abs() < 1.0
                )
            })
            .count()
    };
    let plain = layout(&plain);
    let marked = layout(&marked);
    let plain_count = barline_lines_at(&plain, plain.places.measures[0].right);
    assert_eq!(plain_count, 2, "one barline per staff");
    assert_eq!(
        barline_lines_at(&marked, marked.places.measures[0].right),
        plain_count * 2
    );
}

#[test]
fn a_feel_is_written_where_it_changes() {
    let mut song = testing::song(&[testing::GUITAR_6], 4).unwrap();
    // Swing from measure 1 on (it holds), back to straight from measure 3.
    let _ = song.set_feel(MeasureIndex::new(0), Feel::Eighth).unwrap();
    let _ = song.set_feel(MeasureIndex::new(2), Feel::None).unwrap();
    let laid_out = layout(&song);
    // Swing at measure 1 (not again at 2), straight at 3: a beamed pair each, a triplet's
    // 3 once, and "=" twice.
    assert_eq!(glyph_count(&laid_out, Glyph::TextBeam8th), 3);
    assert_eq!(glyph_count(&laid_out, Glyph::Tuplet3), 1);
    let equals = texts(&laid_out)
        .into_iter()
        .filter(|(text, ..)| text == "=")
        .count();
    assert_eq!(equals, 2);
    let y = laid_out
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::Tuplet3,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .expect("the triplet number");
    assert!(y < laid_out.places.systems[0].top);
    // A straight song has no feel mark.
    let plain = layout(&testing::song(&[testing::GUITAR_6], 2).unwrap());
    assert_eq!(glyph_count(&plain, Glyph::TextNote), 0);
}

#[test]
fn a_fermata_is_drawn_above_the_measure() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let _ = song
        .set_fermata(MeasureIndex::new(0), Some(Fermata::Long))
        .unwrap();
    let laid_out = layout(&song);
    assert_eq!(glyph_count(&laid_out, Glyph::Fermata(Fermata::Long)), 1);
    let y = laid_out
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::Fermata(_),
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .expect("the fermata glyph");
    assert!(y < laid_out.places.systems[0].top);
}

#[test]
fn signs_start_a_bar_and_jumps_end_it() {
    let mut song = testing::song(&[testing::GUITAR_6], 3).unwrap();
    let _ = song
        .set_directions(MeasureIndex::new(0), &[Direction::Segno])
        .unwrap();
    let _ = song
        .set_directions(
            MeasureIndex::new(1),
            &[Direction::Fine, Direction::DaCoda, Direction::Coda],
        )
        .unwrap();
    let _ = song
        .set_directions(MeasureIndex::new(2), &[Direction::DaSegnoAlCoda])
        .unwrap();
    let laid_out = layout(&song);
    // The coda sign at bar 2's start and after "To Coda"; the segno once.
    assert_eq!(glyph_count(&laid_out, Glyph::Segno), 1);
    assert_eq!(glyph_count(&laid_out, Glyph::Coda), 2);
    let texts = texts(&laid_out);
    let find = |text: &str| {
        texts
            .iter()
            .find(|t| t.0 == text)
            .unwrap_or_else(|| panic!("{text} is written"))
            .clone()
    };
    let (fine, to_coda, jump) = (find("Fine"), find("To Coda"), find("D.S. al Coda"));
    let place = |n: usize| &laid_out.places.measures[n];
    // Texts are right-aligned at their bar's end, "Fine" before "To Coda".
    assert!(fine.2 > place(1).left && fine.2 < to_coda.2 && to_coda.2 <= place(1).right);
    assert!(jump.2 > place(2).left && jump.2 <= place(2).right);
    // Above the staff, in one row.
    let top = laid_out.places.systems[0].top;
    assert!(fine.3 < top && (fine.3 - jump.3).abs() < 1e-3);
    // Each bar's signs are a click area that names the bar.
    let marks: Vec<_> = laid_out
        .marks
        .iter()
        .filter_map(|mark| match mark.kind {
            gscore_layout::ScoreMarkKind::Directions { measure } => Some(measure.get()),
            _ => None,
        })
        .collect();
    assert_eq!(marks, [0, 1, 1, 2]);
    // A song without signs has no row for them.
    let plain = layout(&testing::song(&[testing::GUITAR_6], 3).unwrap());
    assert!(plain.places.systems[0].top < top);
}

#[test]
fn section_names_and_tempo_changes_are_written_above_their_measure() {
    let mut song = testing::song(&[testing::GUITAR_6], 3).unwrap();
    let _ = song
        .set_section(MeasureIndex::new(1), "Chorus", TrackColor::DEFAULT)
        .unwrap();
    let _ = song.set_measure_tempo(MeasureIndex::new(2), 90).unwrap();
    let plain = layout(&testing::song(&[testing::GUITAR_6], 3).unwrap());
    let marked = layout(&song);
    let texts = texts(&marked);
    let section = texts
        .iter()
        .find(|t| t.0 == "Chorus")
        .expect("the section name");
    assert_eq!(section.1, TextFont::Bold);
    let tempo = texts
        .iter()
        .find(|t| t.0 == "= 90")
        .expect("the tempo change");
    let measure = |n: usize| marked.places.measures[n].left;
    assert!(section.2 >= measure(1) && section.2 < measure(2));
    assert!(tempo.2 >= measure(2));
    // The header's tempo is the only other one; tempo rows sit above the staff.
    assert_eq!(texts.iter().filter(|t| t.0.starts_with("= ")).count(), 2);
    let top = marked.places.systems[0].top;
    assert!(section.3 < tempo.3 && tempo.3 < top);
    // The rows make room: the staff moves down by them.
    assert!(top > plain.places.systems[0].top);
}

#[test]
fn an_unchanged_tempo_is_not_written_again() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let tempo = song.tempo();
    let _ = song.set_measure_tempo(MeasureIndex::new(1), tempo).unwrap();
    let count = texts(&layout(&song))
        .iter()
        .filter(|t| t.0.starts_with("= "))
        .count();
    assert_eq!(count, 1, "only the header's tempo");
}

#[test]
fn dynamics_are_written_where_they_change() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    for (measure, beat) in [(0, 0), (0, 1), (1, 0), (1, 2)] {
        note(&mut song, measure, beat);
    }
    // New notes are mezzo-forte, which is not written.
    assert_eq!(glyph_count(&layout(&song), Glyph::Dynamic(Dynamic::Mf)), 0);
    let _ = song.set_dynamic(at(0, 1), Dynamic::P).unwrap();
    // Measure 2's notes were entered at mezzo-forte: back to mf after the p.
    let laid_out = layout(&song);
    assert_eq!(glyph_count(&laid_out, Glyph::Dynamic(Dynamic::P)), 1);
    assert_eq!(glyph_count(&laid_out, Glyph::Dynamic(Dynamic::Mf)), 1);
    // The cache gives the same page, also after the next edit.
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let mut cache = LayoutCache::default();
    let _ = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .unwrap();
    let _ = song.set_dynamic(at(1, 0), Dynamic::P).unwrap();
    let cached = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .unwrap();
    assert_eq!(cached, layout(&song));
    // The p now lasts into measure 2; its last note (still mf) gets the mf.
    assert_eq!(glyph_count(&cached, Glyph::Dynamic(Dynamic::P)), 1);
    assert_eq!(glyph_count(&cached, Glyph::Dynamic(Dynamic::Mf)), 1);
}

#[test]
fn repeats_and_endings_are_drawn() {
    // ||: 1 | 2 | 3 (ending 1) :|| 4 (ending 2) | 5, the section played three times.
    let song = testing::song_with_repeats(
        5,
        &[(0, true, -1, 0), (2, false, 2, 0b01), (3, false, -1, 0b10)],
    )
    .unwrap();
    let laid_out = layout(&song);
    // Notation and tab each get two dots at the start and two at the end.
    assert_eq!(glyph_count(&laid_out, Glyph::RepeatDot), 8);
    let texts = texts(&laid_out);
    for label in ["1.", "2.", "x3"] {
        assert!(texts.iter().any(|t| t.0 == label), "{label} in {texts:?}");
    }
    // Repeat signs take room of their own.
    let plain = layout(&testing::song(&[testing::GUITAR_6], 5).unwrap());
    let width = |l: &Layout, n: usize| l.places.measures[n].right - l.places.measures[n].left;
    assert!(width(&laid_out, 0) > width(&plain, 0));
    assert!(laid_out.places.measures[0].content > laid_out.places.measures[0].left);
    // Thick barlines: one per staff for the start, one per staff for the end, plus the
    // final barline's. (The bracket's bar is as thick, but stands left of the measures.)
    let first_left = laid_out.places.measures[0].left;
    let thick = laid_out
        .ops
        .iter()
        .filter(|op| {
            matches!(op, DrawOp::Line { from, thickness, color: ColorToken::Barline, .. }
                if (*thickness - 0.5).abs() < 1e-4 && from.x >= first_left)
        })
        .count();
    assert_eq!(thick, 6);
}

#[test]
fn an_ending_bracket_spans_every_bar_of_the_ending() {
    // |: 0 [1. 1 2 3 :| [2. 4 | 5 — Guitar Pro marks only bar 1 of the first ending and
    // extends it over 2 and 3, through the repeat's close.
    let song = testing::song_with_repeats(
        6,
        &[
            (0, true, -1, 0),
            (1, false, -1, 0b01),
            (3, false, 1, 0),
            (4, false, -1, 0b10),
        ],
    )
    .unwrap();
    let laid_out = layout(&song);
    let bar = |n: usize| {
        let place = &laid_out.places.measures[n];
        (place.left, place.right)
    };
    // The bracket's line is a horizontal annotation line, one piece per measure, and
    // continuous where the measures meet.
    let horizontal: Vec<(f32, f32)> = laid_out
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                from,
                to,
                color: ColorToken::Annotation,
                ..
            } if (from.y - to.y).abs() < 0.01 => Some((from.x, to.x)),
            _ => None,
        })
        .collect();
    for n in [1, 2, 3] {
        let (left, right) = bar(n);
        assert!(
            horizontal
                .iter()
                .any(|&(from, to)| from <= left + 0.21 && to >= right - 0.21),
            "bar {n} lies under the bracket: {horizontal:?}"
        );
    }
    // The numbers are written where their brackets start, once each (bar 1 and bar 4).
    let texts = texts(&laid_out);
    assert_eq!(texts.iter().filter(|(text, ..)| text == "1.").count(), 1);
    assert_eq!(texts.iter().filter(|(text, ..)| text == "2.").count(), 1);
    // The opening hook sits at the first bar, the closing one at the bracket's end (left
    // of the repeat sign that closes it).
    let hooks: Vec<f32> = laid_out
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                from,
                to,
                color: ColorToken::Annotation,
                ..
            } if (from.x - to.x).abs() < 0.01 && (from.y - to.y).abs() > 0.1 => Some(from.x),
            _ => None,
        })
        .collect();
    let closing = horizontal
        .iter()
        .find(|(from, _)| (from - bar(3).0).abs() < 0.01)
        .map(|&(_, to)| to)
        .unwrap_or_default();
    assert!(
        hooks.iter().any(|&x| (x - (bar(1).0 + 0.2)).abs() < 0.01),
        "opening hook at bar 1: {hooks:?}"
    );
    assert!(
        hooks.iter().any(|&x| (x - closing).abs() < 0.01),
        "closing hook at the repeat: {hooks:?}"
    );
}

#[test]
fn the_same_ending_number_on_several_bars_is_one_bracket() {
    // GP3-5 write an ending's number on every bar it covers; the bracket is still one,
    // labelled once.
    let song = testing::song_with_repeats(
        4,
        &[(0, true, -1, 0), (1, false, -1, 0b01), (2, false, 1, 0b01)],
    )
    .unwrap();
    let laid_out = layout(&song);
    assert_eq!(
        texts(&laid_out)
            .iter()
            .filter(|(text, ..)| text == "1.")
            .count(),
        1
    );
    let horizontal = laid_out
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Line {
                    from,
                    to,
                    color: ColorToken::Annotation,
                    ..
                } if (from.y - to.y).abs() < 0.01
            )
        })
        .count();
    assert_eq!(horizontal, 2, "a piece of line over each of the two bars");
}

/// Every glyph's box on the page: `(glyph, left, top, right, bottom)`.
fn glyph_boxes(layout: &Layout) -> Vec<(Glyph, f32, f32, f32, f32)> {
    let fonts = Fonts::bundled().unwrap();
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph, at, scale, ..
            } => {
                let b = fonts.music.bbox(*glyph);
                Some((
                    *glyph,
                    at.x + b.x_min * scale,
                    at.y - b.y_max * scale,
                    at.x + b.x_max * scale,
                    at.y - b.y_min * scale,
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn dynamics_under_low_notes_clear_every_note_they_span_and_the_tab() {
    use gscore_model::testing::TestNote;
    // Eighths low on the guitar (ledger lines below the staff), some with accidentals,
    // their dynamic changing from note to note: forte, fortississimo, forte, ...
    let low: [&[TestNote]; 8] = [
        &[TestNote::new(6, 2), TestNote::new(5, 2)],
        &[TestNote::new(6, 1)],
        &[TestNote::new(6, 2)],
        &[TestNote::new(6, 0)],
        &[TestNote::new(6, 2), TestNote::new(5, 2)],
        &[TestNote::new(6, 1)],
        &[TestNote::new(6, 2)],
        &[TestNote::new(6, 4), TestNote::new(5, 4)],
    ];
    let beats: Vec<(u16, &[TestNote])> = low.iter().map(|notes| (8, *notes)).collect();
    let mut song = testing::song_with_notes(&beats).unwrap();
    for beat in 0..8 {
        let dynamic = if beat % 2 == 0 {
            Dynamic::F
        } else {
            Dynamic::Fff
        };
        let _ = song.set_dynamic(at(0, beat), dynamic).unwrap();
    }
    let laid_out = layout(&song);
    let boxes = glyph_boxes(&laid_out);
    let dynamics: Vec<_> = boxes
        .iter()
        .filter(|b| matches!(b.0, Glyph::Dynamic(_)))
        .collect();
    assert_eq!(dynamics.len(), 8, "every change is written");
    for dynamic in &dynamics {
        for other in boxes.iter().filter(|b| !matches!(b.0, Glyph::Dynamic(_))) {
            let apart = dynamic.3 <= other.1
                || other.3 <= dynamic.1
                || dynamic.4 <= other.2
                || other.4 <= dynamic.2;
            assert!(apart, "{dynamic:?} overlaps {other:?}");
        }
    }
    // The tab's top line: the sixth staff line from the top (below the notation's five;
    // ledger lines are short and left out).
    let mut lines: Vec<f32> = laid_out
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                from, to, color, ..
            } if *color == ColorToken::Staff
                && (from.y - to.y).abs() < 0.01
                && to.x - from.x > 10.0 =>
            {
                Some(from.y)
            }
            _ => None,
        })
        .collect();
    lines.sort_by(f32::total_cmp);
    lines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let tab_top = lines[5];
    for dynamic in &dynamics {
        assert!(
            dynamic.4 < tab_top - 0.5,
            "{dynamic:?} reaches the tab at {tab_top}"
        );
    }
}
