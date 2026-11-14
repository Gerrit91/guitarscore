//! Transposing what the song sounds like: the written notes stay, playback moves, it is
//! stored the way Guitar Pro stores it, it stops at an octave, and undo puts every track
//! back.

use crate::edit::MAX_TRANSPOSE;
use crate::interop::from_gp;
use crate::position::{BeatIndex, MeasureIndex, TrackIndex};
use crate::test_support::{GUITAR_6, beat, measure, rest, song as test_song, track};
use crate::*;

/// Two tracks (a guitar with one note, a drum kit) and two measures.
fn two_tracks() -> Song {
    let mut tracks = vec![
        track(
            "Guitar",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![beat(1, &[(1, 3)])]]),
                measure((4, 4), vec![vec![beat(1, &[(2, 5)])]]),
            ],
        ),
        track(
            "Drums",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![rest(1)]]),
                measure((4, 4), vec![vec![rest(1)]]),
            ],
        ),
    ];
    // A drum kit: its "frets" are the drum sounds, which are not pitched.
    if let Some(drums) = tracks.get_mut(1) {
        drums.percussion_track = true;
    }
    from_gp(test_song(tracks)).unwrap()
}

/// The guitar's first note.
fn first_note(song: &Song) -> NoteView<'_> {
    song.track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(0))
        .unwrap()
        .voice(VoiceIndex::new(0))
        .unwrap()
        .beat(BeatIndex::new(0))
        .unwrap()
        .notes()
        .next()
        .unwrap()
}

#[test]
fn transposing_changes_what_plays_and_not_what_is_written() {
    let mut song = two_tracks();
    let before = song.clone();
    let written = song
        .track(TrackIndex::new(0))
        .unwrap()
        .written_pitch(&first_note(&song))
        .unwrap();
    let sounding = song
        .track(TrackIndex::new(0))
        .unwrap()
        .sounding_pitch(&first_note(&song))
        .unwrap();
    assert_eq!(written, sounding, "nothing is transposed yet");

    let change = song.transpose_by(12).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transpose(), 12);
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .written_pitch(&first_note(&song))
            .unwrap(),
        written,
        "the fret numbers and the score stay"
    );
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .sounding_pitch(&first_note(&song))
            .unwrap()
            .get(),
        sounding.get().saturating_add(12),
        "it plays an octave higher"
    );

    song.revert(&change).unwrap();
    assert!(differences(&before, &song).is_empty());
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transpose(), 0);
}

#[test]
fn transposing_leaves_the_files_own_transpose_alone() {
    // Guitar Pro's `<Transpose>` is the instrument's notation octave (a guitar's music is
    // written an octave up); GuitarScore's transposition is its own setting and does not
    // touch it, so the file's notes and the tablature keep playing as they did.
    let mut song = two_tracks();
    song.inner_mut().tracks[0].transpose_octave = -1;
    let _ = song.transpose_by(12).unwrap();
    assert_eq!(song.inner().tracks[0].transpose_octave, -1);
    assert_eq!(song.inner().tracks[0].transpose_chromatic, 0);
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transposition(), 12);
}

#[test]
fn two_semitone_steps_add_up() {
    let mut song = two_tracks();
    let _ = song.transpose_by(7).unwrap();
    let _ = song.transpose_by(3).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transpose(), 10);
    let _ = song.transpose_by(-4).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transpose(), 6);
}

#[test]
fn a_drum_track_keeps_its_sounds() {
    let mut song = two_tracks();
    let _ = song.transpose_by(12).unwrap();
    assert_eq!(song.track(TrackIndex::new(1)).unwrap().transpose(), 0);
}

#[test]
fn transposing_stops_at_an_octave_and_changes_nothing_then() {
    let mut song = two_tracks();
    let _ = song.transpose_by(MAX_TRANSPOSE).unwrap();
    assert!(!song.can_transpose(1));
    let before = song.clone();
    assert!(song.transpose_by(1).is_err());
    assert!(differences(&before, &song).is_empty());
    // Back down is possible again.
    assert!(song.can_transpose(-1));
    let _ = song.transpose_by(-MAX_TRANSPOSE).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transpose(), 0);
}

#[test]
fn an_out_of_range_step_is_refused() {
    let mut song = two_tracks();
    let before = song.clone();
    for step in [0, MAX_TRANSPOSE + 1, -MAX_TRANSPOSE - 1, i32::MIN, i32::MAX] {
        assert!(song.transpose_by(step).is_err(), "{step}");
    }
    assert!(differences(&before, &song).is_empty());
}

