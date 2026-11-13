//! Effects as the synthesizer plays them, rendered offline with a sine SoundFont: a
//! vibrato changes the pitch, staccato and palm mute stop the sound early.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{SoundBank, SynthEngine};
use gscore_model::testing::{self, TestNote};
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, NoteEffect, StringNumber, TrackIndex, Vibrato,
    VoiceIndex,
};
use gscore_play::Sequence;

const RATE: u32 = 44_100;

fn at() -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    }
}

/// Renders the excerpt of the first beat (a half note on string 5, fret 0: A2) with
/// `effect`, mono.
fn render(effect: Option<NoteEffect>, seconds: f64) -> Vec<f32> {
    let mut song = testing::song_with_notes(&[(2, &[TestNote::new(5, 0)])]).unwrap();
    if let Some(effect) = effect {
        let _ = song
            .set_note_effect(at(), StringNumber::new(5).unwrap(), effect)
            .unwrap();
    }
    let excerpt = Sequence::beat_excerpt(&song, at()).unwrap().unwrap();
    let bank = SoundBank::read(&mut sine_soundfont("Sine").as_slice()).unwrap();
    let mut engine = SynthEngine::new(&bank, RATE).unwrap();
    let _ = engine.load(Arc::new(excerpt));
    engine.play();
    let frames = (seconds * f64::from(RATE)) as usize;
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    engine.render(&mut left, &mut right);
    left.iter()
        .zip(&right)
        .map(|(l, r)| 0.5 * (l + r))
        .collect()
}

/// The frequency of each period, from interpolated upward zero crossings, skipping the
/// note's first 50 ms (its attack).
fn frequencies(samples: &[f32]) -> Vec<f64> {
    let skip = (0.05 * f64::from(RATE)) as usize;
    let crossings: Vec<f64> = samples
        .windows(2)
        .enumerate()
        .skip(skip)
        .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0 && w[1] - w[0] > 1e-4)
        .map(|(index, w)| index as f64 + f64::from(-w[0] / (w[1] - w[0])))
        .collect();
    crossings
        .windows(2)
        .map(|pair| f64::from(RATE) / (pair[1] - pair[0]))
        .collect()
}

/// The loudest sample from `from` seconds on.
fn loudness_after(samples: &[f32], from: f64) -> f32 {
    let start = (from * f64::from(RATE)) as usize;
    samples[start.min(samples.len())..]
        .iter()
        .fold(0.0, |max, s| max.max(s.abs()))
}

#[test]
fn a_vibrato_is_heard_as_a_changing_pitch() {
    let spread = |freqs: &[f64]| {
        let (low, high) = freqs
            .iter()
            .fold((f64::MAX, 0.0_f64), |(lo, hi), &f| (lo.min(f), hi.max(f)));
        high / low
    };
    let plain = frequencies(&render(None, 0.8));
    let shaking = frequencies(&render(
        Some(NoteEffect::Vibrato(Some(Vibrato::Slight))),
        0.8,
    ));
    eprintln!(
        "plain {:.4}, vibrato {:.4}",
        spread(&plain),
        spread(&shaking)
    );
    assert!(spread(&plain) < 1.02, "a plain note holds its pitch");
    // About a quarter tone between the highest and lowest pitch (±25 cents: a ratio of
    // 1.029).
    assert!(spread(&shaking) > 1.022, "the vibrato is not heard");
}

#[test]
fn staccato_stops_the_sound_early() {
    // A half note at 120 BPM lasts a second; staccato stops it dead after an eighth
    // (0.25 s), with no release tail.
    let plain = render(None, 1.2);
    let short = render(Some(NoteEffect::Staccato(true)), 1.2);
    eprintln!(
        "after 0.27 s: plain {}, staccato {}",
        loudness_after(&plain, 0.27),
        loudness_after(&short, 0.27)
    );
    assert!(loudness_after(&short, 0.1) > 0.05, "the note sounds first");
    assert!(loudness_after(&plain, 0.27) > 0.05);
    assert!(
        loudness_after(&short, 0.27) < 0.005,
        "the staccato note rings on"
    );
}
