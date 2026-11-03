//! GuitarScore tests for the page additions: the page setup from GP7's style sheet and the
//! line breaks from its systems layout, read from Guitar Pro 7 and written back; page
//! breaks as a GuitarScore attribute.

use crate::io::Song;
use crate::model::enums::LineBreak;
use crate::model::page::{HEADER_FOOTER_PAGE_NUMBER, HEADER_FOOTER_RUNNING_HEADER};

/// A Guitar Pro 7 file with 12 bars, three to a line, embedded at build time.
const ORNAMENTS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/basic/ornaments.gp"
));

fn read(bytes: &[u8]) -> Song {
    let mut song = Song::default();
    song.read_gp(bytes).expect("reads");
    song
}

fn breaks(song: &Song) -> Vec<(usize, LineBreak)> {
    song.tracks[0]
        .measures
        .iter()
        .enumerate()
        .filter(|(_, measure)| measure.line_break != LineBreak::None)
        .map(|(index, measure)| (index, measure.line_break.clone()))
        .collect()
}

#[test]
fn guitar_pro_7_page_setup_and_line_breaks_are_read() {
    let song = read(ORNAMENTS);
    let setup = &song.page_setup;
    assert_eq!((setup.page_size.x, setup.page_size.y), (210, 297));
    assert_eq!(
        (
            setup.page_margin.top,
            setup.page_margin.left,
            setup.page_margin.bottom
        ),
        (15, 10, 10)
    );
    assert_ne!(setup.header_and_footer & HEADER_FOOTER_PAGE_NUMBER, 0);
    assert_eq!(setup.header_and_footer & HEADER_FOOTER_RUNNING_HEADER, 0);
    let sheet = song
        .stylesheet
        .as_ref()
        .expect("the file has a style sheet");
    assert!(sheet.entries.len() > 500, "{} entries", sheet.entries.len());
    // "3 3 3 3": lines start at bars 4, 7 and 10.
    assert_eq!(
        breaks(&song),
        [
            (3, LineBreak::Break),
            (6, LineBreak::Break),
            (9, LineBreak::Break)
        ]
    );
}

#[test]
fn the_style_sheet_and_the_line_breaks_survive_saving() {
    let song = read(ORNAMENTS);
    let saved = song.write_gp().expect("writes");
    let back = read(&saved);
    assert_eq!(
        back.stylesheet, song.stylesheet,
        "every style entry is kept"
    );
    assert_eq!(back.page_setup, song.page_setup);
    assert_eq!(breaks(&back), breaks(&song));
}

#[test]
fn a_changed_page_setup_and_page_breaks_are_saved() {
    let mut song = read(ORNAMENTS);
    song.page_setup.page_size.x = 297;
    song.page_setup.page_size.y = 210;
    song.page_setup.page_margin.left = 25;
    song.page_setup.header_and_footer |= HEADER_FOOTER_RUNNING_HEADER;
    song.tracks[0].measures[6].line_break = LineBreak::Page;
    song.tracks[0].bars_per_system = Some(2);
    song.tracks[0].measures[9].line_break = LineBreak::None;
    let back = read(&song.write_gp().expect("writes"));
    assert!(
        back.page_setup.same_layout(&song.page_setup),
        "{:?}",
        back.page_setup
    );
    assert_eq!(
        breaks(&back),
        [(3, LineBreak::Break), (6, LineBreak::Page)],
        "the line after bar 9 is free again"
    );
    assert_eq!(back.tracks[0].bars_per_system, Some(2));
}

#[test]
fn a_song_without_breaks_or_page_setup_writes_neither() {
    let mut song = read(ORNAMENTS);
    song.stylesheet = None;
    song.page_setup = crate::model::page::PageSetup::default();
    for measure in &mut song.tracks[0].measures {
        measure.line_break = LineBreak::None;
    }
    let saved = song.write_gp().expect("writes");
    let back = read(&saved);
    assert_eq!(back.stylesheet, None);
    assert!(breaks(&back).is_empty());
}
