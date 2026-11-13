//! The safety limiter at the end of the engine's mix: an over-loud song stays below full
//! scale (instead of being hard-clipped by the device), and a normal one is untouched.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{SoundBank, SynthEngine};
use gscore_model::TrackIndex;
use gscore_model::testing::{self, TestNote};
use gscore_play::Sequence;

const RATE: u32 = 48_000;

/// The loudest sample a full-scale output may have: -0.5 dBFS.
const CEILING: f32 = 0.944;

/// `tracks` tracks each playing a whole note A3 at `volume`, rendered for a second:
/// the output and the master meter's reading.
fn play(tracks: usize, volume: u8) -> (Vec<f32>, Vec<f32>, f32) {
    let mut song =
        testing::song_with_notes_on_tracks(tracks, &[(1, &[TestNote::new(3, 2)])]).unwrap();
    for track in 0..tracks {
        let _ = song
            .set_track_volume(TrackIndex::new(track), volume)
            .unwrap();
    }
    let bank = SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap();
    let mut engine = SynthEngine::new(&bank, RATE).unwrap();
    engine.load(Arc::new(Sequence::new(&song, None).unwrap()));
    engine.play();
    let (mut left, mut right) = (vec![0.0_f32; RATE as usize], vec![0.0_f32; RATE as usize]);
    engine.render(&mut left, &mut right);
    let meter = engine.meters().master().mark_left;
    (left, right, meter)
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()))
}

#[test]
fn an_over_loud_mix_stays_below_full_scale() {
    let (left, right, meter) = play(16, 127);
    // Sixteen loud tracks in unison: the mix itself goes well over full scale, and the
    // master meter shows that.
    assert!(meter > 1.2, "the meter shows the over: {meter}");
    // What is heard stays below the ceiling, and is not turned down further than needed.
    let (left_peak, right_peak) = (peak(&left), peak(&right));
    assert!(
        left_peak <= CEILING && right_peak <= CEILING,
        "{left_peak} {right_peak}"
    );
    assert!(left_peak > 0.9 * CEILING, "{left_peak}");
    assert!(left.iter().chain(&right).all(|x| x.is_finite()));
}

#[test]
fn a_mix_below_full_scale_is_not_touched() {
    // One quiet track: its level is that of the same track at half the volume, scaled by
    // the square law, so nothing in the output path changed it.
    let (full, _, _) = play(1, 64);
    let (half, _, _) = play(1, 32);
    assert!(peak(&full) < CEILING);
    let ratio = peak(&half) / peak(&full);
    let expected = (32.0_f32 / 64.0).powi(2);
    assert!(
        (ratio - expected).abs() < 0.02 * expected,
        "{ratio} against {expected}"
    );
}
