//! Tests for the checked editing methods: effects, undo, transactionality, measures.

use crate::duration::{Duration, NoteValue, TimeSignature};
use crate::interop::from_gp;
use crate::position::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
};
use crate::test_support::{
    BASS_4, GUITAR_6, beat, empty_beat, measure, one_measure, rest, song as test_song, track,
};
use crate::views::BeatKind;
use crate::*;

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

fn fret(value: u8) -> Fret {
    Fret::new(value).unwrap()
}

fn note(value: u16) -> NoteValue {
    NoteValue::new(value, 0, None).unwrap()
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(
        differences(before, song).is_empty(),
        "{:?}",
        differences(before, song)
    );
}

#[test]
fn setting_a_note_adds_it_and_undo_restores_the_song() {
    let mut song = from_gp(one_measure(vec![rest(4)])).unwrap();
    let before = song.clone();
    let change = song.set_note(at(0), string(1), fret(5)).unwrap();

    let beat = song.beat(at(0)).unwrap();
    assert_eq!(beat.kind(), BeatKind::Normal);
    let notes: Vec<_> = beat.notes().collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].string().unwrap(), string(1));
    assert_eq!(notes[0].fret().unwrap(), fret(5));

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn replacing_a_fret_keeps_the_note_effects_and_undo_restores_them() {
    let mut existing = beat(4, &[(1, 3)]);
    existing.notes.get_mut(0).unwrap().effect.palm_mute = true;
    let mut song = from_gp(one_measure(vec![existing])).unwrap();
    let before = song.clone();

    let change = song.set_note(at(0), string(1), fret(7)).unwrap();
    let notes: Vec<_> = song.beat(at(0)).unwrap().notes().collect();
    assert_eq!(notes[0].fret().unwrap(), fret(7));
    assert!(notes[0].is_palm_muted());

    song.revert(&change).unwrap();
    let notes: Vec<_> = song.beat(at(0)).unwrap().notes().collect();
    assert_eq!(notes[0].fret().unwrap(), fret(3));
    assert!(notes[0].is_palm_muted());
    assert_unchanged(&before, &song);
}

#[test]
fn a_missing_string_or_beat_is_an_error_and_changes_nothing() {
    let mut song = from_gp(one_measure(vec![rest(4)])).unwrap();
    let before = song.clone();
    assert!(song.set_note(at(0), string(9), fret(5)).is_err());
    assert_unchanged(&before, &song);
    assert!(song.set_note(at(9), string(1), fret(5)).is_err());
    assert_unchanged(&before, &song);
    assert!(song.clear_note(at(0), string(1)).is_err());
    assert_unchanged(&before, &song);
}

#[test]
fn clearing_the_last_note_makes_a_rest_and_undo_restores_the_note() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(1, 3)])])).unwrap();
    let before = song.clone();
    let change = song.clear_note(at(0), string(1)).unwrap();
    assert_eq!(song.beat(at(0)).unwrap().kind(), BeatKind::Rest);
    assert_eq!(song.beat(at(0)).unwrap().notes().count(), 0);

    song.revert(&change).unwrap();
    assert_eq!(song.beat(at(0)).unwrap().kind(), BeatKind::Normal);
    assert_eq!(song.beat(at(0)).unwrap().notes().count(), 1);
    assert_unchanged(&before, &song);
}

#[test]
fn clearing_one_of_several_notes_keeps_the_beat_normal() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(1, 3), (2, 5)])])).unwrap();
    let _ = song.clear_note(at(0), string(1)).unwrap();
    let beat = song.beat(at(0)).unwrap();
    assert_eq!(beat.kind(), BeatKind::Normal);
    let notes: Vec<_> = beat.notes().collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].string().unwrap(), string(2));
}

