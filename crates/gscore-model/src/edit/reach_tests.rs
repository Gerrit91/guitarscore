//! Tests that time and key signatures and the feel hold from their measure up to the next
//! change, as Guitar Pro applies them, and undo as one step.

use crate::interop::from_gp;
use crate::position::{MeasureIndex, TrackIndex};
use crate::test_support::{GUITAR_6, measure, song as test_song, track};
use crate::{Feel, Song, differences};

/// A two-track song of `count` empty 4/4 measures.
fn song(count: usize) -> Song {
    let measures = || {
        (0..count)
            .map(|_| measure((4, 4), vec![]))
            .collect::<Vec<_>>()
    };
    from_gp(test_song(vec![
        track("Guitar", GUITAR_6, measures()),
        track("Bass", GUITAR_6, measures()),
    ]))
    .unwrap()
}

/// Each measure's time signature in the second track, as "n/d".
fn times(song: &Song) -> Vec<String> {
    let track = song.track(TrackIndex::new(1)).unwrap();
    (0..song.measure_count())
        .map(|index| {
            let signature = track
                .measure(MeasureIndex::new(index))
                .unwrap()
                .time_signature()
                .unwrap();
            format!("{}/{}", signature.numerator(), signature.denominator())
        })
        .collect()
}

#[test]
fn a_time_signature_holds_until_the_next_change() {
    let mut song = song(5);
    let start = song.clone();
    let change = song.set_time_signature(MeasureIndex::new(1), 3, 4).unwrap();
    assert_eq!(times(&song), ["4/4", "3/4", "3/4", "3/4", "3/4"]);
    // It undoes as one step.
    let mut undone = song.clone();
    undone.revert(&change).unwrap();
    assert!(differences(&start, &undone).is_empty());
    // A later change stays where it is written; an earlier one stops before it.
    let _ = song.set_time_signature(MeasureIndex::new(3), 6, 8).unwrap();
    assert_eq!(times(&song), ["4/4", "3/4", "3/4", "6/8", "6/8"]);
    let back = song.set_time_signature(MeasureIndex::new(1), 4, 4).unwrap();
    assert_eq!(times(&song), ["4/4", "4/4", "4/4", "6/8", "6/8"]);
    song.revert(&back).unwrap();
    assert_eq!(times(&song), ["4/4", "3/4", "3/4", "6/8", "6/8"]);
}

#[test]
fn a_key_signature_holds_until_the_next_change() {
    let mut song = song(4);
    let key = |song: &Song| -> Vec<(i8, bool)> {
        (0..4)
            .map(|index| {
                let key = song.key_signature(MeasureIndex::new(index)).unwrap();
                (key.key, key.minor)
            })
            .collect()
    };
    let _ = song
        .set_key_signature(MeasureIndex::new(3), -1, false)
        .unwrap();
    let change = song
        .set_key_signature(MeasureIndex::new(1), 2, true)
        .unwrap();
    assert_eq!(key(&song), [(0, false), (2, true), (2, true), (-1, false)]);
    song.revert(&change).unwrap();
    assert_eq!(
        key(&song),
        [(0, false), (0, false), (0, false), (-1, false)]
    );
}

#[test]
fn a_feel_holds_until_the_next_change_and_undoes_as_one() {
    let mut song = song(4);
    let start = song.clone();
    let feels = |song: &Song| -> Vec<Feel> {
        (0..4)
            .map(|index| song.feel(MeasureIndex::new(index)).unwrap())
            .collect()
    };
    let change = song.set_feel(MeasureIndex::new(0), Feel::Eighth).unwrap();
    assert_eq!(feels(&song), [Feel::Eighth; 4]);
    let straight = song.set_feel(MeasureIndex::new(2), Feel::None).unwrap();
    assert_eq!(
        feels(&song),
        [Feel::Eighth, Feel::Eighth, Feel::None, Feel::None]
    );
    song.revert(&straight).unwrap();
    song.revert(&change).unwrap();
    assert!(differences(&start, &song).is_empty());
}
