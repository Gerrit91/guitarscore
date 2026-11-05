//! Tests for the staves switch and the master mix.

use crate::interop::from_gp;
use crate::playback::{MASTER_UNITY, MasterMix, TrackSound, midi_to_unit, unit_to_midi};
use crate::position::{MeasureIndex, TrackIndex};
use crate::style::TrackStaves;
use crate::test_support::one_measure;
use crate::{Song, differences, settings_differences};

fn song() -> Song {
    from_gp(one_measure(vec![])).unwrap()
}

fn assert_unchanged(before: &Song, song: &Song) {
    assert!(differences(before, song).is_empty());
    assert!(settings_differences(before, song).is_empty());
}

#[test]
fn staves_change_relay_out_and_undo() {
    let mut song = song();
    let before = song.clone();
    let track = TrackIndex::new(0);
    assert_eq!(
        song.track(track).unwrap().staves(),
        TrackStaves::NOTATION_AND_TAB
    );
    for staves in [TrackStaves::NOTATION_ONLY, TrackStaves::TAB_ONLY] {
        let stamp = song.measure_stamp(MeasureIndex::new(0));
        let change = song.set_track_staves(track, staves).unwrap();
        assert_eq!(song.track(track).unwrap().staves(), staves);
        assert_ne!(
            song.measure_stamp(MeasureIndex::new(0)),
            stamp,
            "laid out again"
        );
        let stamp = song.measure_stamp(MeasureIndex::new(0));
        song.revert(&change).unwrap();
        assert_ne!(song.measure_stamp(MeasureIndex::new(0)), stamp);
        assert_unchanged(&before, &song);
        assert_eq!(
            song.track(track).unwrap().staves(),
            TrackStaves::NOTATION_AND_TAB
        );
    }
    assert!(
        song.set_track_staves(TrackIndex::new(5), TrackStaves::TAB_ONLY)
            .is_err()
    );
    assert_unchanged(&before, &song);
}

#[test]
fn the_master_mix_changes_and_undo_restores_it() {
    let mut song = song();
    assert_eq!(song.master_mix(), MasterMix::default());
    let volume = song.set_master_volume(90).unwrap();
    let pan = song.set_master_pan(20).unwrap();
    assert_eq!(
        song.master_mix(),
        MasterMix {
            volume: 90,
            pan: 20
        }
    );
    song.revert(&pan).unwrap();
    assert_eq!(
        song.master_mix(),
        MasterMix {
            volume: 90,
            pan: 64
        }
    );
    song.revert(&volume).unwrap();
    assert_eq!(song.master_mix(), MasterMix::default());
    assert!(song.set_master_volume(128).is_err());
    assert!(song.set_master_pan(200).is_err());
}

#[test]
fn the_master_scales_volumes_and_shifts_pans() {
    let sound = TrackSound {
        program: 25,
        volume: 100,
        pan: 100,
        reverb: 40,
        chorus: 0,
        percussion: false,
    };
    assert_eq!(
        MasterMix::default().apply(sound),
        sound,
        "neutral by default"
    );
    assert_eq!(MasterMix::default().volume, MASTER_UNITY);
    let half = MASTER_UNITY / 2;
    let played = MasterMix {
        volume: half,
        pan: 84,
    }
    .apply(sound);
    assert_eq!((played.volume, played.pan), (49, 120));
    let played = MasterMix {
        volume: 0,
        pan: 127,
    }
    .apply(sound);
    assert_eq!((played.volume, played.pan), (0, 127), "pan is clamped");
    let played = MasterMix {
        volume: 127,
        pan: 0,
    }
    .apply(sound);
    assert_eq!(played.volume, 127, "a boost stops at the MIDI maximum");
    assert_eq!(played.pan, 36);
}

#[test]
fn guitar_pros_default_master_is_neutral() {
    // GP7 files store an untouched master fader as 0.76.
    assert_eq!(unit_to_midi(0.76), MASTER_UNITY);
}

#[test]
fn fader_positions_map_to_midi_values() {
    assert_eq!(unit_to_midi(0.5), 64);
    assert_eq!(unit_to_midi(0.76), 97);
    assert_eq!(unit_to_midi(0.0), 0);
    assert_eq!(unit_to_midi(1.0), 127);
    assert_eq!(unit_to_midi(2.0), 127);
    assert!((midi_to_unit(64) - 0.5).abs() < f32::EPSILON);
    for value in 0..=127 {
        assert_eq!(unit_to_midi(midi_to_unit(value)), value);
    }
}

#[test]
fn a_clef_change_is_set_and_undone() {
    use crate::style::Clef;
    let mut song = song();
    let track = TrackIndex::new(0);
    let before = song.clone();
    assert_eq!(
        song.track(track)
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .clef(),
        None,
        "a synthetic song stores no clef"
    );
    let change = song
        .set_clef(track, MeasureIndex::new(0), Clef::Bass)
        .unwrap();
    assert_eq!(
        song.track(track)
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .clef(),
        Some(Clef::Bass)
    );
    song.revert(&change).unwrap();
    assert_eq!(
        song.track(track)
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .clef(),
        None
    );
    assert!(differences(&before, &song).is_empty());
}
