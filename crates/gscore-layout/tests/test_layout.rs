//! Properties every layout must have, whatever the song: no panic, finite coordinates on
//! the page, one fret number per note, one staff line per string.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{
    ColorToken, DrawOp, Fonts, Glyph, Layout, LayoutError, LayoutOptions, Point, layout_track,
};
use gscore_model::testing::{self, BASS_4, GUITAR_6, GUITAR_7, GUITAR_8};
use gscore_model::{BeatKind, NoteKind, Song, TrackIndex};

/// Every point of every operation.
fn points(op: &DrawOp) -> Vec<Point> {
    match op {
        DrawOp::Line { from, to, .. } => vec![*from, *to],
        DrawOp::Rect { rect, .. } => vec![
            rect.origin,
            Point::new(rect.origin.x + rect.width, rect.origin.y + rect.height),
        ],
        DrawOp::Curve {
            from,
            to,
            control1,
            control2,
            ..
        } => vec![*from, *to, *control1, *control2],
        DrawOp::Polygon { points, .. } => points.clone(),
        DrawOp::Glyph { at, .. } | DrawOp::Text { at, .. } => vec![*at],
    }
}

/// Everything lies on the page and no number is NaN or infinite.
fn assert_on_page(layout: &Layout, what: &str) {
    assert!(
        layout.width.is_finite() && layout.height.is_finite(),
        "{what}"
    );
    for op in &layout.ops {
        for p in points(op) {
            assert!(p.x.is_finite() && p.y.is_finite(), "{what}: {op:?}");
            assert!(
                (0.0..=layout.width).contains(&p.x) && (0.0..=layout.height).contains(&p.y),
                "{what}: {op:?} is off the page ({} x {})",
                layout.width,
                layout.height
            );
        }
    }
}

/// The notes layout should show as fret numbers. A tie continuation is not written within a
/// measure (the origin already carries the note), but one carried into a measure's first
/// column is written as `(7)`.
fn shown_notes(song: &Song, track: usize) -> usize {
    let track = song.track(TrackIndex::new(track)).unwrap();
    let mut count = 0;
    for measure in track.measures() {
        for voice in measure.voices() {
            let mut first = true;
            for beat in voice.beats() {
                if beat.kind() == BeatKind::Empty {
                    continue;
                }
                if beat.kind() == BeatKind::Normal {
                    for note in beat.notes() {
                        match note.kind() {
                            NoteKind::Normal | NoteKind::Dead => count += 1,
                            NoteKind::Tie if first && !beat.is_grace() => count += 1,
                            _ => {}
                        }
                        // An attached grace note is written as a small fret number too.
                        if !beat.is_grace() && note.grace().is_some() {
                            count += 1;
                        }
                    }
                }
                if !beat.is_grace() {
                    first = false;
                }
            }
        }
    }
    count
}

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

fn staff_lines(layout: &Layout) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Line {
                    color: ColorToken::Staff,
                    ..
                }
            )
        })
        .count()
}

/// Rows of staff lines (distinct y positions), which is strings times systems.
fn systems(layout: &Layout, strings: usize) -> usize {
    staff_lines(layout) / strings
}

#[test]
fn every_test_file_lays_out_on_the_page_with_one_fret_number_per_note() {
    for file in common::TESTDATA {
        let song = common::open(file);
        for track in 0..song.track_count() {
            let layout = common::layout(&song, track);
            let what = format!("{file} track {}", track + 1);
            assert_on_page(&layout, &what);
            // A drum kit is written on a percussion staff, without fret numbers.
            let drums = song.track(TrackIndex::new(track)).unwrap().is_percussion();
            let expected = if drums { 0 } else { shown_notes(&song, track) };
            assert_eq!(fret_numbers(&layout), expected, "{what}");
            assert_eq!(
                layout,
                common::layout(&song, track),
                "{what}: not deterministic"
            );
        }
    }
}

#[test]
fn every_string_count_gets_one_line_per_string() {
    for tuning in [BASS_4, GUITAR_6, GUITAR_7, GUITAR_8] {
        let song = testing::song(&[tuning], 3).unwrap();
        let layout = common::layout_tab_only(&song, 0);
        assert_on_page(&layout, &format!("{} strings", tuning.len()));
        assert_eq!(staff_lines(&layout) % tuning.len(), 0);
        assert_eq!(
            systems(&layout, tuning.len()),
            1,
            "three empty measures fit one line"
        );
    }
}

#[test]
fn long_songs_break_into_lines_that_fit_the_page() {
    let song = testing::song(&[GUITAR_6], 200).unwrap();
    let layout = common::layout_tab_only(&song, 0);
    assert_on_page(&layout, "200 measures");
    assert!(systems(&layout, 6) > 10, "{} systems", systems(&layout, 6));
    let right_margin = LayoutOptions::default().page_width - LayoutOptions::default().margin_right;
    for op in &layout.ops {
        if let DrawOp::Line { to, .. } = op {
            assert!(to.x <= right_margin + 1e-3, "{op:?} runs into the margin");
        }
    }
}

