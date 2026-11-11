//! An overdrive pedal, computed, after the classic green mid-hump pedals: what lies above
//! the Tight knob's low cut is driven into the clipping, the lows pass clean beside it.
//!
//! ```text
//!  in ─┬────────────────────────────────────────────────────┬─► + ─► tone ─► level
//!      └─► low cut (Tight: 90-720 Hz) ─► drive ─► [ 4× soft clip ] ─┘
//! ```
//!
//! On its own it is a warm, mild distortion. In front of a high-gain amp it is the metal
//! player's trick: Drive low and Level up, so the amp is hit hard in the mids. Tight
//! sets how much of the low end goes along: at 1 the classic pedal's 720 Hz, a tight but
//! thin chug (the clipped mids then stand far above the clean lows); lower keeps the low
//! mids and the chug's body in the drive.

use crate::amp::{INPUT_PAD, soft};
use crate::filters::Biquad;
use crate::oversample::{FACTOR, Oversampler};

/// The Tight knob's range: the clipping path's low cut, Hz, from 0 (loose: the lows are
/// driven too) to 1 (the classic pedal's 720 Hz). Below it the lows pass the clipping by.
const TIGHT_HZ: (f32, f32) = (90.0, 720.0);
/// The Drive knob's range in dB of the clipping path's gain.
const DRIVE_DB: (f32, f32) = (6.0, 42.0);
/// The Tone knob's range: the lowpass after the pedal, Hz.
const TONE_HZ: (f32, f32) = (900.0, 7_000.0);
/// The clipping's own high cut at the fast rate (the pedal's feedback capacitor), Hz.
const SMOOTH: f32 = 6_500.0;
/// The output's trim, so Level 0 dB at the default Drive is about as loud as the strings
/// without the pedal, as it was before the input was recalibrated (measured on a hard
/// chord; -4 dB until 2026-11-11).
const TRIM_DB: f32 = -12.1;

/// The knobs of a drive block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriveKnobs {
    /// How hard the clipping is driven, 0 to 1.
    pub drive: f32,
    /// The tone, 0 dark to 1 bright.
    pub tone: f32,
    /// The output level in dB.
    pub level_db: f32,
    /// How much of the low end stays out of the clipping, 0 loose to 1 tight (see
    /// [`TIGHT_HZ`]).
    pub tight: f32,
}

impl Default for DriveKnobs {
    fn default() -> Self {
        DriveKnobs {
            drive: 0.3,
            tone: 0.5,
            level_db: 0.0,
            tight: 0.5,
        }
    }
}

/// One channel of the pedal.
#[derive(Debug, Clone)]
pub(crate) struct Drive {
    rate: f32,
    gain: f32,
    out: f32,
    hump: Biquad,
    smooth: Biquad,
    tone: Biquad,
    oversampler: Oversampler,
}

impl Drive {
    /// A pedal for `sample_rate` with `knobs`. Allocates nothing.
    pub(crate) fn new(sample_rate: f32, knobs: DriveKnobs) -> Self {
        let rate = sample_rate.max(1.0);
        let mut drive = Drive {
            rate,
            gain: 1.0,
            out: 1.0,
            hump: Biquad::default(),
            smooth: Biquad::default(),
            tone: Biquad::default(),
            oversampler: Oversampler::new(rate),
        };
        drive.set_knobs(knobs);
        drive
    }

    /// New knob values, keeping what it is playing.
    pub(crate) fn set_knobs(&mut self, knobs: DriveKnobs) {
        let rate = self.rate;
        let share = knobs.drive.clamp(0.0, 1.0);
        self.gain = 10.0_f32.powf((DRIVE_DB.0 + (DRIVE_DB.1 - DRIVE_DB.0) * share) / 20.0);
        self.out = 10.0_f32.powf((TRIM_DB + knobs.level_db) / 20.0) / INPUT_PAD;
        // A first-order-like low cut: gentle, as the pedal's is.
        let tight = knobs.tight.clamp(0.0, 1.0);
        let hump = TIGHT_HZ.0 * (TIGHT_HZ.1 / TIGHT_HZ.0).powf(tight);
        self.hump = self.hump.retuned(Biquad::highpass(rate, hump, 0.5));
        self.smooth = self
            .smooth
            .retuned(Biquad::lowpass(rate * FACTOR as f32, SMOOTH, 0.6));
        let tone = TONE_HZ.0 * (TONE_HZ.1 / TONE_HZ.0).powf(knobs.tone.clamp(0.0, 1.0));
        self.tone = self.tone.retuned(Biquad::lowpass(rate, tone, 0.6));
    }

