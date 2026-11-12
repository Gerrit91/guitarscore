//! The computed acoustic body: a strummed chord audibly coloured (the boxy mids well down
//! against the low end; a classical body darker than a dreadnought), the dreadnought close
//! to the owner's reference impulse response, moved by its size, ringing on
//! briefly and then silent, about as loud as the strings alone, and no body at all
//! changing nothing.

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

/// The level in each octave band (63 Hz to 8 kHz), dB, of `signal`'s first 16384 samples
/// (a windowed DFT's bins summed over the band).
fn bands(signal: &[f32]) -> [f32; 8] {
    let n = 16_384.min(signal.len());
    let part = &signal[..n];
    [
        63.0_f32, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0,
    ]
    .map(|centre| {
        let (low, high) = (centre / 2f32.sqrt(), centre * 2f32.sqrt());
        let mut power = 0.0_f64;
        let mut f = low;
        while f < high {
            let w = std::f64::consts::TAU * f64::from(f) / f64::from(RATE);
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &x) in part.iter().enumerate() {
                let hann = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                re += f64::from(x) * hann * (w * i as f64).cos();
                im += f64::from(x) * hann * (w * i as f64).sin();
            }
            power += re * re + im * im;
            f += RATE / n as f32;
        }
        10.0 * power.max(1e-30).log10() as f32
    })
}

/// `strings` through a body of `kind`.
fn through(kind: BodyKind, strings: &[f32]) -> Vec<f32> {
    let mut out = strings.to_vec();
    let mut body = Body::new(RATE);
    body.set(kind, 1.0, 0.5);
    for block in out.chunks_mut(BLOCK) {
        body.process(block);
    }
    out
}

/// How the body changes each octave band of `strings`, dB.
fn colour(kind: BodyKind, strings: &[f32]) -> [f32; 8] {
    let (dry, wet) = (
        bands(&strings[2_400..]),
        bands(&through(kind, strings)[2_400..]),
    );
    std::array::from_fn(|band| wet[band] - dry[band])
}

#[test]
fn a_body_audibly_colours_a_strummed_chord() {
    // The first body (narrow bells in series) changed no octave by more than 2 dB: the
    // owner heard no difference. A body is heard: the boxy mids well down against the low
    // end (the dreadnought fitted to the owner's reference impulse response).
    for (model, kind) in [
        (GuitarModel::Steel, BodyKind::Dreadnought),
        (GuitarModel::Nylon, BodyKind::Classical),
    ] {
        let strings = strum(model);
        let change = colour(kind, &strings);
        let low = change[0].max(change[1]).max(change[2]);
        let mids = change[3].min(change[4]);
        assert!(mids < -3.0, "{kind:?}: the boxy mids {change:?}");
        assert!(
            low > mids + 6.0,
            "{kind:?}: the low end over the mids {change:?}"
        );
        let db = 20.0 * (rms(&through(kind, &strings)) / rms(&strings)).log10();
        assert!(db.abs() < 2.0, "{kind:?} on {model:?}: {db:.1} dB");
    }
    // The two kinds differ: the classical body is much darker on top.
    let strings = strum(GuitarModel::Steel);
    let (dreadnought, classical) = (
        colour(BodyKind::Dreadnought, &strings),
        colour(BodyKind::Classical, &strings),
    );
    assert!(
        classical[7] < dreadnought[7] - 3.0 && classical[6] < dreadnought[6] - 3.0,
        "{dreadnought:?} against {classical:?}"
    );
}

#[test]
fn size_moves_the_resonances() {
    // A small body's air resonance is higher, a large one's lower.
    let kind = BodyKind::Dreadnought;
    assert!(
        lift(kind, 0.0, 125.0) > lift(kind, 0.5, 125.0) + 3.0,
        "small"
    );
    assert!(lift(kind, 1.0, 80.0) > lift(kind, 0.5, 80.0) + 3.0, "large");
}

#[test]
fn it_rings_on_after_the_strings_and_then_falls_silent() {
    for (model, kind) in [
        (GuitarModel::Steel, BodyKind::Dreadnought),
        (GuitarModel::Nylon, BodyKind::Classical),
    ] {
        let strings = strum(model);
        let mut body = Body::new(RATE);
        body.set(kind, 1.0, 0.5);
        let mut wet = strings.clone();
        for block in wet.chunks_mut(BLOCK) {
            body.process(block);
        }
        // The box rings on after the strings stop, then nothing is computed.
        let last = rms(&wet[wet.len() - 2_400..]);
        let mut tail = vec![0.0_f32; RATE as usize * 2];
        for block in tail.chunks_mut(BLOCK) {
            body.process(block);
        }
        let ring = 20.0 * (rms(&tail[..960]) / last).log10();
        assert!(ring > -20.0, "{kind:?}: it rings on: {ring:.1} dB");
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

#[test]
fn the_dreadnought_follows_the_owners_reference_body() {
    // The third-octave response of the owner's reference impulse response (a Furch
    // Vintage 1 dreadnought, measured 2026-11-12, relative to its mean; only these numbers
    // are taken from it), 40 Hz to 12.9 kHz.
    let reference = [
        -0.2_f32, -1.2, -1.1, 6.6, 10.1, 0.8, -0.2, 6.8, 6.7, -3.8, -15.6, -5.9, -8.7, -9.6, -12.5,
        -4.5, -0.5, -9.7, -0.3, 7.9, 14.3, 8.6, 8.2, 9.0, -2.3, -3.2,
    ];
    let ours: Vec<f32> = (0..reference.len())
        .map(|band| {
            let centre = 40.0 * 2f32.powf(band as f32 / 3.0);
            let lifts: Vec<f32> = (-4..4)
                .map(|step| {
                    lift(
                        BodyKind::Dreadnought,
                        0.5,
                        centre * 2f32.powf(step as f32 / 24.0),
                    )
                })
                .collect();
            lifts.iter().sum::<f32>() / lifts.len() as f32
        })
        .collect();
    let mean = |values: &[f32]| values.iter().sum::<f32>() / values.len() as f32;
    let (ours_mean, reference_mean) = (mean(&ours), mean(&reference));
    let error = (ours
        .iter()
        .zip(&reference)
        .map(|(o, r)| ((o - ours_mean) - (r - reference_mean)).powi(2))
        .sum::<f32>()
        / reference.len() as f32)
        .sqrt();
    assert!(
        error < 2.5,
        "{error:.1} dB from the reference on average: {ours:?}"
    );
}

#[test]
fn the_bodies_are_built_in_responses_for_a_cab_block() {
    use gscore_guitar::body_response;
    assert!(body_response(BodyKind::None).is_none());
    for kind in [BodyKind::Dreadnought, BodyKind::Classical] {
        let response = body_response(kind).unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&response, &body_response(kind).unwrap()),
            "shared"
        );
        let samples = response.mono_at_rate(48_000);
        let energy: f32 = samples.iter().map(|x| x * x).sum();
        assert!(
            (energy - 1.0).abs() < 0.01,
            "{kind:?}: unit energy {energy}"
        );
        assert_eq!(
            samples.last().copied(),
            Some(0.0),
            "{kind:?}: it fades to silence"
        );
    }
}
