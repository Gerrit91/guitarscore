//! Tests for the page edits: the page setup, line and page breaks, and the bars per line.
//! Each edit changes only what it says, refuses bad values, and undoes exactly.

use crate::interop::from_gp;
use crate::page::{Margins, PageSetup, PaperSize, SystemBreak};
use crate::position::{MeasureIndex, TrackIndex};
use crate::test_support::{GUITAR_6, measure, song as test_song, track};
use crate::{Song, differences, settings_differences};

fn song(measures: usize) -> Song {
    let bars = (0..measures)
        .map(|_| measure((4, 4), vec![vec![]]))
        .collect();
    from_gp(test_song(vec![track("Guitar", GUITAR_6, bars)])).unwrap()
}

fn assert_unchanged(before: &Song, song: &Song) {
    let mut found = differences(before, song);
    found.extend(settings_differences(before, song));
    assert!(found.is_empty(), "{found:?}");
}

fn breaks(song: &Song) -> Vec<SystemBreak> {
    song.track(TrackIndex::new(0))
        .unwrap()
        .measures()
        .map(|measure| measure.system_break())
        .collect()
}

#[test]
fn the_page_setup_is_set_checked_and_undone() {
    let mut song = song(2);
    let before = song.clone();
    assert_eq!(song.page_setup(), PageSetup::default());
    let letter = PageSetup {
        width_mm: 279,
        height_mm: 216,
        margins: Margins {
            top: 20,
            right: 12,
            bottom: 18,
            left: 12,
        },
        scale_percent: 80,
        page_numbers: false,
        copyright: false,
        running_header: true,
    };
    let change = song.set_page_setup(letter).unwrap();
    assert_eq!(song.page_setup(), letter);
    assert!(letter.is_landscape());
    assert_eq!(PaperSize::of(279, 216), Some(PaperSize::Letter));
    assert!(
        !settings_differences(&before, &song).is_empty(),
        "the comparison sees it"
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);

    for bad in [
        PageSetup {
            width_mm: 10,
            ..PageSetup::default()
        },
        PageSetup {
            height_mm: 5000,
            ..PageSetup::default()
        },
        PageSetup {
            scale_percent: 0,
            ..PageSetup::default()
        },
        PageSetup {
            scale_percent: 300,
            ..PageSetup::default()
        },
        PageSetup {
            margins: Margins {
                left: 100,
                right: 90,
                ..PageSetup::default().margins
            },
            ..PageSetup::default()
        },
        PageSetup {
            margins: Margins {
                top: 250,
                ..PageSetup::default().margins
            },
            ..PageSetup::default()
        },
    ] {
        assert!(song.set_page_setup(bad).is_err(), "{bad:?}");
        assert_unchanged(&before, &song);
    }
}

#[test]
fn odd_page_setups_from_files_are_moved_into_range() {
    let mut gp = test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![]])],
    )]);
    gp.page_setup.page_size.x = 0;
    gp.page_setup.page_size.y = 9000;
    gp.page_setup.page_margin.left = 400;
    gp.page_setup.score_size_proportion = f32::INFINITY;
    let setup = from_gp(gp).unwrap().page_setup();
    setup.check().unwrap();
    assert_eq!((setup.width_mm, setup.height_mm), (70, 1000));
    assert_eq!(setup.margins, PageSetup::default().margins);
    assert_eq!(setup.scale_percent, 100);
}

#[test]
fn line_and_page_breaks_are_set_and_undone() {
    let mut song = song(4);
    let before = song.clone();
    let track = TrackIndex::new(0);
    let line = song
        .set_system_break(track, MeasureIndex::new(2), SystemBreak::Line)
        .unwrap();
    let page = song
        .set_system_break(track, MeasureIndex::new(3), SystemBreak::Page)
        .unwrap();
    assert_eq!(
        breaks(&song),
        [
            SystemBreak::None,
            SystemBreak::None,
            SystemBreak::Line,
            SystemBreak::Page
        ]
    );
    // The first measure starts the score; no break there.
    assert!(
        song.set_system_break(track, MeasureIndex::new(0), SystemBreak::Line)
            .is_err()
    );
    assert!(
        song.set_system_break(track, MeasureIndex::new(9), SystemBreak::Line)
            .is_err()
    );
    assert!(
        song.set_system_break(TrackIndex::new(5), MeasureIndex::new(1), SystemBreak::Line)
            .is_err()
    );
    song.revert(&page).unwrap();
    song.revert(&line).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn bars_per_line_are_limited_and_undone() {
    let mut song = song(1);
    let before = song.clone();
    let track = TrackIndex::new(0);
    let change = song.set_bars_per_system(track, Some(4)).unwrap();
    assert_eq!(song.track(track).unwrap().bars_per_system(), Some(4));
    for bad in [Some(0), Some(33)] {
        assert!(song.set_bars_per_system(track, bad).is_err());
    }
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}