    /// Silence in every filter.
    pub(crate) fn reset(&mut self) {
        for filter in [&mut self.hump, &mut self.smooth, &mut self.tone] {
            filter.reset();
        }
        self.oversampler.reset();
    }

    /// One sample through the pedal.
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let x = x * INPUT_PAD;
        let driven = self.hump.process(x) * self.gain;
        let smooth = &mut self.smooth;
        // Symmetric, as two diodes clip: odd harmonics, the pedal's edge.
        let clipped = self
            .oversampler
            .run(driven, |value| smooth.process(soft(value)));
        self.tone.process(x + clipped) * self.out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    /// The level of `frequency` (Hz) in `signal` at `rate` (one DFT bin), for tests.
    fn tone_level(signal: &[f32], frequency: f32, rate: f32) -> f32 {
        let (mut re, mut im) = (0.0_f32, 0.0_f32);
        for (i, x) in signal.iter().enumerate() {
            let phase = TAU * frequency * i as f32 / rate;
            re += x * phase.cos();
            im += x * phase.sin();
        }
        2.0 * re.hypot(im) / signal.len().max(1) as f32
    }

    fn sine(frequency: f32, level: f32) -> Vec<f32> {
        (0..24_000)
            .map(|i| level * (TAU * frequency * i as f32 / RATE).sin())
            .collect()
    }

    fn run(knobs: DriveKnobs, input: &[f32]) -> Vec<f32> {
        let mut drive = Drive::new(RATE, knobs);
        input.iter().map(|&x| drive.process(x)).collect()
    }

    fn distortion(signal: &[f32], frequency: f32) -> f32 {
        let tail = &signal[12_000..];
        let harmonics: f32 = (2..8)
            .map(|h| tone_level(tail, frequency * h as f32, RATE))
            .sum();
        harmonics / tone_level(tail, frequency, RATE).max(1e-9)
    }

    /// A string's level: a hard chord sustains at about 0.04 RMS.
    const STRING: f32 = 0.06;

    #[test]
    fn tight_keeps_the_lows_clean_and_loose_drives_them() {
        let full = |tight: f32| DriveKnobs {
            drive: 1.0,
            tone: 1.0,
            tight,
            ..DriveKnobs::default()
        };
        // An open low E (82 Hz) against a note in the mids (1 kHz's neighbour, 880 Hz).
        let low = |tight| distortion(&run(full(tight), &sine(82.0, STRING)), 82.0);
        let mid = distortion(&run(full(1.0), &sine(880.0, STRING)), 880.0);
        assert!(mid > 3.0 * low(1.0), "tight: mids {mid}, lows {}", low(1.0));
        assert!(low(0.0) > 3.0 * low(1.0), "loose: the lows are driven too");
        // And more drive, more distortion.
        let little = DriveKnobs {
            drive: 0.0,
            tone: 1.0,
            ..DriveKnobs::default()
        };
        assert!(mid > distortion(&run(little, &sine(880.0, STRING)), 880.0));
    }

    #[test]
    fn about_as_loud_as_without_it_and_silent_in_silence() {
        let input = sine(330.0, STRING);
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        let ratio = rms(&run(DriveKnobs::default(), &input)[12_000..]) / rms(&input);
        assert!((0.5..2.0).contains(&ratio), "{ratio}");
        assert!(
            run(DriveKnobs::default(), &[0.0; 4_800])
                .iter()
                .all(|x| x.abs() < 1e-6)
        );
    }
}
