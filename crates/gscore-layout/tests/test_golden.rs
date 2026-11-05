//! SVG output for the owner's test files and a few synthetic songs, compared with the
//! reference files in `tests/golden/`.
//!
//! If the output changes on purpose, regenerate the references with
//! `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-layout --test test_golden`, look at them
//! (for example with `rsvg-convert`, see docs/development.md) and have the owner review the
//! visual change before committing.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{LayoutOptions, Theme, to_svg};
use gscore_model::Song;
use gscore_model::testing::{self, BASS_4, GUITAR_8};

/// Compares `svg` with `tests/golden/<name>.svg`, or writes it when updating.
fn check(name: &str, svg: &str) {
    let path = common::crate_dir()
        .join("tests/golden")
        .join(format!("{name}.svg"));
    if std::env::var_os("GSCORE_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, svg).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; create it with GSCORE_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    if expected != svg {
        let first = expected
            .lines()
            .zip(svg.lines())
            .position(|(a, b)| a != b)
            .map_or_else(|| "length".to_owned(), |line| format!("line {}", line + 1));
        panic!(
            "{name}: the SVG differs from {} (first difference: {first}). If the change is \
             intended, regenerate with GSCORE_UPDATE_GOLDEN=1 and have it reviewed.",
            path.display()
        );
    }
}

fn svg(song: &Song, theme: &Theme) -> String {
    to_svg(
        &common::layout(song, 0),
        theme,
        LayoutOptions::default().staff_space_mm,
    )
}

#[test]
fn owner_files_match_their_golden_svg() {
    for name in [
        "steel_guitar_dadgad",
        "note_lengths",
        "ornaments",
        "advanced_ornaments",
        "multi_staff",
    ] {
        let song = common::open(&format!("basic/{name}.gp"));
        check(name, &svg(&song, &Theme::LIGHT));
    }
}

#[test]
fn every_track_together_matches_its_golden_svg() {
    let song = common::open("basic/multi_staff.gp");
    let tracks: Vec<_> = song.tracks().map(|track| track.index()).collect();
    let layout = gscore_layout::layout_tracks(
        &song,
        &tracks,
        tracks[0],
        &LayoutOptions::default(),
        &gscore_layout::Fonts::bundled().unwrap(),
    )
    .unwrap();
    check(
        "multi_staff.all",
        &to_svg(
            &layout,
            &Theme::LIGHT,
            LayoutOptions::default().staff_space_mm,
        ),
    );
}

#[test]
fn the_dark_theme_only_changes_colors() {
    let song = common::open("basic/steel_guitar_dadgad.gp");
    let dark = svg(&song, &Theme::DARK);
    check("steel_guitar_dadgad.dark", &dark);
    let light = svg(&song, &Theme::LIGHT);
    let without_colors = |svg: &str| -> String {
        svg.split('"')
            .filter(|part| !part.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\"")
    };
    assert_eq!(without_colors(&light), without_colors(&dark));
}

#[test]
fn bass_and_eight_string_guitar_match_their_golden_svg() {
    let bass = common::with_notes(
        testing::song(&[BASS_4], 2).unwrap(),
        &[
            (0, 0, 4, 0),
            (0, 1, 4, 3),
            (0, 2, 3, 5),
            (0, 3, 1, 12),
            (1, 0, 2, 7),
        ],
    );
    check("bass_4_strings", &svg(&bass, &Theme::LIGHT));

    let eight = common::with_notes(
        testing::song(&[GUITAR_8], 2).unwrap(),
        &[
            (0, 0, 8, 0),
            (0, 0, 7, 2),
            (0, 1, 1, 24),
            (0, 2, 4, 10),
            (1, 3, 6, 3),
        ],
    );
    check("guitar_8_strings", &svg(&eight, &Theme::LIGHT));
}