#[test]
fn songs_without_tracks_or_measures_are_handled() {
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let missing = layout_track(&testing::empty(), TrackIndex::new(0), &options, &fonts);
    assert_eq!(missing, Err(LayoutError::NoSuchTrack(TrackIndex::new(0))));
    let one = testing::one_measure().unwrap();
    assert!(layout_track(&one, TrackIndex::new(1), &options, &fonts).is_err());

    let no_measures = testing::song(&[GUITAR_6], 0).unwrap();
    let layout = common::layout(&no_measures, 0);
    assert_on_page(&layout, "no measures");
    assert_eq!(staff_lines(&layout), 0, "no measures, no staff");
}

#[test]
fn odd_options_do_not_panic() {
    let fonts = Fonts::bundled().unwrap();
    let song = common::open("basic/note_lengths.gp");
    for options in [
        LayoutOptions {
            page_width: 1.0,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            page_width: 0.0,
            margin_left: 100.0,
            margin_right: 100.0,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            tab_line_spacing: f32::NAN,
            fret_size: f32::INFINITY,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            spacing: gscore_layout::Spacing {
                quarter: -5.0,
                doubling_ratio: 0.0,
                min_space: f32::NAN,
                min_gap: -1.0,
                barline_padding: f32::NAN,
            },
            ..LayoutOptions::default()
        },
    ] {
        // Nonsense settings may give a nonsense picture, but never a panic.
        let _ = layout_track(&song, TrackIndex::new(0), &options, &fonts);
    }
}

#[test]
fn unreadable_notes_show_as_question_marks_instead_of_failing_the_track() {
    // A negative fret is shown as `?`; a note on a 9th string of a 6-string guitar has no
    // line and is left out. Both are listed by the model as issues.
    let song = testing::one_beat_with_raw_notes(&[(1, -7), (2, 5), (9, 3)]).unwrap();
    assert_eq!(song.issues().len(), 2, "{:?}", song.issues());
    let layout = common::layout(&song, 0);
    assert_on_page(&layout, "odd notes");
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                text,
                color: ColorToken::FretNumber,
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["?", "5"]);
}

/// A note never sits right against the barline at either end of a measure.
#[test]
fn notes_keep_their_distance_from_barlines() {
    let song = common::with_notes(testing::song(&[GUITAR_6], 1).unwrap(), &[(0, 0, 1, 0)]);
    let layout = common::layout(&song, 0);
    let barlines: Vec<f32> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                to,
                color: ColorToken::Barline,
                ..
            } => Some(to.x),
            _ => None,
        })
        .collect();
    let notes: Vec<f32> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                at,
                color: ColorToken::FretNumber,
                ..
            } => Some(at.x),
            _ => None,
        })
        .collect();
    assert_eq!(notes.len(), 1, "one note");
    assert!(barlines.len() >= 2, "a start and an end line");
    let padding = LayoutOptions::default().spacing.barline_padding;
    for note in &notes {
        for barline in &barlines {
            assert!(
                (note - barline).abs() >= padding,
                "note at {note} is {:.2} from a barline at {barline} (want {padding})",
                (note - barline).abs()
            );
        }
    }
}

/// The ornaments file must actually show its effects, not just the note grid.
#[test]
fn ornaments_are_drawn_on_the_tab_staff() {
    let song = common::open("basic/ornaments.gp");
    let layout = common::layout(&song, 0);
    assert_on_page(&layout, "ornaments");
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for expected in [
        "(0)",      // ghost note in parentheses
        "<7>",      // natural harmonic
        "7 <19>",   // pinch harmonic and its node
        "P.M.",     // palm-mute row
        "let ring", // let-ring row
        "full",     // bend amount
        "sl.",      // legato slide
        "H",        // hammer-on
        "P",        // pull-off
        "PH",       // pinch harmonic
        "-3",       // whammy dive
    ] {
        assert!(texts.contains(&expected), "missing {expected:?}: {texts:?}");
    }
    // Only the ghost note is parenthesized; the tie continuation is not written at all.
    assert_eq!(texts.iter().filter(|text| **text == "(0)").count(), 1);
    let glyph_count = |wanted: Glyph| {
        layout
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Glyph { glyph, .. } if *glyph == wanted))
            .count()
    };
    // With notation, accents and the staccato dot are engraved by the noteheads.
    let either = |above, below| glyph_count(above) + glyph_count(below);
    assert_eq!(either(Glyph::AccentAbove, Glyph::AccentBelow), 1, "accent");
    assert_eq!(
        either(Glyph::MarcatoAbove, Glyph::MarcatoBelow),
        1,
        "heavy accent"
    );
    assert_eq!(
        either(Glyph::StaccatoAbove, Glyph::StaccatoBelow),
        1,
        "staccato"
    );
    assert!(
        !texts.contains(&">") && !texts.contains(&"v"),
        "not on the tab too"
    );
    // The tab-only layout keeps them on the tab.
    let tab_texts: Vec<String> = common::layout_tab_only(&song, 0)
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for expected in [">", "v"] {
        assert!(
            tab_texts.iter().any(|t| t == expected),
            "tab-only {expected:?}"
        );
    }
    assert!(
        layout
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::Curve { .. })),
        "slurs and bends are curves"
    );
}

