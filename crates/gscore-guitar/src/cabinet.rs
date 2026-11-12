//! The built-in cabinet: a computed impulse response for a Cabinet block without a file of
//! its own, modelled after the owner's reference, a Marshall 1960A 4×12 with Celestion
//! G12M speakers and an MD 421 at the cap (measured 2026-11-12; only its figures were
//! taken from it). A file loaded into the block replaces it.
//!
//! The response is a chain of filters fitted to the reference, rendered once into an
//! impulse response and played like a loaded one:
//!
//! - the closed back's low end, rolling off below about 90 Hz, with a little thump;
//! - a broad body from 150 to 400 Hz;
//! - a dip around 1-1.3 kHz;
//! - the G12M's presence from 2.4 to 4 kHz, the microphone on the cap;
//! - a steep fall above 5 kHz, which keeps distortion from fizzing;
//! - narrow peaks of the cone breaking up (620 Hz to 4.6 kHz), ringing briefly.

use std::sync::{Arc, LazyLock};

use crate::filters::Biquad;
use crate::impulse::ImpulseResponse;

/// The rate the response is rendered at (the engine resamples it like a loaded file).
const RATE: u32 = 48_000;
/// Its length: as long as the reference.
const LENGTH: usize = 9_600;

/// One filter of the model.
#[derive(Debug, Clone, Copy)]
enum Part {
    HighPass(f32, f32),
    LowPass(f32, f32),
    Bell(f32, f32, f32),
    HighShelf(f32, f32),
}

/// The model, fitted to the reference's third-octave response.
const PARTS: [Part; 17] = [
    // The closed back: a low cut with a little resonance, and its thump.
    Part::HighPass(95.0, 0.8),
    Part::Bell(120.0, 1.4, 1.0),
    // The body, the dip, the presence.
    Part::Bell(250.0, 0.7, 7.0),
    Part::Bell(1_100.0, 2.0, -12.0),
    Part::Bell(2_700.0, 1.0, 8.0),
    Part::Bell(3_500.0, 2.5, 2.0),
    // The steep fall above 5 kHz (fourth order), levelling off quietly above 10 kHz.
    Part::LowPass(4_800.0, 0.541_196),
    Part::LowPass(4_800.0, 1.306_563),
    Part::HighShelf(11_000.0, 16.0),
    // The cone breaking up.
    Part::Bell(622.0, 11.0, 2.5),
    Part::Bell(783.0, 7.0, 3.0),
    Part::Bell(1_046.0, 17.0, 5.0),
    Part::Bell(1_437.0, 11.0, 2.0),
    Part::Bell(1_709.0, 7.0, 3.5),
    Part::Bell(3_320.0, 7.0, 3.0),
    Part::Bell(4_561.0, 11.0, 3.0),
    Part::Bell(5_270.0, 1.4, 2.0),
];

/// The model's filters at `rate`.
fn filters(rate: f32) -> Vec<Biquad> {
    PARTS
        .iter()
        .map(|&part| match part {
            Part::HighPass(frequency, q) => Biquad::highpass(rate, frequency, q),
            Part::LowPass(frequency, q) => Biquad::lowpass(rate, frequency, q),
            Part::Bell(frequency, q, gain) => Biquad::peaking(rate, frequency, q, gain),
            Part::HighShelf(frequency, gain) => Biquad::shelf(rate, frequency, gain, true),
        })
        .collect()
}

/// The built-in cabinet's impulse response (made once, shared).
pub fn built_in_cabinet() -> Arc<ImpulseResponse> {
    static CABINET: LazyLock<Option<Arc<ImpulseResponse>>> = LazyLock::new(|| {
        let mut chain = filters(RATE as f32);
        let samples: Vec<f32> = (0..LENGTH)
            .map(|index| {
                let x = if index == 0 { 1.0 } else { 0.0 };
                let y = chain
                    .iter_mut()
                    .fold(x, |value, filter| filter.process(value));
                // A short fade over the last tenth, so the response ends at silence.
                let left = (LENGTH - index) as f32 / (LENGTH as f32 * 0.1);
                y * left.min(1.0)
            })
            .collect();
        ImpulseResponse::from_samples(&samples, RATE)
            .ok()
            .map(Arc::new)
    });
    CABINET.clone().unwrap_or_else(|| {
        // The rendering above cannot fail for these filters; should it ever, a plain
        // impulse (no colour at all) keeps the block playing.
        Arc::new(
            ImpulseResponse::from_samples(&[1.0, 0.0], RATE)
                .unwrap_or_else(|_| unreachable_response()),
        )
    })
}

/// Never reached: a two-sample impulse is a valid response.
fn unreachable_response() -> ImpulseResponse {
    ImpulseResponse::from_samples(&[1.0], RATE).unwrap_or_else(|_| unreachable_response())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    /// The model's level at `frequency`, dB.
    fn response(frequency: f32) -> f32 {
        let gain: f32 = filters(RATE as f32)
            .iter()
            .map(|filter| filter.gain_at(RATE as f32, frequency))
            .product();
        20.0 * gain.max(1e-9).log10()
    }

    #[test]
    fn it_follows_the_owners_reference_cabinet() {
        // The reference's third-octave response (relative to its mean), 40 Hz to 12.9 kHz.
        let reference = [
            -11.1_f32, -7.5, -2.9, 0.3, 3.7, 6.4, 6.9, 8.9, 9.4, 7.9, 7.5, 6.0, 3.5, 1.4, -5.5,
            -4.5, 3.9, 5.1, 11.3, 10.6, 8.4, 5.1, -8.5, -15.4, -22.1, -22.3,
        ];
        let ours: Vec<f32> = (0..reference.len())
            .map(|band| {
                let centre = 40.0 * 2f32.powf(band as f32 / 3.0);
                (-4..4)
                    .map(|step| response(centre * 2f32.powf(step as f32 / 24.0)))
                    .sum::<f32>()
                    / 8.0
            })
            .collect();
        let mean = |values: &[f32]| values.iter().sum::<f32>() / values.len() as f32;
        let (ours_mean, reference_mean) = (mean(&ours), mean(&reference));
        for (band, (o, r)) in ours.iter().zip(&reference).enumerate() {
            eprintln!(
                "{:7.0} Hz ours {:+6.1} reference {:+6.1}",
                40.0 * 2f32.powf(band as f32 / 3.0),
                o - ours_mean,
                r - reference_mean
            );
        }
        let error = (ours
            .iter()
            .zip(&reference)
            .map(|(o, r)| ((o - ours_mean) - (r - reference_mean)).powi(2))
            .sum::<f32>()
            / reference.len() as f32)
            .sqrt();
        eprintln!("{error:.1} dB from the reference on average");
        assert!(error < 2.5, "{error:.1} dB from the reference on average");
    }

    #[test]
    fn the_response_is_made_once_at_unit_energy_and_dies_away() {
        let cabinet = built_in_cabinet();
        assert!(Arc::ptr_eq(&cabinet, &built_in_cabinet()), "shared");
        let samples = cabinet.mono_at_rate(RATE);
        assert_eq!(samples.len(), LENGTH);
        let energy: f32 = samples.iter().map(|x| x * x).sum();
        assert!((energy - 1.0).abs() < 0.01, "unit energy: {energy}");
        let tail: f32 = samples[LENGTH / 2..].iter().map(|x| x * x).sum();
        assert!(tail < 1e-4, "it has died away by 100 ms: {tail}");
    }
}
