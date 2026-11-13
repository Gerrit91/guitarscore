//! The safety limiter at the very end of the mix: it keeps the output below full scale
//! instead of letting an over-loud mix be hard-clipped by the device (or the browser).
//!
//! A peak limiter without look-ahead, the same for both sides (so the stereo image stays
//! put): when a frame would go over [`CEILING`], the gain drops at once to exactly what
//! keeps it there, holds for [`HOLD_SECONDS`], then recovers with [`RELEASE_SECONDS`].
//! Below the ceiling the gain is exactly 1, so a mix that never goes over passes through
//! bit for bit, and it adds no latency. A sample that is not a number becomes silence.

/// The highest output level: -0.5 dBFS.
pub(crate) const CEILING: f32 = 0.944;
/// How long the gain stays down after a peak, so a low note's cycles do not each pump it.
const HOLD_SECONDS: f32 = 0.01;
/// How fast the gain recovers afterwards (time constant).
const RELEASE_SECONDS: f32 = 0.15;
/// Closer to 1 than this (-0.001 dB), the gain is 1 again and the output bit-exact.
const UNITY_SNAP: f32 = 1.0e-4;

/// The limiter's state.
#[derive(Debug, Clone)]
pub(crate) struct Limiter {
    /// How far the gain is below 1 (0: unity). Kept this way round so the recovery,
    /// a shrinking factor, does not stall in `f32` just below 1.
    deficit: f32,
    hold_left: u32,
    hold: u32,
    /// The factor the deficit shrinks by per frame while recovering.
    release: f32,
}

impl Limiter {
    /// A limiter for `sample_rate`, at rest (gain 1).
    pub(crate) fn new(sample_rate: u32) -> Self {
        let rate = sample_rate.max(1) as f32;
        Limiter {
            deficit: 0.0,
            hold_left: 0,
            hold: (HOLD_SECONDS * rate) as u32,
            release: (-1.0 / (RELEASE_SECONDS * rate)).exp(),
        }
    }

    /// Limits one block in place (the sides as far as both reach).
    pub(crate) fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if !l.is_finite() {
                *l = 0.0;
            }
            if !r.is_finite() {
                *r = 0.0;
            }
            let peak = l.abs().max(r.abs());
            let needed = if peak > CEILING {
                1.0 - CEILING / peak
            } else {
                0.0
            };
            let recovered = if self.hold_left > 0 {
                self.hold_left -= 1;
                self.deficit
            } else {
                let deficit = self.deficit * self.release;
                if deficit < UNITY_SNAP { 0.0 } else { deficit }
            };
            if needed > recovered {
                self.deficit = needed;
                self.hold_left = self.hold;
            } else {
                self.deficit = recovered;
            }
            if self.deficit > 0.0 {
                // The clamp only catches rounding (a last bit over the ceiling).
                let gain = 1.0 - self.deficit;
                *l = (*l * gain).clamp(-CEILING, CEILING);
                *r = (*r * gain).clamp(-CEILING, CEILING);
            }
        }
    }

    /// How far it turns the mix down now, in dB (0 or below).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reduction_db(&self) -> f32 {
        20.0 * (1.0 - self.deficit).max(1.0e-6).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                amplitude * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / RATE as f32).sin()
            })
            .collect()
    }

    #[test]
    fn a_mix_below_the_ceiling_passes_bit_for_bit() {
        let mut limiter = Limiter::new(RATE);
        let input = sine(0.9, 4_800);
        let (mut left, mut right) = (input.clone(), input.clone());
        limiter.process(&mut left, &mut right);
        assert_eq!(left, input);
        assert_eq!(right, input);
        assert_eq!(limiter.reduction_db(), 0.0);
    }

    #[test]
    fn an_over_loud_mix_stays_below_the_ceiling_on_both_sides_alike() {
        let mut limiter = Limiter::new(RATE);
        let mut left = sine(4.0, 4_800);
        let mut right: Vec<f32> = left.iter().map(|s| 0.5 * s).collect();
        limiter.process(&mut left, &mut right);
        assert!(left.iter().chain(&right).all(|s| s.abs() <= CEILING));
        let loudest = left.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            loudest > 0.9 * CEILING,
            "turned down only as far as needed: {loudest}"
        );
        // Linked: the right side keeps half the left's level, sample by sample.
        for (l, r) in left.iter().zip(&right) {
            assert!((0.5 * l - r).abs() < 1.0e-6);
        }
        assert!(limiter.reduction_db() < -12.0);
    }

    #[test]
    fn the_gain_recovers_to_exactly_one_after_a_peak() {
        let mut limiter = Limiter::new(RATE);
        let (mut left, mut right) = (vec![2.0_f32; 64], vec![0.0_f32; 64]);
        limiter.process(&mut left, &mut right);
        assert!(limiter.reduction_db() < -6.0);
        // Two seconds later: back at unity, so quiet audio is untouched.
        let quiet = sine(0.3, 2 * RATE as usize);
        let (mut left, mut right) = (quiet.clone(), quiet.clone());
        limiter.process(&mut left, &mut right);
        assert_eq!(limiter.reduction_db(), 0.0);
        assert_eq!(left.last(), quiet.last());
    }

    #[test]
    fn samples_that_are_not_numbers_become_silence() {
        let mut limiter = Limiter::new(RATE);
        let (mut left, mut right) = (vec![f32::NAN, f32::INFINITY, 0.5], vec![0.25; 3]);
        limiter.process(&mut left, &mut right);
        assert_eq!(left, vec![0.0, 0.0, 0.5]);
        assert_eq!(right, vec![0.25; 3]);
    }
}
