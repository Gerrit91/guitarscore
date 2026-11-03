//! PDF export: one PDF page per layout page at the paper's size, the fonts embedded as
//! subsets with a Unicode map, the title in the document information; bad input is an
//! error, never a panic.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{Fonts, Layout, LayoutOptions, PdfFonts, Theme, layout_track, to_pdf};
use gscore_model::TrackIndex;
use gscore_model::testing::{self, GUITAR_6};

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn paged(song: &gscore_model::Song) -> (Layout, LayoutOptions) {
    let options = LayoutOptions::default()
        .with_page_setup(&song.page_setup())
        .on_pages();
    let fonts = Fonts::bundled().unwrap();
    (
        layout_track(song, TrackIndex::new(0), &options, &fonts).unwrap(),
        options,
    )
}

#[test]
fn every_page_becomes_an_a4_pdf_page_with_embedded_fonts() {
    let song = common::open("basic/ornaments.gp");
    let mut long = testing::song(&[GUITAR_6], 120).unwrap();
    long = common::with_notes(long, &[(0, 0, 1, 12), (3, 0, 2, 5)]);
    for (song, what) in [(song, "ornaments"), (long, "120 measures")] {
        let (layout, options) = paged(&song);
        let pdf = to_pdf(
            &layout,
            &Theme::LIGHT,
            options.staff_space_mm,
            &PdfFonts::bundled(),
            "Basic Ornaments",
        )
        .unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "{what}");
        assert_eq!(
            count(&pdf, b"/Type /Page\n") + count(&pdf, b"/Type /Page "),
            layout.pages.len(),
            "{what}: one PDF page per layout page"
        );
        // A4 is 595.28 x 841.89 points.
        assert!(
            count(&pdf, b"/MediaBox [0 0 595.27") > 0,
            "{what}: A4 width"
        );
        let text_fonts = count(&pdf, b"/FontFile2");
        assert!(
            (1..=2).contains(&text_fonts),
            "{what}: Noto Sans (and bold)"
        );
        assert_eq!(count(&pdf, b"/FontFile3"), 1, "{what}: Bravura");
        assert_eq!(
            count(&pdf, b"/ToUnicode"),
            text_fonts + 1,
            "{what}: every font has a Unicode map"
        );
        assert!(count(&pdf, b"(Basic Ornaments)") > 0, "{what}: the title");
        // Subsets: far smaller than the 2.4 MB of font files.
        assert!(pdf.len() < 400_000, "{what}: {} bytes", pdf.len());
    }
}

#[test]
fn odd_input_is_an_error_not_a_panic() {
    let song = testing::song(&[GUITAR_6], 2).unwrap();
    let (layout, _) = paged(&song);
    let fonts = PdfFonts::bundled();
    for space in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            to_pdf(&layout, &Theme::LIGHT, space, &fonts, "").is_err(),
            "{space}"
        );
    }
    let broken = PdfFonts {
        music: &[1, 2, 3],
        ..fonts
    };
    assert!(to_pdf(&layout, &Theme::LIGHT, 1.75, &broken, "").is_err());
    let empty = Layout {
        pages: Vec::new(),
        ..layout
    };
    assert!(to_pdf(&empty, &Theme::LIGHT, 1.75, &fonts, "").is_err());
}

/// Writes the ornaments PDF to `GSCORE_PDF_OUT` when set, to look at it by hand.
#[test]
fn write_a_sample_when_asked() {
    let Some(path) = std::env::var_os("GSCORE_PDF_OUT") else {
        return;
    };
    let song = common::open("basic/ornaments.gp");
    let (layout, options) = paged(&song);
    let pdf = to_pdf(
        &layout,
        &Theme::LIGHT,
        options.staff_space_mm,
        &PdfFonts::bundled(),
        song.title(),
    )
    .unwrap();
    std::fs::write(path, pdf).unwrap();
}
