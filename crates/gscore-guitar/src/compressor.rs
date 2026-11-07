//! A compressor after the classic FET studio compressors of the late 1960s (the "1176"
//! family; ours is called CA-76): very fast, punchy, a little colored. Computed, nothing
//! captured from a real unit.
//!
//! ```text
//!  in ─► input gain ─┬──────────────────────► × gain ─► FET color ─► output gain ─┬─► mix
//!                    └─► level ─► gain computer ─► attack / release ─┘                │
//!  in (dry) ──────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! - **Input** drives the signal into a fixed threshold, as on the original: more input,
//!   more compression. **Output** is the make-up gain.
//! - **Attack** 20 µs to 0.8 ms and **release** 50 ms to 1.1 s, in real time units. The
//!   release is program dependent: after compressing for a while it slows down (up to
//!   three times), so a sustained part breathes instead of pumping.
//! - **Ratio** 4, 8, 12 or 20 (each with its own threshold and knee: 4 soft and early, 20
//!   a hard limiter), or **All**: every ratio button pressed at once, the famous trick that
//!   pushes it into an aggressive, gritty over-compression.
//! - **Color:** the FET adds a little second harmonic, more the harder it works.
//! - **Mix** blends the dry signal back in (parallel compression inside one chain).
//!
//! Stereo is linked: both sides are compressed by the louder one, so the image stays.

use crate::filters::{DcBlocker, flushed};

/// The compressor's ratio buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompRatio {
    #[default]
    Four,
    Eight,
    Twelve,
    Twenty,
    /// All buttons in: harder than 20, its gain reduction overshooting, more color.
    All,
}

impl CompRatio {
    /// Every ratio, in the order the buttons sit.
    pub const ALL_RATIOS: [CompRatio; 5] = [
        CompRatio::Four,
        CompRatio::Eight,
        CompRatio::Twelve,
        CompRatio::Twenty,
        CompRatio::All,
    ];

    /// The button's label.
    pub fn label(self) -> &'static str {
        match self {
            CompRatio::Four => "4",
            CompRatio::Eight => "8",
            CompRatio::Twelve => "12",
            CompRatio::Twenty => "20",
            CompRatio::All => "All",
        }
    }

    /// Ratio, threshold (dBFS), knee width (dB), how far the gain reduction overshoots,
    /// and how strongly the FET colors.
    fn curve(self) -> (f32, f32, f32, f32, f32) {
        match self {
            CompRatio::Four => (4.0, -20.0, 8.0, 1.0, 1.0),
            CompRatio::Eight => (8.0, -18.0, 6.0, 1.0, 1.0),
            CompRatio::Twelve => (12.0, -17.0, 4.0, 1.0, 1.1),
            CompRatio::Twenty => (20.0, -16.0, 2.0, 1.0, 1.2),
            CompRatio::All => (30.0, -18.0, 3.0, 1.35, 2.5),
        }
    }
}

/// The knobs of a compressor block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorKnobs {
    /// The input gain in dB: how hard the signal is driven into the threshold.
    pub input_db: f32,
    /// The output (make-up) gain in dB.
    pub output_db: f32,
    /// Attack in ms (0.02 to 0.8).
    pub attack_ms: f32,
    /// Release in ms (50 to 1100).
    pub release_ms: f32,
    pub ratio: CompRatio,
    /// The share of the compressed signal, 0 dry to 1 only compressed.
    pub mix: f32,
}

impl Default for CompressorKnobs {
    fn default() -> Self {
        CompressorKnobs {
            input_db: 0.0,
            output_db: 0.0,
            attack_ms: 0.3,
            release_ms: 300.0,
            ratio: CompRatio::Four,
            mix: 1.0,
        }
    }
}

/// How long compression must last (time constant, seconds) before the release slows.
const SUSTAIN_SECONDS: f32 = 0.8;
/// How much the release slows after long compression.
const RELEASE_SLOWDOWN: f32 = 2.0;
/// The FET's second harmonic at 20 dB of gain reduction (a share of the signal).
const COLOR: f32 = 0.04;

/// One compressor (both sides, linked).
#[derive(Debug, Clone)]
pub(crate) struct Compressor {
    rate: f32,
    knobs: CompressorKnobs,
    input: f32,
    output: f32,
    attack: f32,
    release: f32,
    sustain_rate: f32,
    /// The gain reduction now, in dB (0 or below), and how long it has lasted (0 to 1).
    reduction: f32,
    sustain: f32,
    /// The deepest reduction since it was last read, in dB (for a meter).
    deepest: f32,
    dc: [DcBlocker; 2],
}

