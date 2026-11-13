//! A track's staves stay in step: every edit that adds, removes or re-signs a measure does
//! it on every staff, and undo puts every staff back. A song whose staves are out of step
//! is refused when it is read.

use crate::interop::from_gp;
use crate::position::{MeasureIndex, StaffIndex, TrackIndex};
use crate::test_support::{GUITAR_6, beat, measure, rest, song as test_song, track};
use crate::*;

/// A piano-like track: its first staff on a guitar's stand-in tuning, a second staff on a
/// bass's (as Guitar Pro stores a keyboard), each with a note in both measures; and a
/// guitar track after it.
fn two_staves() -> Song {
    let mut gp = test_song(vec![
        track(
            "Piano",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![beat(1, &[(1, 5)])]]),
                measure((4, 4), vec![vec![beat(1, &[(2, 3)])]]),
            ],
        ),
        track(
            "Guitar",
            GUITAR_6,
            vec![
                measure((4, 4), vec![vec![rest(1)]]),
                measure((4, 4), vec![vec![rest(1)]]),
            ],
        ),
    ]);
    let mut bass_measures = vec![
        measure((4, 4), vec![vec![beat(1, &[(4, 7)])]]),
        measure((4, 4), vec![vec![beat(1, &[(5, 2)])]]),
    ];
    for bar in &mut bass_measures {
        bar.clef = gscore_gp::MeasureClef::Bass;
    }
    gp.tracks[0].staves.push(gscore_gp::Staff {
        measures: bass_measures,
        strings: vec![(1, 43), (2, 38), (3, 33), (4, 28), (5, 23)],
        has_tuning: true,
        ..gscore_gp::Staff::default()
    });
    from_gp(gp).unwrap()
}

fn second_staff(song: &Song) -> StaffView<'_> {
    song.track(TrackIndex::new(0))
        .unwrap()
        .staff(StaffIndex::new(1))
        .unwrap()
}

#[test]
fn the_second_staff_is_seen_with_its_own_strings() {
    let song = two_staves();
    let piano = song.track(TrackIndex::new(0)).unwrap();
    assert_eq!(piano.staff_count(), 2);
    assert_eq!(song.track(TrackIndex::new(1)).unwrap().staff_count(), 1);
    let staff = second_staff(&song);
    assert_eq!(staff.string_count(), 5);
    assert_eq!(staff.measures().len(), 2);
    let tuning = staff.note_tuning().unwrap().unwrap();
    assert_eq!(tuning.last().unwrap().pitch.get(), 23);
    assert!(piano.staff(StaffIndex::new(2)).is_none());
}

#[test]
fn inserting_and_deleting_a_measure_keeps_the_staves_in_step() {
    let mut song = two_staves();
    let before = song.clone();
    let inserted = song.insert_measure(MeasureIndex::new(1)).unwrap();
    assert_eq!(song.measure_count(), 3);
    let staff = second_staff(&song);
    assert_eq!(staff.measures().len(), 3);
    // The new bar on the bass staff keeps its clef (as stored: it shows no clef change).
    assert_eq!(
        song.inner().tracks[0].staves[0].measures[1].clef,
        gscore_gp::MeasureClef::Bass
    );
    let last = staff.measure(MeasureIndex::new(2)).unwrap();
    assert_eq!(last.voices().next().unwrap().beats().count(), 1);
    song.revert(&inserted).unwrap();
    assert!(differences(&before, &song).is_empty());

    let deleted = song.delete_measure(MeasureIndex::new(0)).unwrap();
    assert_eq!(second_staff(&song).measures().len(), 1);
    // The bar left is the second one, on both staves.
    let note = |song: &Song, staff: usize| {
        song.track(TrackIndex::new(0))
            .unwrap()
            .staff(StaffIndex::new(staff))
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
            .string()
            .unwrap()
            .get()
    };
    assert_eq!((note(&song, 0), note(&song, 1)), (2, 5));
    song.revert(&deleted).unwrap();
    assert!(differences(&before, &song).is_empty());
    assert_eq!((note(&song, 0), note(&song, 1)), (1, 4));
}

#[test]
fn a_time_signature_reaches_every_staff_and_undo_restores_it() {
    let mut song = two_staves();
    let before = song.clone();
    let signature = TimeSignature::new(3, 4).unwrap();
    let change = song.set_time_signature(MeasureIndex::new(1), 3, 4).unwrap();
    let bar = second_staff(&song).measure(MeasureIndex::new(1)).unwrap();
    assert_eq!(bar.time_signature().unwrap(), signature);
    song.revert(&change).unwrap();
    assert!(differences(&before, &song).is_empty());
}

#[test]
fn losing_the_second_staff_is_a_difference() {
    let song = two_staves();
    let mut gp = song.inner().clone();
    gp.tracks[0].staves.clear();
    let without = from_gp(gp).unwrap();
    let found = differences(&song, &without);
    assert!(
        found.iter().any(|d| d.detail.contains("staves")),
        "{found:?}"
    );
}

#[test]
fn a_staff_out_of_step_is_refused() {
    let song = two_staves();
    let mut gp = song.inner().clone();
    gp.tracks[0].staves[0].measures.pop();
    assert!(from_gp(gp).is_err());
}
