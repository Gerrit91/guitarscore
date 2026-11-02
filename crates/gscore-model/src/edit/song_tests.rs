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
fn text_that_is_too_long_is_refused_and_changes_nothing() {
    let mut song = song();
    let before = song.clone();
    let long = "x".repeat(MAX_SONG_TEXT_CHARS.saturating_add(1));
    assert!(song.set_text(SongTextField::Title, &long).is_err());
    assert_unchanged(&before, &song);
}