impl Compressor {
    /// A compressor for `sample_rate` with `knobs`. Allocates nothing.
    pub(crate) fn new(sample_rate: f32, knobs: CompressorKnobs) -> Self {
        let rate = sample_rate.max(1.0);
        let mut compressor = Compressor {
            rate,
            knobs,
            input: 1.0,
            output: 1.0,
            attack: 0.0,
            release: 0.0,
            sustain_rate: 1.0 - (-1.0 / (SUSTAIN_SECONDS * rate)).exp(),
            reduction: 0.0,
            sustain: 0.0,
            deepest: 0.0,
            dc: [DcBlocker::new(rate), DcBlocker::new(rate)],
        };
        compressor.set_knobs(knobs);
        compressor
    }

    /// New knob values (no allocation).
    pub(crate) fn set_knobs(&mut self, knobs: CompressorKnobs) {
        let db = |db: f32| 10.0_f32.powf(db / 20.0);
        self.input = db(knobs.input_db);
        self.output = db(knobs.output_db);
        let coefficient = |ms: f32| (-1.0 / (ms.max(0.001) * 0.001 * self.rate)).exp();
        self.attack = coefficient(knobs.attack_ms);
        self.release = coefficient(knobs.release_ms);
        self.knobs = knobs;
    }

    /// Silence in its detector and filters.
    pub(crate) fn reset(&mut self) {
        self.reduction = 0.0;
        self.sustain = 0.0;
        for dc in &mut self.dc {
            dc.reset();
        }
    }

    /// The deepest gain reduction (dB, 0 or below) since the last call.
    pub(crate) fn take_deepest(&mut self) -> f32 {
        std::mem::take(&mut self.deepest)
    }

    /// The static curve: the gain reduction (dB, 0 or below) for a level of `level_db`.
    fn target(&self, level_db: f32) -> f32 {
        let (ratio, threshold, knee, overshoot, _) = self.knobs.ratio.curve();
        let over = level_db - threshold;
        let slope = 1.0 / ratio - 1.0;
        let reduction = if 2.0 * over < -knee {
            0.0
        } else if 2.0 * over <= knee {
            slope * (over + knee / 2.0).powi(2) / (2.0 * knee.max(1e-3))
        } else {
            slope * over
        };
        reduction * overshoot
    }

