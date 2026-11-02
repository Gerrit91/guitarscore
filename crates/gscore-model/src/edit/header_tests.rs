//! Tests for the header and beat-switch edits of M14: rests, ties, tempo, signatures,
//! section markers and batches.

use crate::duration::TimeSignature;
use crate::interop::from_gp;
use crate::position::{
    BeatIndex, BeatPosition, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
};
use crate::style::TrackColor;
use crate::test_support::{GUITAR_6, beat, measure, song as test_song, track};
use crate::views::BeatKind;
use crate::{Change, Song, differences};

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(
        differences(before, song).is_empty(),
        "{:?}",
        differences(before, song)
    );
}

#[test]
fn a_beat_becomes_a_rest_and_undo_puts_its_notes_back() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(4, &[(1, 3), (2, 5)])]])],
    )]))
    .unwrap();
    let before = song.clone();
    let change = song.set_beat_rest(at(0)).unwrap();
    assert_eq!(song.beat(at(0)).unwrap().kind(), BeatKind::Rest);
    assert_eq!(song.beat(at(0)).unwrap().notes().count(), 0);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn a_note_can_be_tied_and_untied() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(4, &[(1, 3)])]])],
    )]))
    .unwrap();
    let before = song.clone();
    let string = StringNumber::new(1).unwrap();
    let change = song.set_note_tie(at(0), string, true).unwrap();
    assert_eq!(
        song.beat(at(0)).unwrap().notes().next().unwrap().kind(),
        crate::NoteKind::Tie
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(
        song.set_note_tie(at(0), StringNumber::new(9).unwrap(), true)
            .is_err()
    );
}

#[test]
fn the_measure_tempo_changes_and_undo_restores_it() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![]), measure((4, 4), vec![])],
    )]))
    .unwrap();
    let before = song.clone();
    let change = song.set_measure_tempo(MeasureIndex::new(1), 144).unwrap();
    assert_eq!(
        song.tempo_change(MeasureIndex::new(1)),
        Some(144),
        "the sequence reads the header tempo"
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(song.set_measure_tempo(MeasureIndex::new(0), 0).is_err());
    assert!(song.set_measure_tempo(MeasureIndex::new(9), 120).is_err());
}

#[test]
fn a_time_signature_reaches_every_track_and_undoes() {
    let mut song = from_gp(test_song(vec![
        track("Guitar", GUITAR_6, vec![measure((4, 4), vec![])]),
        track("Bass", GUITAR_6, vec![measure((4, 4), vec![])]),
    ]))
    .unwrap();
    let before = song.clone();
    let change = song.set_time_signature(MeasureIndex::new(0), 3, 4).unwrap();
    assert_eq!(
        song.track(TrackIndex::new(1))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .time_signature()
            .unwrap(),
        TimeSignature::new(3, 4).unwrap()
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(song.set_time_signature(MeasureIndex::new(0), 4, 3).is_err());
    assert!(song.set_time_signature(MeasureIndex::new(9), 4, 4).is_err());
}

#[test]
fn a_key_signature_reaches_every_track_and_undoes() {
    let mut song = from_gp(test_song(vec![
        track("Guitar", GUITAR_6, vec![measure((4, 4), vec![])]),
        track("Bass", GUITAR_6, vec![measure((4, 4), vec![])]),
    ]))
    .unwrap();
    let before = song.clone();
    let change = song
        .set_key_signature(MeasureIndex::new(0), 2, true)
        .unwrap();
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert!(
        song.set_key_signature(MeasureIndex::new(0), 8, false)
            .is_err()
    );
}

#[test]
fn a_section_marker_is_set_and_removed() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![]), measure((4, 4), vec![])],
    )]))
    .unwrap();
    let before = song.clone();
    let change = song
        .set_section(MeasureIndex::new(1), "Chorus", TrackColor::new(0x00FF00))
        .unwrap();
    let section = song.section(MeasureIndex::new(1)).unwrap();
    assert_eq!(section.title, "Chorus");
    assert_eq!(section.color, TrackColor::new(0x00FF00));
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);

    let change = song
        .set_section(MeasureIndex::new(0), "Intro", TrackColor::DEFAULT)
        .unwrap();
    let remove = song
        .set_section(MeasureIndex::new(0), "", TrackColor::DEFAULT)
        .unwrap();
    assert!(song.section(MeasureIndex::new(0)).is_none());
    song.revert(&remove).unwrap();
    assert!(song.section(MeasureIndex::new(0)).is_some());
    song.revert(&change).unwrap();
    assert!(song.section(MeasureIndex::new(0)).is_none());
    assert!(
        song.set_section(MeasureIndex::new(0), &"x".repeat(300), TrackColor::DEFAULT)
            .is_err()
    );
}

#[test]
fn a_batch_undoes_its_parts_in_reverse() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(4, &[(1, 3)])]])],
    )]))
    .unwrap();
    let before = song.clone();
    let first = song
        .set_note(
            at(0),
            StringNumber::new(2).unwrap(),
            crate::Fret::new(7).unwrap(),
        )
        .unwrap();
    let second = song.set_measure_tempo(MeasureIndex::new(0), 90).unwrap();
    let batch = Change::batch(vec![first, second]);
    assert_eq!(song.beat(at(0)).unwrap().notes().count(), 2);
    song.revert(&batch).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn a_second_voice_is_created_for_its_first_beat() {
    use crate::duration::NoteValue;
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![beat(4, &[])]])],
    )]))
    .unwrap();
    let quarter = NoteValue::new(4, 0, None).unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(1),
        beat: BeatIndex::new(0),
    };
    let change = song.insert_beat(at, quarter).unwrap();
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .len(),
        2
    );
    song.revert(&change).unwrap();
    assert_eq!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .len(),
        1
    );
}