#[test]
fn setting_a_dotted_or_tuplet_length_uses_the_exact_duration() {
    let mut song = from_gp(one_measure(vec![rest(4)])).unwrap();
    let before = song.clone();

    let dotted = NoteValue::new(8, 1, None).unwrap();
    let change = song.set_duration(at(0), dotted).unwrap();
    assert_eq!(
        song.beat(at(0)).unwrap().duration().unwrap(),
        dotted.duration().unwrap()
    );

    let triplet = NoteValue::new(8, 0, Some((3, 2))).unwrap();
    let _ = song.set_duration(at(0), triplet).unwrap();
    assert_eq!(
        song.beat(at(0)).unwrap().duration().unwrap(),
        triplet.duration().unwrap()
    );

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn inserting_a_beat_adds_a_rest_before_it_and_undo_removes_it() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(1, 3)])])).unwrap();
    let before = song.clone();
    let change = song.insert_beat(at(0), note(8)).unwrap();

    assert_eq!(song.beat(at(0)).unwrap().kind(), BeatKind::Rest);
    assert_eq!(
        song.beat(at(0)).unwrap().duration().unwrap(),
        Duration::new(1, 8).unwrap()
    );
    assert_eq!(song.beat(at(1)).unwrap().kind(), BeatKind::Normal);
    assert_eq!(song.beat(at(1)).unwrap().notes().count(), 1);

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn a_placeholder_beat_cannot_be_resized() {
    let mut song = from_gp(one_measure(vec![empty_beat()])).unwrap();
    let before = song.clone();
    assert!(song.set_duration(at(0), note(8)).is_err());
    assert_unchanged(&before, &song);
}

#[test]
fn a_beat_can_be_appended_at_the_end_but_not_past_it() {
    let mut song = from_gp(one_measure(vec![rest(4)])).unwrap();
    let _ = song.insert_beat(at(1), note(4)).unwrap();
    assert_eq!(song.beat(at(0)).unwrap().index().get(), 0);
    assert!(song.insert_beat(at(3), note(4)).is_err());
}

#[test]
fn deleting_a_beat_removes_it_and_undo_puts_it_back() {
    let mut song = from_gp(one_measure(vec![beat(4, &[(1, 3)]), rest(4)])).unwrap();
    let before = song.clone();
    let change = song.delete_beat(at(0)).unwrap();
    assert_eq!(song.beat(at(0)).unwrap().kind(), BeatKind::Rest);
    assert!(song.delete_beat(at(5)).is_err());
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn inserting_a_measure_adds_a_rest_filled_bar_to_every_track() {
    let mut song = from_gp(test_song(vec![
        track(
            "Guitar",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![rest(4)]]),
                measure((4, 4), vec![vec![rest(4)]]),
            ],
        ),
        track(
            "Bass",
            BASS_4,
            vec![
                measure((4, 4), vec![vec![rest(4)]]),
                measure((4, 4), vec![vec![rest(4)]]),
            ],
        ),
    ]))
    .unwrap();
    let before = song.clone();
    let change = song.insert_measure(MeasureIndex::new(1)).unwrap();

    assert_eq!(song.measure_count(), 3);
    for track in song.tracks() {
        assert_eq!(track.measures().len(), 3, "{track:?}");
    }
    let added = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(1))
        .unwrap();
    assert_eq!(
        added.time_signature().unwrap(),
        TimeSignature::new(4, 4).unwrap()
    );
    let voice = added.voice(VoiceIndex::new(0)).unwrap();
    assert_eq!(
        voice.total_duration().unwrap(),
        TimeSignature::new(4, 4).unwrap().measure_length()
    );
    assert!(voice.beats().all(|beat| beat.kind() == BeatKind::Rest));

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn inserting_at_the_beginning_inherits_the_first_signature() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((7, 8), vec![vec![rest(8)]])],
    )]))
    .unwrap();

    let _ = song.insert_measure(MeasureIndex::new(0)).unwrap();
    let added = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(0))
        .unwrap();
    assert_eq!(
        added.time_signature().unwrap(),
        TimeSignature::new(7, 8).unwrap()
    );
    let voice = added.voice(VoiceIndex::new(0)).unwrap();
    assert_eq!(
        voice.total_duration().unwrap(),
        TimeSignature::new(7, 8).unwrap().measure_length()
    );
}