    /// One stereo sample through the compressor (`right` is ignored and copied when the
    /// signal is still mono).
    pub(crate) fn process(&mut self, left: f32, right: f32, stereo: bool) -> (f32, f32) {
        let right = if stereo { right } else { left };
        let (in_left, in_right) = (left * self.input, right * self.input);
        let level = in_left.abs().max(in_right.abs());
        let level_db = 20.0 * level.max(1e-9).log10();
        let target = self.target(level_db);
        // Toward more reduction at the attack's speed, back at the release's, slower after
        // compressing for a while.
        let coefficient = if target < self.reduction {
            self.attack
        } else {
            let slower = 1.0 + RELEASE_SLOWDOWN * self.sustain;
            self.release.powf(1.0 / slower)
        };
        self.reduction = flushed(target + (self.reduction - target) * coefficient);
        let working = if self.reduction < -3.0 { 1.0 } else { 0.0 };
        self.sustain += (working - self.sustain) * self.sustain_rate;
        self.deepest = self.deepest.min(self.reduction);
        let gain = 10.0_f32.powf(self.reduction / 20.0);
        let (_, _, _, _, color) = self.knobs.ratio.curve();
        let amount = COLOR * color * (-self.reduction / 20.0).min(2.0);
        let mix = self.knobs.mix.clamp(0.0, 1.0);
        let [dc_left, dc_right] = &mut self.dc;
        let shape = |x: f32, dc: &mut DcBlocker| dc.process(x + amount * x * x);
        let wet_left = shape(in_left * gain, dc_left) * self.output;
        let wet_right = shape(in_right * gain, dc_right) * self.output;
        let out_left = left * (1.0 - mix) + wet_left * mix;
        let out_right = right * (1.0 - mix) + wet_right * mix;
        (out_left, out_right)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    fn sine(level: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| level * (TAU * 220.0 * i as f32 / RATE).sin())
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |p, x| p.max(x.abs()))
    }

    fn run(knobs: CompressorKnobs, input: &[f32]) -> (Vec<f32>, Compressor) {
        let mut compressor = Compressor::new(RATE, knobs);
        let out = input
            .iter()
            .map(|&x| compressor.process(x, x, false).0)
            .collect();
        (out, compressor)
    }

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    #[test]
    fn quiet_signals_pass_and_loud_ones_are_held_down_by_the_ratio() {
        // Well below the threshold: unchanged (but for the color, which needs reduction).
        let (quiet, _) = run(CompressorKnobs::default(), &sine(0.01, 24_000));
        assert!((peak(&quiet[12_000..]) - 0.01).abs() < 0.0005);
        // 20 dB over the threshold at 4:1 comes out about 15 dB down; at 20:1 about 19.
        for (ratio, expected) in [(CompRatio::Four, 15.0), (CompRatio::Twenty, 19.0)] {
            let (_, threshold, ..) = ratio.curve();
            let level = 10.0_f32.powf((threshold + 20.0) / 20.0);
            let knobs = CompressorKnobs {
                ratio,
                ..CompressorKnobs::default()
            };
            let (out, _) = run(knobs, &sine(level, 48_000));
            let reduction = db(level) - db(peak(&out[36_000..]));
            assert!(
                (reduction - expected).abs() < 2.0,
                "{ratio:?}: {reduction} dB, expected about {expected}"
            );
        }
    }

    #[test]
    fn more_input_more_compression_and_all_buttons_hit_hardest() {
        let input = sine(0.3, 48_000);
        let reduction = |knobs: CompressorKnobs| {
            let (_, mut compressor) = run(knobs, &input);
            -compressor.take_deepest()
        };
        let base = CompressorKnobs::default();
        let gentle = reduction(base);
        let driven = reduction(CompressorKnobs {
            input_db: 12.0,
            ..base
        });
        assert!(driven > gentle + 6.0, "{gentle} then {driven}");
        let all = reduction(CompressorKnobs {
            input_db: 12.0,
            ratio: CompRatio::All,
            ..base
        });
        let twenty = reduction(CompressorKnobs {
            input_db: 12.0,
            ratio: CompRatio::Twenty,
            ..base
        });
        assert!(all > twenty, "all buttons {all} against 20:1 {twenty}");
    }

    #[test]
    fn the_attack_lets_a_transient_through_and_the_release_recovers() {
        // A loud burst after silence: with a slow attack its first samples pass louder
        // than with a fast one.
        let mut burst = vec![0.0_f32; 4_800];
        burst.extend(sine(0.8, 4_800));
        let first = |attack_ms: f32| {
            let (out, _) = run(
                CompressorKnobs {
                    attack_ms,
                    ratio: CompRatio::Twenty,
                    ..CompressorKnobs::default()
                },
                &burst,
            );
            peak(&out[4_800..4_900])
        };
        assert!(
            first(0.8) > 1.3 * first(0.02),
            "{} {}",
            first(0.8),
            first(0.02)
        );
        // After the burst, the gain comes back: faster with a fast release.
        let recovered = |release_ms: f32| {
            let mut compressor = Compressor::new(
                RATE,
                CompressorKnobs {
                    release_ms,
                    ..CompressorKnobs::default()
                },
            );
            for x in sine(0.8, 9_600) {
                compressor.process(x, x, false);
            }
            for _ in 0..4_800 {
                compressor.process(0.0, 0.0, false);
            }
            compressor.reduction
        };
        assert!(recovered(50.0) > recovered(1_100.0), "fast recovers more");
    }

    #[test]
    fn mix_blends_the_dry_signal_back_and_stereo_stays_linked() {
        let input = sine(0.5, 24_000);
        let wet = |mix: f32| {
            let (out, _) = run(
                CompressorKnobs {
                    mix,
                    ratio: CompRatio::Twenty,
                    ..CompressorKnobs::default()
                },
                &input,
            );
            peak(&out[12_000..])
        };
        assert!(wet(0.0) > 0.49, "dry");
        assert!(wet(0.5) > wet(1.0) && wet(0.5) < wet(0.0));
        // A loud left and a quiet right: both are turned down alike.
        let mut compressor = Compressor::new(RATE, CompressorKnobs::default());
        let mut last = (0.0, 0.0);
        for x in sine(0.8, 24_000) {
            last = compressor.process(x, x * 0.1, true);
        }
        let _ = last;
        let (l, r) = compressor.process(0.8, 0.08, true);
        assert!((r / l - 0.1).abs() < 0.01, "linked: {l} {r}");
    }
}
