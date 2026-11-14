//! Tests for the design-mode edits: a bar's width scale and a beat column's offset. Each
//! edit changes only what it says, refuses bad values, and undoes exactly.

use crate::edit::design::{DESIGN_SCALE_RANGE, DESIGN_SPACING_RANGE};
use crate::interop::from_gp;
use crate::position::{BeatIndex, BeatPosition, MeasureIndex, StaffIndex, TrackIndex, VoiceIndex};
use crate::test_support::{GUITAR_6, beat, measure, song as test_song, track};
use crate::{ScaleScope, Song};

fn song() -> Song {
    from_gp(test_song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure(
            (4, 4),
            vec![vec![beat(4, &[(1, 3)]), beat(4, &[(2, 5)])]],
        )],
    )]))
    .unwrap()
}

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        staff: StaffIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn scale(song: &Song) -> f32 {
    song.measure_scale(TrackIndex::new(0), StaffIndex::new(0), MeasureIndex::new(0))
}

#[test]
fn a_song_wide_scale_applies_to_every_staff_and_is_undone() {
    let mut song = song();
    assert_eq!(scale(&song), 1.0);
    let change = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), Some(1.5))
        .unwrap();
    assert_eq!(scale(&song), 1.5);
    assert_eq!(song.song_measure_scale(MeasureIndex::new(0)), Some(1.5));
    song.revert(&change).unwrap();
    assert_eq!(scale(&song), 1.0);
    assert_eq!(song.song_measure_scale(MeasureIndex::new(0)), None);
}

#[test]
fn a_staffs_own_scale_overrides_the_songs_and_both_undo() {
    let mut song = song();
    let song_change = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), Some(1.25))
        .unwrap();
    let staff_change = song
        .set_measure_scale(
            ScaleScope::Staff {
                track: TrackIndex::new(0),
                staff: StaffIndex::new(0),
            },
            MeasureIndex::new(0),
            Some(2.0),
        )
        .unwrap();
    assert_eq!(scale(&song), 2.0, "the staff's own bar wins");
    song.revert(&staff_change).unwrap();
    assert_eq!(scale(&song), 1.25, "the song's scale is back");
    song.revert(&song_change).unwrap();
    assert_eq!(scale(&song), 1.0);
}

#[test]
fn a_beat_column_offset_is_set_rounded_and_undone() {
    let mut song = song();
    assert_eq!(
        song.track(TrackIndex::new(0))
            .and_then(|track| track.measure(MeasureIndex::new(0)))
            .and_then(|measure| measure.voice(VoiceIndex::new(0)))
            .and_then(|voice| voice.beat(BeatIndex::new(1)))
            .and_then(|beat| beat.spacing()),
        None
    );
    let change = song.set_beat_spacing(at(1), Some(0.123_456)).unwrap();
    let spacing = |song: &Song, beat: usize| {
        song.track(TrackIndex::new(0))
            .and_then(|track| track.measure(MeasureIndex::new(0)))
            .and_then(|measure| measure.voice(VoiceIndex::new(0)))
            .and_then(|voice| voice.beat(BeatIndex::new(beat)))
            .and_then(|beat| beat.spacing())
    };
    assert_eq!(spacing(&song, 1), Some(0.123));
    // The other beat is untouched.
    assert_eq!(spacing(&song, 0), None);
    song.revert(&change).unwrap();
    assert_eq!(spacing(&song, 1), None);
}

#[test]
fn a_offset_that_rounds_to_nothing_is_removed() {
    let mut song = song();
    let change = song.set_beat_spacing(at(0), Some(0.4)).unwrap();
    let removed = song.set_beat_spacing(at(0), Some(0.000_1)).unwrap();
    let spacing = |song: &Song| {
        song.track(TrackIndex::new(0))
            .and_then(|track| track.measure(MeasureIndex::new(0)))
            .and_then(|measure| measure.voice(VoiceIndex::new(0)))
            .and_then(|voice| voice.beat(BeatIndex::new(0)))
            .and_then(|beat| beat.spacing())
    };
    assert_eq!(spacing(&song), None);
    song.revert(&removed).unwrap();
    assert_eq!(spacing(&song), Some(0.4));
    song.revert(&change).unwrap();
    assert_eq!(spacing(&song), None);
}

#[test]
fn hostile_values_are_refused_and_change_nothing() {
    let mut song = song();
    let before = song.clone();
    for scale in [Some(0.0), Some(-1.0), Some(9.0), Some(f32::NAN)] {
        assert!(
            song.set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), scale)
                .is_err(),
            "{scale:?}"
        );
    }
    for spacing in [Some(-9.0), Some(9.0), Some(f32::INFINITY)] {
        assert!(
            song.set_beat_spacing(at(0), spacing).is_err(),
            "{spacing:?}"
        );
    }
    // A measure that does not exist is refused too.
    assert!(
        song.set_measure_scale(ScaleScope::Song, MeasureIndex::new(99), Some(1.0))
            .is_err()
    );
    assert_eq!(scale(&song), 1.0);
    assert_eq!(song.song_measure_scale(MeasureIndex::new(0)), None);
    assert_eq!(before.song_measure_scale(MeasureIndex::new(0)), None);
    // The range bounds themselves are accepted.
    assert!(
        song.set_measure_scale(
            ScaleScope::Song,
            MeasureIndex::new(0),
            Some(DESIGN_SCALE_RANGE.0)
        )
        .is_ok()
    );
    assert!(
        song.set_beat_spacing(at(0), Some(DESIGN_SPACING_RANGE.1))
            .is_ok()
    );
}
