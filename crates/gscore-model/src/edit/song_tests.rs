//! Tests for the song-wide edits: one text field and the tempo.

use crate::differences;
use crate::interop::from_gp;
use crate::metadata::{MAX_SONG_TEXT_CHARS, MAX_TEMPO, SONG_TEXT_FIELDS, SongTextField};
use crate::song::Song;
use crate::test_support::one_measure;

fn song() -> Song {
    from_gp(one_measure(vec![])).unwrap()
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(
        differences(before, song).is_empty(),
        "{:?}",
        differences(before, song)
    );
}

#[test]
fn every_text_field_changes_and_undo_restores_it() {
    let mut song = song();
    let before = song.clone();
    for field in SONG_TEXT_FIELDS {
        let text = format!("{} value – ünïcode", field.id());
        let change = song.set_text(*field, &text).unwrap();
        assert_eq!(song.info(), {
            let mut info = before.info();
            field.set(&mut info, &text);
            info
        });
        song.revert(&change).unwrap();
        assert_unchanged(&before, &song);
    }
}

#[test]
fn notes_keep_their_lines() {
    let mut song = song();
    let before = song.clone();
    let change = song
        .set_text(SongTextField::Notes, "first\n\nthird\n")
        .unwrap();
    assert_eq!(song.info().notes, ["first", "", "third", ""]);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(song.info().notes.is_empty());
}

#[test]
fn the_tempo_changes_and_undo_restores_it() {
    let mut song = song();
    let before = song.clone();
    let change = song.set_tempo(150).unwrap();
    assert_eq!(song.tempo(), 150);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(song.set_tempo(0).is_err());
    assert!(song.set_tempo(MAX_TEMPO.saturating_add(1)).is_err());
    assert_unchanged(&before, &song);
}

#[test]
fn the_tempo_also_changes_measure_one_when_it_has_its_own() {
    // GP6 and later store the initial tempo as measure 1's tempo too; playback takes the
    // measure's, so both must change together.
    let mut gp = one_measure(vec![]);
    gp.measure_headers[0].tempo = 120;
    let mut song = from_gp(gp).unwrap();
    let first = crate::position::MeasureIndex::new(0);
    let change = song.set_tempo(90).unwrap();
    assert_eq!(song.tempo(), 90);
    assert_eq!(song.tempo_change(first), Some(90));
    song.revert(&change).unwrap();
    assert_eq!(song.tempo_change(first), Some(120));

    // Without a tempo of its own, measure 1 keeps none.
    let mut song = song_with_no_first_tempo();
    let _change = song.set_tempo(90).unwrap();
    assert_eq!(song.tempo_change(first), None);
}

fn song_with_no_first_tempo() -> Song {
    let mut gp = one_measure(vec![]);
    gp.measure_headers[0].tempo = 0;
    from_gp(gp).unwrap()
}

#[test]
fn text_that_is_too_long_is_refused_and_changes_nothing() {
    let mut song = song();
    let before = song.clone();
    let long = "x".repeat(MAX_SONG_TEXT_CHARS.saturating_add(1));
    assert!(song.set_text(SongTextField::Title, &long).is_err());
    assert_unchanged(&before, &song);
}
