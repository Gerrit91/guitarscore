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
        staff: StaffIndex::new(0),
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

#[test]
fn a_fourth_voice_is_created_with_the_ones_before_it_and_undone() {
    let mut song = from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![vec![rest(4)]])],
    )]))
    .unwrap();
    let voices = |song: &Song| {
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .len()
    };
    let fourth = BeatPosition {
        voice: crate::VoiceIndex::new(3),
        ..at(0)
    };
    let change = song.insert_beat(fourth, note(4)).unwrap();
    assert_eq!(voices(&song), 4);
    song.revert(&change).unwrap();
    assert_eq!(voices(&song), 1, "undo removes the voices it created");
    let fifth = BeatPosition {
        voice: crate::VoiceIndex::new(4),
        ..at(0)
    };
    assert!(
        song.insert_beat(fifth, note(4)).is_err(),
        "four voices at most"
    );
}

/// A two-track, three-measure song: quarter rests in every measure.
fn three_measures() -> Song {
    let measures = || {
        (0..3)
            .map(|_| measure((4, 4), vec![vec![rest(4), rest(4), rest(4), rest(4)]]))
            .collect()
    };
    from_gp(test_song(vec![
        track("Guitar", GUITAR_6, measures()),
        track("Bass", BASS_4, measures()),
    ]))
    .unwrap()
}

fn stamps(song: &Song) -> Vec<u64> {
    (0..song.measure_count())
        .map(|m| song.measure_stamp(MeasureIndex::new(m)))
        .collect()
}

#[test]
fn measure_stamps_change_only_for_the_edited_measure() {
    let mut song = three_measures();
    let first = stamps(&song);
    assert!(first.iter().all(|&s| s != 0));
    assert_eq!(
        song.measure_stamp(MeasureIndex::new(3)),
        0,
        "no such measure"
    );
    let mut unique = first.clone();
    unique.dedup();
    assert_eq!(unique.len(), 3, "stamps differ: {first:?}");

    // A note in measure 2 of the bass: only measure 2's stamp changes.
    let at = BeatPosition {
        staff: StaffIndex::new(0),
        track: TrackIndex::new(1),
        measure: MeasureIndex::new(1),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(2),
    };
    let change = song.set_note(at, string(4), fret(3)).unwrap();
    let edited = stamps(&song);
    assert_eq!((edited[0], edited[2]), (first[0], first[2]));
    assert_ne!(edited[1], first[1]);
    // Undo is another change: a new stamp, never an old one again.
    song.revert(&change).unwrap();
    let undone = stamps(&song);
    assert!(undone[1] != first[1] && undone[1] != edited[1]);

    // A failed edit changes neither the song nor its stamps.
    let before = song.clone();
    let mut missing = at;
    missing.beat = BeatIndex::new(9);
    assert!(song.set_note(missing, string(1), fret(1)).is_err());
    assert!(
        song.set_note(at, string(5), fret(1)).is_err(),
        "a bass has 4 strings"
    );
    assert!(song.delete_beat(missing).is_err());
    assert_eq!(stamps(&song), undone);
    assert_unchanged(&before, &song);
}

#[test]
fn inserted_and_removed_measures_move_the_stamps_with_them() {
    let mut song = three_measures();
    let first = stamps(&song);
    let inserted = song.insert_measure(MeasureIndex::new(1)).unwrap();
    let after_insert = stamps(&song);
    assert_eq!(after_insert.len(), 4);
    assert_eq!(after_insert[0], first[0]);
    assert_eq!(
        &after_insert[2..],
        &first[1..],
        "later measures keep their stamps"
    );
    assert!(
        !first.contains(&after_insert[1]),
        "the new measure has a new stamp"
    );

    let removed = song.delete_measure(MeasureIndex::new(0)).unwrap();
    assert_eq!(stamps(&song), after_insert[1..].to_vec());
    // Undoing both brings the measures (and fresh stamps for re-inserted ones) back.
    song.revert(&removed).unwrap();
    song.revert(&inserted).unwrap();
    let undone = stamps(&song);
    assert_eq!(undone.len(), 3);
    assert_eq!(&undone[1..], &first[1..]);

    // Failed structural edits change nothing.
    let before = song.clone();
    assert!(song.insert_measure(MeasureIndex::new(9)).is_err());
    assert!(song.delete_measure(MeasureIndex::new(3)).is_err());
    assert_eq!(stamps(&song), undone);
    assert_unchanged(&before, &song);
}

