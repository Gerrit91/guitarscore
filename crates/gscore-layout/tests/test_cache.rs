//! The layout cache gives exactly the layout a fresh layout gives, after every kind of
//! edit, and lays out again only the measures that changed.

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

use gscore_layout::{Fonts, LayoutCache, LayoutOptions, layout_track};
use gscore_model::{
    BeatIndex, BeatKind, BeatPosition, Change, Fret, MeasureIndex, NoteValue, Song, StringNumber,
    TrackIndex, VoiceIndex,
};

fn first_beat(song: &Song, measure: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    }
    .clamped(song)
}

trait Clamp {
    fn clamped(self, song: &Song) -> Self;
}

impl Clamp for BeatPosition {
    /// The same position, or `beat 0` if the measure has no such beat.
    fn clamped(mut self, song: &Song) -> Self {
        let beats = song
            .track(self.track)
            .and_then(|t| t.measure(self.measure))
            .and_then(|m| m.voice(self.voice))
            .map_or(0, |v| v.beats().len());
        if self.beat.get() >= beats {
            self.beat = BeatIndex::new(0);
        }
        self
    }
}

/// One edit, chosen by `step` (deterministic): a note, a length, a beat or a measure.
fn edit(song: &mut Song, step: usize) -> Option<Change> {
    let measures = song.measure_count();
    let measure = (step * 7) % measures;
    let at = first_beat(song, measure);
    let is_real = song.beat(at).is_some_and(|b| b.kind() != BeatKind::Empty);
    match step % 5 {
        0 if is_real => song
            .set_note(
                at,
                StringNumber::new(1).unwrap(),
                Fret::new(u8::try_from(step % 20).unwrap()).unwrap(),
            )
            .ok(),
        1 if is_real => song
            .set_duration(at, NoteValue::new(8, 0, None).unwrap())
            .ok(),
        2 => song
            .insert_beat(at, NoteValue::new(16, 0, None).unwrap())
            .ok(),
        3 => song.insert_measure(MeasureIndex::new(measure)).ok(),
        4 if measures > 2 => song.delete_measure(MeasureIndex::new(measure)).ok(),
        _ => None,
    }
}

#[test]
fn the_cache_matches_a_fresh_layout_after_every_edit_and_undo() {
    let fonts = Fonts::bundled().unwrap();
    let tab_only = LayoutOptions {
        notation: false,
        ..LayoutOptions::default()
    };
    for options in [LayoutOptions::default(), tab_only] {
        for file in [
            "basic/note_lengths.gp",
            "basic/ornaments.gp",
            "basic/steel_guitar_dadgad.gp5",
        ] {
            let mut song = common::open(file);
            let track = TrackIndex::new(0);
            let mut cache = LayoutCache::default();
            let mut changes = Vec::new();
            for step in 0..40 {
                if let Some(change) = edit(&mut song, step) {
                    changes.push(change);
                }
                let cached = cache.layout(&song, track, &options, &fonts).unwrap();
                let fresh = layout_track(&song, track, &options, &fonts).unwrap();
                assert!(
                    cached == fresh,
                    "{file}, step {step}: the cached layout differs"
                );
            }
            assert!(
                changes.len() >= 20,
                "{file}: only {} edits applied",
                changes.len()
            );
            while let Some(change) = changes.pop() {
                song.revert(&change).unwrap();
                let cached = cache.layout(&song, track, &options, &fonts).unwrap();
                assert!(cached == layout_track(&song, track, &options, &fonts).unwrap());
            }
        }
    }
}

#[test]
fn the_cache_follows_a_key_signature_change() {
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let mut song = common::open("basic/note_lengths.gp");
    let track = TrackIndex::new(0);
    let mut cache = LayoutCache::default();
    let _ = cache.layout(&song, track, &options, &fonts).unwrap();

    let _change = song
        .set_key_signature(MeasureIndex::new(4), 3, false)
        .unwrap();
    let cached = cache.layout(&song, track, &options, &fonts).unwrap();
    let fresh = layout_track(&song, track, &options, &fonts).unwrap();
    assert_eq!(cached, fresh, "a key change must not leave stale measures");
    assert!(
        cache.laid_out() > 0,
        "the affected measures are laid out again"
    );
}

#[test]
fn only_changed_measures_are_laid_out_again() {
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let mut song = common::open("basic/note_lengths.gp");
    let track = TrackIndex::new(0);
    let measures = song.measure_count();
    let mut cache = LayoutCache::default();

    let _ = cache.layout(&song, track, &options, &fonts).unwrap();
    assert_eq!(
        cache.laid_out(),
        measures,
        "the first call lays out everything"
    );
    let _ = cache.layout(&song, track, &options, &fonts).unwrap();
    assert_eq!(cache.laid_out(), 0, "nothing changed");

    let at = first_beat(&song, 3);
    let _change = song
        .set_note(at, StringNumber::new(2).unwrap(), Fret::new(7).unwrap())
        .unwrap();
    let _ = cache.layout(&song, track, &options, &fonts).unwrap();
    assert_eq!(cache.laid_out(), 1, "one note: one measure");

    // An inserted measure is the only new one; the measures after it moved but are reused.
    let _change = song.insert_measure(MeasureIndex::new(2)).unwrap();
    let _ = cache.layout(&song, track, &options, &fonts).unwrap();
    assert_eq!(cache.laid_out(), 1);

    // A wider page only re-breaks the lines; options that change measures, or another
    // track, start afresh.
    let mut wider = options.clone();
    wider.page_width += 10.0;
    let _ = cache.layout(&song, track, &wider, &fonts).unwrap();
    assert_eq!(
        cache.laid_out(),
        0,
        "the page width is not part of a measure"
    );
    let mut bigger = wider.clone();
    bigger.fret_size += 0.2;
    let _ = cache.layout(&song, track, &bigger, &fonts).unwrap();
    assert_eq!(cache.laid_out(), song.measure_count());
    assert!(
        cache
            .layout(&song, TrackIndex::new(5), &bigger, &fonts)
            .is_err()
    );
}
