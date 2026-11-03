//! Tests for beat dynamics: setting, undoing, and new notes taking the dynamic in effect.

use crate::dynamics::Dynamic;
use crate::interop::from_gp;
use crate::position::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
};
use crate::test_support::{GUITAR_6, beat, measure, rest, song, track};
use crate::{Song, differences};

fn at(measure: usize, beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

/// Two measures: a chord and a rest, then a rest and an empty-ish quarter.
fn two_measures() -> Song {
    from_gp(song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![
            measure((2, 4), vec![vec![beat(4, &[(1, 3), (2, 5)]), rest(4)]]),
            measure((2, 4), vec![vec![rest(4), rest(4)]]),
        ],
    )]))
    .unwrap()
}

#[test]
fn a_dynamic_applies_to_every_note_of_the_beat_and_undoes() {
    let mut song = two_measures();
    let before = song.clone();
    // The test builder's notes are forte, as GP3-5's reader makes them.
    assert_eq!(song.beat(at(0, 0)).unwrap().dynamic(), Some(Dynamic::F));
    let change = song.set_dynamic(at(0, 0), Dynamic::Pp).unwrap();
    let beat = song.beat(at(0, 0)).unwrap();
    assert_eq!(beat.dynamic(), Some(Dynamic::Pp));
    assert!(beat.notes().all(|note| note.velocity() == 31));
    assert!(
        !differences(&before, &song).is_empty(),
        "the change is seen"
    );
    song.revert(&change).unwrap();
    assert!(differences(&before, &song).is_empty());
    assert_eq!(song.beat(at(0, 0)).unwrap().dynamic(), Some(Dynamic::F));
}

#[test]
fn rests_and_missing_beats_have_no_dynamic_to_set() {
    let mut song = two_measures();
    let before = song.clone();
    assert_eq!(song.beat(at(0, 1)).unwrap().dynamic(), None);
    assert!(song.set_dynamic(at(0, 1), Dynamic::P).is_err());
    assert!(song.set_dynamic(at(9, 0), Dynamic::P).is_err());
    assert!(differences(&before, &song).is_empty());
}

#[test]
fn new_notes_take_the_dynamic_in_effect() {
    let mut song = two_measures();
    let _ = song.set_dynamic(at(0, 0), Dynamic::Mp).unwrap();
    // The dynamic lasts over the rest and the barline.
    assert_eq!(song.dynamic_at(at(1, 1)), Dynamic::Mp);
    let string = StringNumber::new(3).unwrap();
    let _ = song
        .set_note(at(1, 1), string, Fret::new(2).unwrap())
        .unwrap();
    assert_eq!(song.beat(at(1, 1)).unwrap().dynamic(), Some(Dynamic::Mp));
    // A note added to a chord joins its dynamic.
    let _ = song
        .set_note(at(0, 0), string, Fret::new(4).unwrap())
        .unwrap();
    assert_eq!(
        song.beat(at(0, 0))
            .unwrap()
            .notes()
            .map(|n| n.dynamic())
            .collect::<Vec<_>>(),
        [Dynamic::Mp; 3]
    );
    // Without any note before, it is the default, mezzo-forte.
    let empty = from_gp(crate::test_support::song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((1, 4), vec![vec![rest(4)]])],
    )]))
    .unwrap();
    assert_eq!(empty.dynamic_at(at(0, 0)), Dynamic::Mf);
    assert_eq!(empty.dynamic_at(at(5, 5)), Dynamic::Mf);
    // Hostile positions answer at once (this used to walk back over usize::MAX measures).
    let far = BeatPosition {
        measure: MeasureIndex::new(usize::MAX),
        beat: BeatIndex::new(usize::MAX),
        ..at(0, 0)
    };
    assert_eq!(two_measures().dynamic_at(far), Dynamic::Mf);
}
