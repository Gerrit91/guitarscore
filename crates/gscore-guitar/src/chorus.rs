//! A stereo chorus: two short delay lines whose delay swings slowly, one side a quarter
//! cycle ahead of the other, mixed under the dry signal. A cheap chain block mounted per
//! instrument (the owner's design: the chorus is not a mixer bus).
//!
//! Ring-buffer index arithmetic only, bounded by the buffer length: the module allows the
//! arithmetic lint for that (no value can leave the buffer, and nothing panics).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use std::f32::consts::TAU;

/// The knobs of a chorus block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChorusKnobs {
    /// How fast the delay swings, Hz.
    pub rate_hz: f32,
    /// How far it swings, ms around [`CENTRE_MS`].
    pub depth_ms: f32,
    /// The share of the delayed signal, 0 dry to 1 only the chorus.
    pub mix: f32,
}

/// The middle delay the LFO swings around, ms.
pub const CENTRE_MS: f32 = 14.0;

impl Default for ChorusKnobs {
    fn default() -> Self {
        ChorusKnobs {
            rate_hz: 0.6,
            depth_ms: 3.0,
            mix: 0.5,
        }
    }
}

impl ChorusKnobs {
    /// Every value in its range (a hand-edited settings file can hold anything).
    pub fn sanitized(self) -> Self {
        let fit = |value: f32, (low, high): (f32, f32), fallback: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        let default = ChorusKnobs::default();
        ChorusKnobs {
            rate_hz: fit(
                self.rate_hz,
                crate::knob_ranges::CHORUS_RATE,
                default.rate_hz,
            ),
            depth_ms: fit(
                self.depth_ms,
                crate::knob_ranges::CHORUS_DEPTH,
                default.depth_ms,
            ),
            mix: fit(self.mix, crate::knob_ranges::MIX, default.mix),
        }
    }
}

/// One side's delay line.
#[derive(Debug, Clone)]
struct Line {
    buffer: Vec<f32>,
    at: usize,
    /// The LFO's phase, 0 to 1.
    phase: f32,
}

impl Line {
    fn new(frames: usize) -> Self {
        Line {
            buffer: vec![0.0; frames.max(4)],
            at: 0,
            phase: 0.0,
        }
    }

    /// Writes `x` and reads the delay `delay` samples back (linear interpolation).
    fn process(&mut self, x: f32, delay: f32) -> f32 {
        let length = self.buffer.len();
        if let Some(slot) = self.buffer.get_mut(self.at) {
            *slot = x;
        }
        self.at = (self.at.wrapping_add(1)) % length;
        let delay = delay.clamp(1.0, (length.saturating_sub(2)) as f32);
        let whole = delay.floor();
        let fraction = delay - whole;
        let index = (whole as usize).min(length.saturating_sub(1));
        let first = (self.at.wrapping_add(length).wrapping_sub(index)) % length;
        let second = (first.wrapping_add(length).wrapping_sub(1)) % length;
        let a = self.buffer.get(first).copied().unwrap_or(0.0);
        let b = self.buffer.get(second).copied().unwrap_or(0.0);
        a * (1.0 - fraction) + b * fraction
    }
}

/// The chorus of one chain block.
#[derive(Debug, Clone)]
pub(crate) struct Chorus {
    rate: f32,
    knobs: ChorusKnobs,
    left: Line,
    right: Line,
}

impl Chorus {
    /// A chorus for `sample_rate` with `knobs`. Allocates (off the audio thread).
    pub(crate) fn new(sample_rate: f32, knobs: ChorusKnobs) -> Self {
        let rate = sample_rate.max(1.0);
        let frames = (rate * (CENTRE_MS + 12.0) / 1_000.0).ceil() as usize + 4;
        let mut chorus = Chorus {
            rate,
            knobs: knobs.sanitized(),
            left: Line::new(frames),
            right: Line::new(frames),
        };
        chorus.set_knobs(knobs);
        chorus
    }

    /// New knob values, keeping what it is playing (no allocation).
    pub(crate) fn set_knobs(&mut self, knobs: ChorusKnobs) {
        self.knobs = knobs.sanitized();
        // The two sides a quarter cycle apart: the width.
        self.right.phase = 0.25;
    }

    /// Silence in its lines.
    pub(crate) fn reset(&mut self) {
        self.left.buffer.fill(0.0);
        self.right.buffer.fill(0.0);
        self.left.phase = 0.0;
        self.right.phase = 0.25;
    }

    /// The chorus on a stereo signal, in place.
    pub(crate) fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let mix = self.knobs.mix;
        let step = self.knobs.rate_hz / self.rate;
        let swing = self.knobs.depth_ms * self.rate / 1_000.0;
        let centre = CENTRE_MS * self.rate / 1_000.0;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let wet_l = self
                .left
                .process(*l, centre + swing * (TAU * self.left.phase).sin());
            let wet_r = self
                .right
                .process(*r, centre + swing * (TAU * self.right.phase).sin());
            self.left.phase = (self.left.phase + step).fract();
            self.right.phase = (self.right.phase + step).fract();
            *l = *l * (1.0 - mix) + wet_l * mix;
            *r = *r * (1.0 - mix) + wet_r * mix;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(knobs: ChorusKnobs, hz: f32, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let mut chorus = Chorus::new(RATE, knobs);
        let input: Vec<f32> = (0..frames)
            .map(|i| (TAU * hz * i as f32 / RATE).sin())
            .collect();
        let (mut left, mut right) = (input.clone(), input);
        chorus.process(&mut left, &mut right);
        (left, right)
    }

    #[test]
    fn a_dry_chorus_passes_the_signal_through() {
        let (left, _) = run(
            ChorusKnobs {
                mix: 0.0,
                ..ChorusKnobs::default()
            },
            220.0,
            4_800,
        );
        let input: Vec<f32> = (0..4_800)
            .map(|i| (TAU * 220.0 * i as f32 / RATE).sin())
            .collect();
        assert_eq!(left, input);
    }

    #[test]
    fn the_chorus_moves_the_sound_and_keeps_both_sides_apart() {
        let (left, right) = run(
            ChorusKnobs {
                rate_hz: 1.0,
                depth_ms: 5.0,
                mix: 1.0,
            },
            440.0,
            48_000,
        );
        assert!(left.iter().all(|x| x.is_finite()));
        let difference = left
            .iter()
            .zip(&right)
            .map(|(l, r)| (l - r).abs())
            .fold(0.0_f32, f32::max);
        assert!(difference > 0.05, "the sides differ: {difference}");
        let peak = left.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()));
        assert!(peak > 0.8, "{peak}");
    }

    #[test]
    fn a_broken_settings_file_never_makes_nan_or_a_huge_signal() {
        let broken = ChorusKnobs {
            rate_hz: f32::NAN,
            depth_ms: f32::INFINITY,
            mix: 1e9,
        }
        .sanitized();
        assert!(broken.rate_hz.is_finite() && broken.depth_ms.is_finite());
        assert!((broken.mix - 1.0).abs() < 1e-6);
        let (left, right) = run(broken, 110.0, 24_000);
        assert!(left.iter().chain(&right).all(|x| x.is_finite()));
        assert!(left.iter().all(|x| x.abs() < 4.0));
        let mut chorus = Chorus::new(RATE, ChorusKnobs::default());
        chorus.reset();
        assert!(chorus.left.buffer.iter().all(|&x| x == 0.0));
    }
}