#[test]
fn notation_adds_a_five_line_staff_and_one_notehead_per_note() {
    use gscore_layout::Glyph;
    // DADGAD: six open strings as eighths, then a quarter rest; one system.
    let song = common::open("basic/steel_guitar_dadgad.gp");
    let with = common::layout(&song, 0);
    let without = common::layout_tab_only(&song, 0);
    assert_on_page(&with, "notation");
    // Staff lines run across the system; ledger lines (also staff-colored) are short.
    let long_lines = |layout: &Layout| {
        layout
            .ops
            .iter()
            .filter(|op| {
                matches!(op, DrawOp::Line { from, to, color: ColorToken::Staff, .. }
                    if to.x - from.x > 10.0)
            })
            .count()
    };
    assert_eq!(long_lines(&with), long_lines(&without) + 5);
    let glyphs = |layout: &Layout, wanted: Glyph| {
        layout
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Glyph { glyph, .. } if *glyph == wanted))
            .count()
    };
    assert_eq!(glyphs(&with, Glyph::NoteheadBlack), 6);
    assert_eq!(glyphs(&with, Glyph::GClef), 1);
    // The rest is written in the notation only, as in Guitar Pro.
    assert_eq!(glyphs(&with, Glyph::RestQuarter), 1);
    assert_eq!(glyphs(&without, Glyph::RestQuarter), 1);
    // Bass tracks get the bass clef.
    let bass = common::layout(&testing::song(&[BASS_4], 1).unwrap(), 0);
    assert_eq!(glyphs(&bass, Glyph::FClef), 1);
}

/// The staff test file changes clefs through the score: treble, bass, alto, tenor, treble.
#[test]
fn clef_changes_are_read_and_drawn() {
    let song = common::open("basic/staff.gp");
    let layout = common::layout(&song, 0);
    let count = |wanted: Glyph| {
        layout
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Glyph { glyph, .. } if *glyph == wanted))
            .count()
    };
    assert!(count(Glyph::GClef) >= 1, "treble clef");
    assert!(count(Glyph::FClef) >= 1, "bass clef change");
    assert!(
        count(Glyph::CClef) >= 2,
        "alto and tenor clef changes (got {})",
        count(Glyph::CClef)
    );
}

/// The staff file's free-time measure says so, and its time signature is in parentheses.
#[test]
fn free_time_is_written_over_its_measure() {
    let song = common::open("basic/staff.gp");
    let layout = common::layout(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().filter(|text| **text == "Free time").count() == 1,
        "{texts:?}"
    );
}

/// A run of free-time bars is marked once, where it starts: the bars after it repeat
/// neither "Free time" nor the time signature (each keeps its dashed barline).
#[test]
fn a_run_of_free_time_bars_is_marked_once() {
    use gscore_model::MeasureIndex;
    let count = |song: &Song| {
        let layout = common::layout(song, 0);
        let labels = layout
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Text { text, .. } if text == "Free time"))
            .count();
        let digits = layout
            .ops
            .iter()
            .filter(|op| {
                matches!(
                    op,
                    DrawOp::Glyph {
                        glyph: Glyph::TimeSigDigit(_),
                        ..
                    }
                )
            })
            .count();
        (labels, digits)
    };
    let mut song = common::open("basic/staff.gp");
    let alone = count(&song);
    // Bar 7 is free; bar 8 joins it.
    let _ = song.set_free_time(MeasureIndex::new(7), true).unwrap();
    let run = count(&song);
    assert_eq!(run.0, 1, "one label for the run");
    assert_eq!(run.1, alone.1, "no second time signature inside the run");
}

