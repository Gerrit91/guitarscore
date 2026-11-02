//! Tests for the track settings M15 adds: program, drum kit, capo, string count and the
//! tuning of one string. Each edit must change only what it says and undo exactly.

use crate::interop::from_gp;
use crate::position::{Pitch, StringNumber, TrackIndex};
use crate::test_support::{
    GUITAR_6, GUITAR_7, beat, measure, one_measure, song as test_song, track,
};
use crate::{Song, differences};

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

fn pitches(song: &Song) -> Vec<u8> {
    song.track(TrackIndex::new(0))
        .unwrap()
        .tuning()
        .unwrap()
        .unwrap_or_default()
        .iter()
        .map(|string| string.pitch.get())
        .collect()
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(
        differences(before, song).is_empty(),
        "{:?}",
        differences(before, song)
    );
}

#[test]
fn setting_the_program_changes_the_sound_and_undo_restores_it() {
    let mut song = from_gp(one_measure(vec![])).unwrap();
    let before = song.clone();
    let previous = song.track_sound(TrackIndex::new(0)).unwrap().program;

    let change = song.set_track_program(TrackIndex::new(0), 29).unwrap();
    assert_eq!(song.track_sound(TrackIndex::new(0)).unwrap().program, 29);
    assert_ne!(previous, 29);

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(song.set_track_program(TrackIndex::new(0), 128).is_err());
    assert_unchanged(&before, &song);
}

#[test]
fn a_track_becomes_a_drum_kit_and_back() {
    let mut gp = one_measure(vec![]);
    gp.tracks[0].instrument_type = Some("electricGuitar".to_owned());
    gp.tracks[0].instrument_name = Some("Electric Guitar".to_owned());
    let mut song = from_gp(gp).unwrap();
    let before = song.clone();

    let change = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let track = song.track(TrackIndex::new(0)).unwrap();
    assert!(track.is_percussion());
    assert_eq!(track.instrument_kind(), crate::InstrumentKind::Drums);
    assert!(song.track_sound(TrackIndex::new(0)).unwrap().percussion);

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);

    // A track that is already a drum kit loses the kit when it stops being one.
    let _ = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let change = song.set_track_drums(TrackIndex::new(0), false).unwrap();
    let track = song.track(TrackIndex::new(0)).unwrap();
    assert!(!track.is_percussion());
    assert_eq!(track.instrument_kind(), crate::InstrumentKind::Strings);
    song.revert(&change).unwrap();
    assert!(song.track(TrackIndex::new(0)).unwrap().is_percussion());
}

#[test]
fn the_capo_moves_the_sounding_pitch_and_undo_restores_it() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(6, 3)])])).unwrap();
    let before = song.clone();
    let pitch = |song: &Song| {
        let track = song.track(TrackIndex::new(0)).unwrap();
        let beat = song
            .beat(crate::BeatPosition {
                track: TrackIndex::new(0),
                measure: crate::MeasureIndex::new(0),
                voice: crate::VoiceIndex::new(0),
                beat: crate::BeatIndex::new(0),
            })
            .unwrap();
        track
            .sounding_pitch(&beat.notes().next().unwrap())
            .unwrap()
            .get()
    };
    assert_eq!(pitch(&song), 43);

    let change = song.set_track_capo(TrackIndex::new(0), 2).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().capo().unwrap(), 2);
    assert_eq!(pitch(&song), 45);

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn growing_a_track_adds_lower_strings_in_fourths() {
    let mut song = from_gp(one_measure(vec![])).unwrap();
    let before = song.clone();

    let change = song.set_track_strings(TrackIndex::new(0), 7).unwrap();
    assert_eq!(
        pitches(&song),
        GUITAR_7
            .iter()
            .map(|&p| u8::try_from(p).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().string_count(), 7);

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn removing_strings_with_notes_on_them_is_refused_but_undo_works() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(6, 3)])])).unwrap();
    let before = song.clone();
    assert!(song.check_track_strings(TrackIndex::new(0), 5).is_err());
    assert!(song.set_track_strings(TrackIndex::new(0), 5).is_err());
    assert_unchanged(&before, &song);

    // The same reduction is fine once the notes on the removed strings are gone.
    let cleared = song.clear_note(
        crate::BeatPosition {
            track: TrackIndex::new(0),
            measure: crate::MeasureIndex::new(0),
            voice: crate::VoiceIndex::new(0),
            beat: crate::BeatIndex::new(0),
        },
        string(6),
    );
    assert!(cleared.is_ok());
    let change = song.set_track_strings(TrackIndex::new(0), 5).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().string_count(), 5);
    assert_eq!(
        pitches(&song),
        GUITAR_6[..5]
            .iter()
            .map(|&p| u8::try_from(p).unwrap())
            .collect::<Vec<_>>()
    );
    song.revert(&change).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().string_count(), 6);
}

#[test]
fn tuning_one_string_changes_only_that_string() {
    let mut gp = test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![])],
    )]);
    gp.tracks[0].tuning_label = Some("E standard".to_owned());
    let mut song = from_gp(gp).unwrap();
    let before = song.clone();

    let change = song
        .set_track_string_pitch(TrackIndex::new(0), string(6), Pitch::new(38).unwrap())
        .unwrap();
    assert_eq!(
        pitches(&song),
        [64, 59, 55, 50, 45, 38],
        "only string 6 changes"
    );
    // The old name no longer describes the tuning.
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .tuning_name()
            .unwrap(),
        Some("Drop D".to_owned())
    );

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .tuning_name()
            .unwrap()
            .as_deref(),
        Some("E standard")
    );
    assert!(
        song.set_track_string_pitch(TrackIndex::new(0), string(9), Pitch::new(40).unwrap())
            .is_err()
    );
}

#[test]
fn track_settings_of_a_missing_track_are_errors() {
    let mut song = from_gp(one_measure(vec![])).unwrap();
    let before = song.clone();
    let missing = TrackIndex::new(9);
    assert!(song.set_track_program(missing, 0).is_err());
    assert!(song.set_track_drums(missing, true).is_err());
    assert!(song.set_track_capo(missing, 1).is_err());
    assert!(song.set_track_strings(missing, 7).is_err());
    assert!(
        song.set_track_string_pitch(missing, string(1), Pitch::new(40).unwrap())
            .is_err()
    );
    assert_unchanged(&before, &song);
}
