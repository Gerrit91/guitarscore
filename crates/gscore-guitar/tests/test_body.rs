//! The computed acoustic body: its air and top resonances where a dreadnought's and a
//! classical guitar's are, moved by its size, ringing on briefly and then silent, about as
//! loud as the strings alone, and no body at all changing nothing.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::f32::consts::TAU;

use gscore_guitar::{Attack, Body, BodyKind, GuitarModel, Pluck, StringVoice, frequency};

const RATE: f32 = 48_000.0;
const BLOCK: usize = 64;

fn rms(signal: &[f32]) -> f32 {
    (signal.iter().map(|x| x * x).sum::<f32>() / signal.len().max(1) as f32).sqrt()
}

/// How much `body` lifts a steady sine at `hz`, dB.
fn lift(kind: BodyKind, size: f32, hz: f32) -> f32 {
    let mut body = Body::new(RATE);
    body.set(kind, 1.0, size);
    let mut signal: Vec<f32> = (0..24_000)
        .map(|i| 0.1 * (TAU * hz * i as f32 / RATE).sin())
        .collect();
    let dry = rms(&signal[12_000..]);
    for block in signal.chunks_mut(BLOCK) {
        body.process(block);
    }
    20.0 * (rms(&signal[12_000..]) / dry).log10()
}

/// An open E major chord strummed on `model`'s strings, one second, mono.
fn strum(model: GuitarModel) -> Vec<f32> {
    let strings = model.strings();
    let mut voices: Vec<StringVoice> = [40_u8, 47, 52, 56, 59, 64]
        .iter()
        .map(|&key| {
            let mut voice = StringVoice::new(RATE, 20.0, strings);
            voice.pluck(Pluck {
                frequency: frequency(key, 0.0),
                velocity: 0.8,
                attack: Attack::Pick,
                palm_mute: false,
                dead: false,
                harmonic: false,
            });
            voice
        })
        .collect();
    let mut out = vec![0.0_f32; RATE as usize];
    for block in out.chunks_mut(BLOCK) {
        for voice in &mut voices {
            voice.render(block);
        }
    }
    out
}

#[test]
fn the_air_and_the_top_ring_where_a_guitar_s_do() {
    for (kind, air, top) in [
        (BodyKind::Dreadnought, 100.0, 195.0),
        (BodyKind::Classical, 96.0, 188.0),
    ] {
        let floor = lift(kind, 0.5, 150.0);
        assert!(
            lift(kind, 0.5, air) > floor + 6.0,
            "{kind:?}: the air resonance"
        );
        assert!(lift(kind, 0.5, top) > floor + 6.0, "{kind:?}: the top");
        assert!(
            lift(kind, 0.5, 50.0) < floor,
            "{kind:?}: little below the air"
        );
    }
}

#[test]
fn size_moves_the_resonances() {
    // A small body's air resonance is higher, a large one's lower.
    let kind = BodyKind::Dreadnought;
    assert!(
        lift(kind, 0.0, 125.0) > lift(kind, 0.5, 125.0) + 6.0,
        "small"
    );
    assert!(lift(kind, 1.0, 80.0) > lift(kind, 0.5, 80.0) + 6.0, "large");
}

#[test]
fn about_as_loud_as_the_strings_and_it_rings_out() {
    for (model, kind) in [
        (GuitarModel::Steel, BodyKind::Dreadnought),
        (GuitarModel::Nylon, BodyKind::Classical),
    ] {
        let dry = strum(model);
        let mut wet = dry.clone();
        let mut body = Body::new(RATE);
        body.set(kind, 1.0, 0.5);
        for block in wet.chunks_mut(BLOCK) {
            body.process(block);
        }
        let db = 20.0 * (rms(&wet) / rms(&dry)).log10();
        assert!(db.abs() < 2.0, "{kind:?} on {model:?}: {db:.1} dB");
        // After the strings stop, the body rings briefly, then nothing is computed.
        let mut tail = vec![0.0_f32; RATE as usize * 2];
        for block in tail.chunks_mut(BLOCK) {
            body.process(block);
        }
        assert!(rms(&tail[..2_400]) > 1e-5, "{kind:?}: it rings on");
        assert!(
            rms(&tail[RATE as usize..]) < 1e-6,
            "{kind:?}: then it is silent"
        );
    }
}

#[test]
fn no_body_or_no_mix_changes_nothing() {
    let dry = strum(GuitarModel::Steel);
    for (kind, mix) in [(BodyKind::None, 1.0), (BodyKind::Dreadnought, 0.0)] {
        let mut out = dry.clone();
        let mut body = Body::new(RATE);
        body.set(kind, mix, 0.5);
        assert!(!body.is_on());
        for block in out.chunks_mut(BLOCK) {
            body.process(block);
        }
        assert_eq!(out, dry, "{kind:?} {mix}");
    }
    // Broken values do not break the sound.
    let mut body = Body::new(RATE);
    body.set(BodyKind::Classical, f32::NAN, f32::INFINITY);
    let mut out = dry.clone();
    for block in out.chunks_mut(BLOCK) {
        body.process(block);
    }
    assert!(out.iter().all(|x| x.is_finite()));
}