#[test]
fn a_new_measure_keeps_the_neighbours_voice_count() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![rest(4)], vec![rest(4)]])],
    )]))
    .unwrap();
    let _ = song.insert_measure(MeasureIndex::new(1)).unwrap();
    let added = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(1))
        .unwrap();
    assert_eq!(added.voices().len(), 2);
    // Voice 1 is filled with rests; voice 2 stays empty, as in Guitar Pro.
    let voices: Vec<_> = added.voices().collect();
    assert_eq!(
        voices[0].total_duration().unwrap(),
        TimeSignature::new(4, 4).unwrap().measure_length()
    );
    assert!(voices[1].is_empty());
    // The fixture's first measure is underfull on purpose; the new one has no issues.
    let issues: Vec<_> = song
        .issues()
        .into_iter()
        .filter(|issue| issue.measure == MeasureIndex::new(1))
        .collect();
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn inserting_a_measure_into_a_song_without_tracks_is_refused() {
    let mut song = Song::default();
    assert!(song.insert_measure(MeasureIndex::new(0)).is_err());
}

#[test]
fn a_track_without_measures_gets_its_first_measure() {
    // A track with zero measures is valid; editing can add the first bar.
    let mut song = from_gp(test_song(vec![track("Guitar", GUITAR_6, Vec::new())])).unwrap();
    assert_eq!(song.measure_count(), 0);
    let change = song.insert_measure(MeasureIndex::new(0)).unwrap();
    assert_eq!(song.measure_count(), 1);
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().measures().len(), 1);
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .time_signature()
            .unwrap(),
        TimeSignature::new(4, 4).unwrap()
    );
    song.revert(&change).unwrap();
    assert_eq!(song.measure_count(), 0);
}

#[test]
fn deleting_a_measure_removes_it_from_every_track_and_undo_restores_it() {
    let mut song = from_gp(test_song(vec![
        track(
            "Guitar",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![rest(4)]]),
                measure((3, 4), vec![vec![rest(4)]]),
                measure((4, 4), vec![vec![rest(4)]]),
            ],
        ),
        track(
            "Bass",
            BASS_4,
            vec![
                measure((4, 4), vec![vec![rest(4)]]),
                measure((3, 4), vec![vec![rest(4)]]),
                measure((4, 4), vec![vec![rest(4)]]),
            ],
        ),
    ]))
    .unwrap();
    let before = song.clone();
    let change = song.delete_measure(MeasureIndex::new(1)).unwrap();

    assert_eq!(song.measure_count(), 2);
    for track in song.tracks() {
        assert_eq!(track.measures().len(), 2);
        assert_eq!(
            track
                .measure(MeasureIndex::new(1))
                .unwrap()
                .time_signature()
                .unwrap(),
            TimeSignature::new(4, 4).unwrap()
        );
    }

    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn the_only_measure_and_out_of_range_measures_cannot_be_deleted() {
    let mut song = from_gp(one_measure(vec![rest(4)])).unwrap();
    let before = song.clone();
    assert!(song.delete_measure(MeasureIndex::new(0)).is_err());
    assert!(song.delete_measure(MeasureIndex::new(9)).is_err());
    assert_unchanged(&before, &song);
}

#[test]
fn rest_filling_covers_common_time_signatures_exactly() {
    for (numerator, denominator) in [
        (4, 4),
        (3, 4),
        (2, 4),
        (7, 8),
        (5, 4),
        (6, 8),
        (1, 4),
        (12, 8),
    ] {
        let signature = TimeSignature::new(numerator, denominator).unwrap();
        let rests = super::measures::fill_rests(signature.measure_length()).unwrap();
        let sum = rests.iter().try_fold(Duration::ZERO, |sum, value| {
            sum.checked_add(value.duration().unwrap())
        });
        assert_eq!(sum.unwrap(), signature.measure_length(), "{signature}");
        assert!(!rests.is_empty(), "{signature}");
    }
}

#[test]
fn undoing_an_insert_into_a_measure_without_voices_removes_the_created_voice() {
    // GP7 imports an empty bar without voices; inserting a beat creates voice 1.
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![])],
    )]))
    .unwrap();
    let measure = |song: &Song| {
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .len()
    };
    assert_eq!(measure(&song), 0);
    let change = song.insert_beat(at(0), note(4)).unwrap();
    assert_eq!(measure(&song), 1);
    song.revert(&change).unwrap();
    assert_eq!(measure(&song), 0, "undo must leave no voice behind");
}