/// The names of all tracks, in order.
fn track_names(song: &Song) -> Vec<String> {
    song.tracks().map(|track| track.name().to_owned()).collect()
}

/// Two tracks with one measure each, the first with a note. Both use channel 0 of a real
/// 64-channel table (as a GP3-5 file has), at a volume and pan away from the defaults.
fn two_tracks() -> Song {
    let mut gp = test_song(vec![
        track(
            "Lead",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(4, &[(1, 5)])]])],
        ),
        track("Bass", BASS_4, vec![measure((4, 4), vec![vec![rest(4)]])]),
    ]);
    gp.channels = (0..64)
        .map(|index| gscore_gp::MidiChannel {
            channel: u8::try_from(index % 16).unwrap_or(0),
            volume: 90,
            balance: 30,
            ..gscore_gp::MidiChannel::default()
        })
        .collect();
    from_gp(gp).unwrap()
}

#[test]
fn inserting_a_track_fills_its_measures_with_rests_and_undo_restores_the_song() {
    let mut song = two_tracks();
    let before = song.clone();
    let change = song.insert_track(TrackIndex::new(1)).unwrap();
    assert_eq!(song.track_count(), 3);
    assert_eq!(track_names(&song)[1], "Track");
    let new = song.track(TrackIndex::new(1)).unwrap();
    assert_eq!(new.string_count(), 6, "the neighbour lends its tuning");
    assert_eq!(new.measures().len(), song.measure_count());
    let measure = new.measure(MeasureIndex::new(0)).unwrap();
    assert!(!measure.has_notes());
    assert_eq!(
        measure.voices().next().unwrap().beats().count(),
        1,
        "one rest fills the measure"
    );
    assert_eq!(
        new.measure(MeasureIndex::new(0)).unwrap().index(),
        MeasureIndex::new(0)
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(song.track_count(), 2);
}

#[test]
fn deleting_and_duplicating_tracks_undo_exactly() {
    let mut song = two_tracks();
    let before = song.clone();

    let change = song.duplicate_track(TrackIndex::new(0)).unwrap();
    assert_eq!(track_names(&song), ["Lead", &track_names(&song)[1], "Bass"]);
    assert_eq!(song.track_count(), 3);
    assert!(
        song.track(TrackIndex::new(1))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .has_notes(),
        "the copy keeps the notes"
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);

    let change = song.delete_track(TrackIndex::new(0)).unwrap();
    assert_eq!(track_names(&song), ["Bass"]);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);

    let change = song.delete_track(TrackIndex::new(1)).unwrap();
    assert_eq!(track_names(&song), ["Lead"]);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
}