#[test]
fn the_switch_turns_the_sound_back_to_the_written_pitch() {
    let mut song = two_tracks();
    let _ = song.transpose_by(12).unwrap();
    let sounding = |song: &Song| {
        song.track(TrackIndex::new(0))
            .unwrap()
            .sounding_pitch(&first_note(song))
            .unwrap()
            .get()
    };
    let written = |song: &Song| {
        song.track(TrackIndex::new(0))
            .unwrap()
            .written_pitch(&first_note(song))
            .unwrap()
            .get()
    };
    assert_eq!(sounding(&song), written(&song).saturating_add(12));
    let off = song.set_transpose_active(false).unwrap();
    assert!(!song.transpose_active());
    assert_eq!(sounding(&song), written(&song), "as written again");
    assert_eq!(
        song.track(TrackIndex::new(0)).unwrap().transposition(),
        12,
        "the value is kept"
    );
    let on = song.set_transpose_active(true).unwrap();
    assert_eq!(sounding(&song), written(&song).saturating_add(12));
    song.revert(&on).unwrap();
    song.revert(&off).unwrap();
    assert!(song.transpose_active());
    assert!(song.set_transpose_active(true).is_err(), "already on");
}

#[test]
fn moving_the_notes_moves_the_frets_and_undo_puts_them_back() {
    let mut song = two_tracks();
    let before = song.clone();
    let written = song
        .track(TrackIndex::new(0))
        .unwrap()
        .written_pitch(&first_note(&song))
        .unwrap();
    let change = song.transpose_notes(None, 12).unwrap();
    let note = first_note(&song);
    assert_eq!(
        note.string().unwrap().get(),
        1,
        "its own string still holds it"
    );
    assert_eq!(note.fret().unwrap().get(), 15, "three frets up an octave");
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .written_pitch(&note)
            .unwrap()
            .get(),
        written.get().saturating_add(12)
    );
    song.revert(&change).unwrap();
    assert!(differences(&before, &song).is_empty());
}

#[test]
fn a_note_moves_to_a_lower_string_when_its_own_cannot_hold_it() {
    // String 1, fret 2 (F#4) an octave down is F#3: not on string 1, but on string 4 fret 4.
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(1, &[(1, 2)])]])],
    )]))
    .unwrap();
    let _ = song.transpose_notes(None, -12).unwrap();
    let note = first_note(&song);
    assert_eq!(note.string().unwrap().get(), 4);
    assert_eq!(note.fret().unwrap().get(), 4);
}

#[test]
fn a_note_that_would_leave_the_strings_stops_the_edit() {
    // The lowest string's open note cannot go an octave down on a guitar.
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(1, &[(6, 0)])]])],
    )]))
    .unwrap();
    let before = song.clone();
    assert!(song.transpose_notes(None, -12).is_err());
    assert!(differences(&before, &song).is_empty());
}

#[test]
fn one_track_can_be_transposed_on_its_own() {
    let mut song = two_tracks();
    let change = song.transpose_track_by(TrackIndex::new(0), 12).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().transposition(), 12);
    assert_eq!(song.track(TrackIndex::new(1)).unwrap().transposition(), 0);
    // The drum kit has no pitch, so it does not count as a differing track.
    assert_eq!(song.transposition(), Some(12));
    song.revert(&change).unwrap();
    assert_eq!(song.transposition(), Some(0));
}

#[test]
fn the_notes_of_one_track_move_on_their_own() {
    let mut song = from_gp(test_song(vec![
        track(
            "Guitar",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[(1, 0)])]])],
        ),
        track(
            "Bass",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[(1, 0)])]])],
        ),
    ]))
    .unwrap();
    let _ = song.transpose_notes(Some(TrackIndex::new(0)), 12).unwrap();
    let fret = |track: usize| {
        song.track(TrackIndex::new(track))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .next()
            .unwrap()
            .beats()
            .next()
            .unwrap()
            .notes()
            .next()
            .unwrap()
            .fret()
            .unwrap()
            .get()
    };
    assert_eq!((fret(0), fret(1)), (12, 0));
}

#[test]
fn a_drum_kit_is_not_moved() {
    let mut song = two_tracks();
    let drums = TrackIndex::new(1);
    assert!(!song.can_transpose_notes(Some(drums), 12));
    assert!(song.transpose_notes(Some(drums), 12).is_err());
    // Moving the whole song leaves the kit's "frets" (the drum sounds) alone.
    let _ = song.transpose_notes(None, 12).unwrap();
    assert_eq!(song.track(drums).unwrap().transposition(), 0);
}