/// Bar repetitions are read and drawn: the staff file repeats measures 9 to 11.
#[test]
fn simile_marks_are_read_and_drawn() {
    use gscore_model::{MeasureIndex, SimileMark};
    let song = common::open("basic/staff.gp");
    let track = song.track(TrackIndex::new(0)).unwrap();
    assert_eq!(
        track.measure(MeasureIndex::new(8)).unwrap().simile_mark(),
        Some(SimileMark::Simple)
    );
    assert_eq!(
        track.measure(MeasureIndex::new(9)).unwrap().simile_mark(),
        Some(SimileMark::FirstOfDouble)
    );
    assert_eq!(
        track.measure(MeasureIndex::new(10)).unwrap().simile_mark(),
        Some(SimileMark::SecondOfDouble)
    );
    let layout = common::layout(&song, 0);
    let slashes = layout
        .ops
        .iter()
        .filter(
            |op| matches!(op, DrawOp::Line { thickness, .. } if (*thickness - 0.16).abs() < 1e-6),
        )
        .count();
    assert!(
        slashes >= 5,
        "one simple mark and two double marks: {slashes}"
    );
}

/// Octave signs are read from the file and written over/under the staff.
#[test]
fn octave_signs_are_read_and_drawn() {
    let song = common::open("basic/staff.gp");
    let layout = common::layout(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for sign in ["8va", "8vb", "15ma", "15mb"] {
        assert!(texts.contains(&sign), "missing {sign:?}: {texts:?}");
    }
}

/// The chord diagram of the staff file is read and drawn with its name and shape.
#[test]
fn chord_diagrams_are_read_and_drawn() {
    let song = common::open("basic/staff.gp");
    let diagram = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measures()
        .flat_map(|measure| {
            measure
                .voices()
                .flat_map(|voice| voice.beats())
                .filter_map(|beat| beat.chord_diagram())
        })
        .next()
        .expect("the file has a chord diagram");
    assert_eq!(diagram.name, "Gb7/#11");
    assert_eq!(diagram.string_count, 6);
    assert!(!diagram.frets.is_empty());
    let layout = common::layout(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Gb7/#11"), "{texts:?}");
    // The diagram has its own row: its grid ends above the beat text of the same system
    // ("Octaves"), instead of running into it.
    let baseline = |wanted: &str| {
        layout
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::Text { text, at, .. } if text == wanted => Some(at.y),
                _ => None,
            })
            .unwrap()
    };
    let grid_bottom = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            // The dots are the round polygons (arrowheads have three corners).
            DrawOp::Polygon { points, .. } if points.len() > 8 => {
                points.iter().map(|p| p.y).reduce(f32::max)
            }
            _ => None,
        })
        .reduce(f32::max)
        .expect("the diagram's dots");
    assert!(baseline("Gb7/#11") < grid_bottom);
    assert!(grid_bottom + 1.0 < baseline("Octaves") - 1.0);
}

/// The staff file's repeats, alternative endings and key change are read and drawn.
#[test]
fn repeats_endings_and_key_changes_come_from_the_staff_file() {
    use gscore_model::MeasureIndex;
    let song = common::open("basic/staff.gp");
    let first = song.repeat(MeasureIndex::new(0)).unwrap();
    assert!(first.start);
    let second = song.repeat(MeasureIndex::new(1)).unwrap();
    assert_eq!(second.end_repeats, Some(1), "one repeat back to the start");
    assert_eq!(second.endings, 1);
    assert_eq!(
        song.repeat(MeasureIndex::new(2)).unwrap().endings,
        2,
        "the second ending"
    );
    let key = song.key_signature(MeasureIndex::new(2)).unwrap();
    assert_eq!((key.key, key.minor), (-4, true));
    assert!(song.free_time(MeasureIndex::new(6)).unwrap());

    let layout = common::layout(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for label in ["1.", "2."] {
        assert!(
            texts.contains(&label),
            "missing ending {label:?}: {texts:?}"
        );
    }
    assert!(
        layout.ops.iter().any(|op| matches!(
            op,
            DrawOp::Glyph {
                glyph: Glyph::RepeatDot,
                ..
            }
        )),
        "repeat dots are drawn"
    );
}

/// Time signature changes are laid out: 3/8 in a 4/4 piece.
#[test]
fn time_signature_changes_are_laid_out() {
    use gscore_model::{MeasureIndex, TimeSignature};
    let mut song = testing::song(&[GUITAR_6], 2).unwrap();
    let signature = TimeSignature::new(3, 8).unwrap();
    let _change = song.set_time_signature(MeasureIndex::new(1), 3, 8).unwrap();
    let track = song.track(TrackIndex::new(0)).unwrap();
    assert_eq!(
        track
            .measure(MeasureIndex::new(1))
            .unwrap()
            .time_signature()
            .unwrap(),
        signature
    );
    let layout = common::layout(&song, 0);
    let digits: Vec<u8> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::TimeSigDigit(digit),
                ..
            } => Some(digit.get()),
            _ => None,
        })
        .collect();
    assert!(digits.contains(&3), "the numerator: {digits:?}");
    assert!(digits.contains(&8), "the denominator: {digits:?}");
}