#[test]
fn a_tracks_sound_state_follows_insert_delete_duplicate_and_move() {
    let mut song = two_tracks();
    assert_eq!(track_names(&song), ["Lead", "Bass"]);
    let lead = r#"{"comp_send":96,"tone":"lead"}"#.to_owned();
    let bass = r#"{"comp_send":0,"tone":"bass"}"#.to_owned();
    let _ = song
        .set_track_sound_state(TrackIndex::new(0), Some(lead.clone()))
        .unwrap();
    let _ = song
        .set_track_sound_state(TrackIndex::new(1), Some(bass.clone()))
        .unwrap();
    let before = song.clone();
    let before_states = sound_states(&before);

    // A duplicated track copies the state.
    let change = song.duplicate_track(TrackIndex::new(0)).unwrap();
    let states: Vec<Option<&str>> = song.tracks().map(|track| track.sound_state()).collect();
    assert_eq!(
        states,
        [
            Some(lead.as_str()),
            Some(lead.as_str()),
            Some(bass.as_str())
        ]
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);

    // A moved track keeps its state.
    let change = song
        .move_track(TrackIndex::new(0), TrackIndex::new(1))
        .unwrap();
    let states: Vec<Option<&str>> = song.tracks().map(|track| track.sound_state()).collect();
    assert_eq!(states, [Some(bass.as_str()), Some(lead.as_str())]);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);

    // A deleted track takes its state with it, and undo brings it back.
    let change = song.delete_track(TrackIndex::new(0)).unwrap();
    assert_eq!(track_names(&song), ["Bass"]);
    assert_eq!(
        song.track(TrackIndex::new(0)).unwrap().sound_state(),
        Some(bass.as_str())
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);

    // A new track starts without one; its neighbours keep theirs.
    let change = song.insert_track(TrackIndex::new(1)).unwrap();
    assert_eq!(song.track(TrackIndex::new(1)).unwrap().sound_state(), None);
    assert_eq!(
        song.track(TrackIndex::new(0)).unwrap().sound_state(),
        Some(lead.as_str())
    );
    assert_eq!(
        song.track(TrackIndex::new(2)).unwrap().sound_state(),
        Some(bass.as_str())
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);

    // Setting and clearing are edits with undo too.
    let change = song
        .set_track_sound_state(TrackIndex::new(0), None)
        .unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().sound_state(), None);
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);

    // A missing track, and a payload beyond the bound, are refused without a change.
    assert!(
        song.set_track_sound_state(TrackIndex::new(9), None)
            .is_err()
    );
    let huge = "x".repeat(300 * 1024);
    assert!(
        song.set_track_sound_state(TrackIndex::new(0), Some(huge))
            .is_err()
    );
    assert_unchanged(&before, &song);
    assert_eq!(sound_states(&song), before_states);
}

/// Every track's own sound payload, in order.
fn sound_states(song: &Song) -> Vec<Option<String>> {
    song.tracks()
        .map(|track| track.sound_state().map(str::to_owned))
        .collect()
}

#[test]
fn the_songs_own_sound_state_is_an_undoable_edit() {
    let mut song = two_tracks();
    assert_eq!(song.sound_state(), None);
    let before = song.clone();
    let payload = r#"{"buses":{"reverb":{"level":96}}}"#.to_owned();
    let change = song.set_sound_state(Some(payload.clone())).unwrap();
    assert_eq!(song.sound_state(), Some(payload.as_str()));
    song.revert(&change).unwrap();
    assert_eq!(song.sound_state(), None);
    assert_unchanged(&before, &song);

    // Clearing it is an edit too, and undo brings it back.
    let _ = song.set_sound_state(Some(payload.clone())).unwrap();
    let set = song.clone();
    let change = song.set_sound_state(None).unwrap();
    assert_eq!(song.sound_state(), None);
    song.revert(&change).unwrap();
    assert_unchanged(&set, &song);

    // A payload beyond the bound is refused without a change.
    let huge = "x".repeat(300 * 1024);
    assert!(song.set_sound_state(Some(huge)).is_err());
    assert_unchanged(&set, &song);
}

#[test]
fn moving_a_track_changes_the_order_and_undo_puts_it_back() {
    let mut song = two_tracks();
    let before = song.clone();
    let change = song
        .move_track(TrackIndex::new(0), TrackIndex::new(1))
        .unwrap();
    assert_eq!(track_names(&song), ["Bass", "Lead"]);
    assert_eq!(
        song.track(TrackIndex::new(1))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .index(),
        MeasureIndex::new(0)
    );
    song.revert(&change).unwrap();
    assert_unchanged(&before, &song);
    // A move onto the same index is refused.
    assert!(
        song.move_track(TrackIndex::new(0), TrackIndex::new(0))
            .is_err()
    );
}

#[test]
fn renaming_a_track_undoes_and_rejects_unbounded_names() {
    let mut song = two_tracks();
    let change = song.rename_track(TrackIndex::new(0), "Rhythm").unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().name(), "Rhythm");
    song.revert(&change).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().name(), "Lead");
    let long = "x".repeat(300);
    assert!(song.rename_track(TrackIndex::new(0), &long).is_err());
    assert!(song.rename_track(TrackIndex::new(9), "Nobody").is_err());
}

