//! Separate pages: systems flow onto A4 pages stacked with a gap, never across a page's
//! bottom margin, with "Page n/N" on each; the endless page stays one page.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{DrawOp, Fonts, LayoutCache, LayoutOptions, PAGE_GAP, Rect, layout_track};
use gscore_model::TrackIndex;
use gscore_model::testing::{self, GUITAR_6};

fn paged() -> LayoutOptions {
    LayoutOptions::default().on_pages()
}

fn page_of(pages: &[Rect], y: f32) -> Option<usize> {
    pages
        .iter()
        .position(|page| y >= page.origin.y && y <= page.origin.y + page.height)
}

fn page_numbers(ops: &[DrawOp]) -> Vec<String> {
    ops.iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } if text.starts_with("Page ") => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_long_song_flows_onto_pages_that_keep_their_margins() {
    let song = testing::song(&[GUITAR_6], 200).unwrap();
    let fonts = Fonts::bundled().unwrap();
    let options = paged();
    let layout = layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap();
    let page_height = options.page_height.unwrap();
    assert!(layout.pages.len() > 1, "{} pages", layout.pages.len());
    // Pages stack from the top with a gap; the layout is exactly as tall as all of them.
    for (index, page) in layout.pages.iter().enumerate() {
        let expected = index as f32 * (page_height + PAGE_GAP);
        assert!(
            (page.origin.y - expected).abs() < 1e-3,
            "page {index} at {}",
            page.origin.y
        );
        assert!((page.height - page_height).abs() < 1e-3);
        assert!((page.width - options.page_width).abs() < 1e-3);
    }
    let last = layout.pages.last().unwrap();
    assert!((layout.height - (last.origin.y + last.height)).abs() < 1e-3);
    // Every system lies inside one page, clear of the bottom margin.
    for system in &layout.places.systems {
        let page = page_of(&layout.pages, system.top)
            .unwrap_or_else(|| panic!("system at {} is between pages", system.top));
        let page = layout.pages[page];
        let bottom = system.tab_top + 5.0 * system.string_spacing;
        assert!(
            bottom <= page.origin.y + page.height - options.margin_bottom,
            "a system ends at {bottom}, below the margin of the page at {}",
            page.origin.y
        );
    }
    // Every page has systems, and every page its number.
    for index in 0..layout.pages.len() {
        assert!(
            layout
                .places
                .systems
                .iter()
                .any(|system| page_of(&layout.pages, system.top) == Some(index)),
            "page {index} is empty"
        );
    }
    let count = layout.pages.len();
    let expected: Vec<String> = (1..=count).map(|n| format!("Page {n}/{count}")).collect();
    assert_eq!(page_numbers(&layout.ops), expected);
}

#[test]
fn the_endless_page_is_one_page_without_numbers() {
    let song = testing::song(&[GUITAR_6], 200).unwrap();
    let layout = common::layout(&song, 0);
    assert_eq!(layout.pages.len(), 1);
    assert!((layout.pages[0].height - layout.height).abs() < 1e-3);
    assert!(page_numbers(&layout.ops).is_empty());
}

#[test]
fn a_short_song_fills_one_whole_page() {
    let song = testing::song(&[GUITAR_6], 2).unwrap();
    let fonts = Fonts::bundled().unwrap();
    let options = paged();
    let layout = layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap();
    assert_eq!(layout.pages.len(), 1);
    assert!((layout.height - options.page_height.unwrap()).abs() < 1e-3);
    assert_eq!(page_numbers(&layout.ops), ["Page 1/1"]);
}

#[test]
fn switching_to_pages_keeps_the_measure_cache() {
    let song = testing::song(&[GUITAR_6], 40).unwrap();
    let fonts = Fonts::bundled().unwrap();
    let mut cache = LayoutCache::default();
    let endless = cache
        .layout(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts)
        .unwrap();
    assert_eq!(cache.laid_out(), 40);
    let on_pages = cache
        .layout(&song, TrackIndex::new(0), &paged(), &fonts)
        .unwrap();
    assert_eq!(cache.laid_out(), 0, "no measure is laid out again");
    // The same systems, only placed on pages.
    assert_eq!(endless.places.systems.len(), on_pages.places.systems.len());
    assert_eq!(
        endless.places.measures.len(),
        on_pages.places.measures.len()
    );
    // Another paper, margins or score size only re-breaks lines (the page setup preview
    // relies on it).
    let landscape = gscore_model::PageSetup {
        margins: gscore_model::Margins {
            left: 25,
            ..gscore_model::PageSetup::default().margins
        },
        scale_percent: 80,
        ..gscore_model::PageSetup::default().turned()
    };
    let turned = cache
        .layout(
            &song,
            TrackIndex::new(0),
            &LayoutOptions::default()
                .with_page_setup(&landscape)
                .on_pages(),
            &fonts,
        )
        .unwrap();
    assert_eq!(cache.laid_out(), 0, "no measure is laid out again");
    assert!(
        turned.places.systems.len() < on_pages.places.systems.len(),
        "wider lines"
    );
}

#[test]
fn odd_page_heights_fall_back_to_the_endless_page() {
    let song = testing::song(&[GUITAR_6], 20).unwrap();
    let fonts = Fonts::bundled().unwrap();
    for height in [0.0, -5.0, f32::NAN, f32::INFINITY, 1.0] {
        let options = LayoutOptions {
            page_height: Some(height),
            ..LayoutOptions::default()
        };
        let layout = layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap();
        assert_eq!(layout.pages.len(), 1, "page height {height}");
        assert!(layout.height.is_finite());
    }
}

#[test]
fn a_tiny_page_still_takes_one_system_each() {
    // A page that fits less than one system: each system gets its own page, none is lost.
    let song = testing::song(&[GUITAR_6], 60).unwrap();
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions {
        page_height: Some(
            LayoutOptions::default().margin_top + LayoutOptions::default().margin_bottom + 1.0,
        ),
        ..LayoutOptions::default()
    };
    let layout = layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap();
    assert_eq!(layout.pages.len(), layout.places.systems.len());
}