#[test]
fn mute_solo_and_color_undo() {
    let mut song = two_tracks();
    let track = TrackIndex::new(0);
    let changes = [
        song.set_track_mute(track, true).unwrap(),
        song.set_track_solo(track, true).unwrap(),
        song.set_track_color(track, TrackColor::new(0x123456))
            .unwrap(),
    ];
    assert!(song.track(track).unwrap().is_muted());
    assert!(song.track(track).unwrap().is_soloed());
    assert_eq!(song.track(track).unwrap().color().rgb(), 0x123456);
    assert!(song.any_soloed());
    for change in changes.iter().rev() {
        song.revert(change).unwrap();
    }
    assert!(!song.track(track).unwrap().is_muted());
    assert!(!song.track(track).unwrap().is_soloed());
    assert_eq!(song.track(track).unwrap().color(), TrackColor::DEFAULT);
    assert!(!song.any_soloed());
}

#[test]
fn a_shared_channel_is_cloned_so_only_one_track_changes() {
    let mut song = two_tracks();
    // Both tracks use channel 0.
    let both_sound = |song: &Song| {
        let sound = |track| song.track_sound(TrackIndex::new(track)).unwrap();
        (sound(0), sound(1))
    };
    assert_eq!(
        song.track_sound(TrackIndex::new(0)).unwrap().volume,
        song.track_sound(TrackIndex::new(1)).unwrap().volume
    );
    let before = both_sound(&song);
    assert_eq!((before.0.volume, before.0.pan), (90, 30));
    let change = song.set_track_volume(TrackIndex::new(0), 20).unwrap();
    let after = both_sound(&song);
    assert_eq!(after.0.volume, 20);
    assert_eq!(
        after.1.volume, before.1.volume,
        "the other track keeps its volume"
    );

    let change_pan = song.set_track_pan(TrackIndex::new(0), 127).unwrap();
    assert_eq!(song.track_sound(TrackIndex::new(0)).unwrap().pan, 127);
    assert_eq!(
        song.track_sound(TrackIndex::new(1)).unwrap().pan,
        before.1.pan
    );
    song.revert(&change_pan).unwrap();
    song.revert(&change).unwrap();
    assert_eq!(both_sound(&song), before);
    // The track is back on the shared channel, not on the copy that undo removed.
    let after = song.track_sound(TrackIndex::new(0)).unwrap();
    assert_eq!((after.volume, after.pan), (90, 30));

    // A later change still works and can be undone.
    let change = song.set_track_volume(TrackIndex::new(0), 100).unwrap();
    assert_eq!(song.track_sound(TrackIndex::new(0)).unwrap().volume, 100);
    song.revert(&change).unwrap();
    assert_eq!(song.track_sound(TrackIndex::new(0)).unwrap().volume, 90);
}

#[test]
fn reverb_and_chorus_sends_change_one_track_and_undo() {
    let mut song = two_tracks();
    let sound = |song: &Song, track| song.track_sound(TrackIndex::new(track)).unwrap();
    let before = (sound(&song, 0), sound(&song, 1));
    let reverb = song.set_track_reverb(TrackIndex::new(0), 90).unwrap();
    let chorus = song.set_track_chorus(TrackIndex::new(0), 200).unwrap();
    assert_eq!(sound(&song, 0).reverb, 90);
    assert_eq!(sound(&song, 0).chorus, 127, "clamped to the MIDI range");
    assert_eq!(sound(&song, 1), before.1, "the shared channel was copied");
    song.revert(&chorus).unwrap();
    song.revert(&reverb).unwrap();
    assert_eq!((sound(&song, 0), sound(&song, 1)), before);
    assert!(song.set_track_reverb(TrackIndex::new(9), 10).is_err());
}

#[test]
fn sections_and_note_squares_come_from_the_song() {
    let mut gp = test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![
            measure((4, 4), vec![vec![beat(4, &[(1, 5)])]]),
            measure((4, 4), vec![vec![rest(4)]]),
        ],
    )]);
    gp.measure_headers[0].marker = Some(gscore_gp::Marker {
        title: "Intro".to_owned(),
        color: 0x00FF00,
    });
    let song = from_gp(gp).unwrap();
    assert!(
        song.track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .has_notes()
    );
    assert!(
        !song
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(1))
            .unwrap()
            .has_notes()
    );
    let section = song.section(MeasureIndex::new(0)).unwrap();
    assert_eq!(section.title, "Intro");
    assert_eq!(section.color.rgb(), 0x00FF00);
    assert!(song.section(MeasureIndex::new(1)).is_none());
}

#[test]
fn settings_differences_name_what_a_save_may_not_keep() {
    let before = two_tracks();
    let mut after = before.clone();
    let _ = after.set_track_mute(TrackIndex::new(0), true).unwrap();
    let _ = after.set_track_volume(TrackIndex::new(1), 20).unwrap();
    let found = settings_differences(&before, &after);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .any(|difference| difference.detail.contains("mute"))
    );
    assert!(
        found
            .iter()
            .any(|difference| difference.detail.contains("volume"))
    );
    assert!(
        differences(&before, &after).is_empty(),
        "the notes did not change"
    );
}

#[test]
fn renaming_keeps_measure_stamps_but_moving_a_track_renews_them() {
    let mut song = three_measures();
    let before = stamps(&song);

    // Renaming changes the track, not any measure.
    let renamed = song.rename_track(TrackIndex::new(0), "Lead").unwrap();
    assert_eq!(stamps(&song), before, "a rename keeps the layout cache");

    // After a move a track index names another track, and a layout cache keeps measures
    // per track index (several tracks are laid out together): every stamp is renewed.
    let moved = song
        .move_track(TrackIndex::new(1), TrackIndex::new(0))
        .unwrap();
    let after_move = stamps(&song);
    assert!(
        after_move.iter().all(|stamp| !before.contains(stamp)),
        "a move renews the stamps"
    );

    // Undo is a track edit too: new stamps again, never the old ones.
    song.revert(&moved).unwrap();
    song.revert(&renamed).unwrap();
    assert_eq!(song.track(TrackIndex::new(0)).unwrap().name(), "Guitar");
    let undone = stamps(&song);
    assert!(undone.iter().all(|stamp| !after_move.contains(stamp)));
}

/// Each track's GP7 sound as the file had it (`<Sounds>`, `<AudioEngineState>`).
fn rse_sounds(song: &Song) -> Vec<(Option<String>, Option<String>)> {
    song.inner()
        .tracks
        .iter()
        .map(|track| (track.gpif_sounds.clone(), track.gpif_audio_engine.clone()))
        .collect()
}

#[test]
fn another_instrument_drops_the_gp7_sound_and_undo_brings_it_back() {
    let mut song = two_tracks();
    let sounds =
        "<Sounds><Sound><Path>Stringed/Electric Guitars/Clean Guitar</Path></Sound></Sounds>";
    let engine = "<AudioEngineState>RSE</AudioEngineState>";
    for track in &mut song.inner_mut().tracks {
        track.gpif_sounds = Some(sounds.to_owned());
        track.gpif_audio_engine = Some(engine.to_owned());
    }
    let kept = (Some(sounds.to_owned()), Some(engine.to_owned()));
    let program = song.track_sound(TrackIndex::new(0)).unwrap().program;

    // The same program again is no change of instrument: the sound stays.
    let change = song.set_track_program(TrackIndex::new(0), program).unwrap();
    assert_eq!(rse_sounds(&song), [kept.clone(), kept.clone()]);
    song.revert(&change).unwrap();

    // Another program: the old instrument's sound goes, only on that track.
    let change = song
        .set_track_program(TrackIndex::new(0), program.wrapping_add(1) % 128)
        .unwrap();
    assert_eq!(rse_sounds(&song), [(None, None), kept.clone()]);
    song.revert(&change).unwrap();
    assert_eq!(rse_sounds(&song), [kept.clone(), kept.clone()]);

    // A drum kit too.
    let change = song.set_track_drums(TrackIndex::new(1), true).unwrap();
    assert_eq!(rse_sounds(&song), [kept.clone(), (None, None)]);
    song.revert(&change).unwrap();
    assert_eq!(rse_sounds(&song), [kept.clone(), kept.clone()]);

    // A new track does not take its neighbour's sound.
    let change = song.insert_track(TrackIndex::new(1)).unwrap();
    assert_eq!(
        rse_sounds(&song),
        [kept.clone(), (None, None), kept.clone()]
    );
    song.revert(&change).unwrap();
    assert_eq!(rse_sounds(&song), [kept.clone(), kept]);
}
