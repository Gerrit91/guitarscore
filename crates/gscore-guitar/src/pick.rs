//! What the pick adds besides setting the string going: the click of slipping off the
//! string and the short scrape of its edge along the winding, and the slightly sharp start
//! of a hard pluck (the string's tension rises with its swing and settles as the swing
//! dies down: tension modulation).
//!
//! The click is one bipolar force pulse (no offset) as wide as the pick takes to slip off:
//! about 0.6 ms for a soft, thick pick to 0.1 ms for a hard, thin one (the model's
//! hardness). The scrape is noise above about a kilohertz that dies within a few
//! milliseconds, longer for a soft pick. Both go into what the pickup or bridge hears,
//! not into the vibrating loop, so they never change the note itself.

use std::f32::consts::TAU;

use crate::filters::{Biquad, Noise};

/// The slip-off pulse's width (seconds) for a soft (hardness 0) and a hard pick (1).
const WIDTH: (f32, f32) = (0.000_6, 0.000_1);
/// The scrape's decay time constant (seconds), soft and hard.
const SCRAPE: (f32, f32) = (0.006, 0.002);
/// The scrape's noise is cut below this many Hz.
const SCRAPE_FROM: f32 = 1_200.0;
/// The click's and the scrape's level at full strength, against the strike's amplitude.
const CLICK_LEVEL: f32 = 2.0;
const SCRAPE_LEVEL: f32 = 0.8;
/// The scrape stops once its envelope is this low.
const SCRAPE_END: f32 = 1e-4;
/// The pitch's rise at the start of a pluck settles with this time constant (seconds):
/// within about 50 ms.
const TENSION_SETTLE: f32 = 0.015;
/// Below this many cents the rise is over.
const TENSION_END: f32 = 0.02;

/// The pick's click and scrape on one string.
#[derive(Debug, Clone)]
pub(crate) struct PickNoise {
    sample_rate: f32,
    noise: Noise,
    filter: Biquad,
    /// The click: where it is, how long it lasts (samples) and how loud.
    click_at: usize,
    click_length: usize,
    click_level: f32,
    /// The scrape's envelope now and its fall per sample.
    scrape: f32,
    scrape_fall: f32,
}

impl PickNoise {
    pub(crate) fn new(sample_rate: f32) -> Self {
        PickNoise {
            sample_rate,
            noise: Noise::new(0x2545_F491),
            filter: Biquad::highpass(sample_rate, SCRAPE_FROM, 0.707),
            click_at: 0,
            click_length: 0,
            click_level: 0.0,
            scrape: 0.0,
            scrape_fall: 0.0,
        }
    }

    /// Starts a click and scrape of `strength` (the strike's amplitude times the model's
    /// pick-noise knob and the attack's share) for a pick of `hardness` (0 to 1).
    pub(crate) fn start(&mut self, strength: f32, hardness: f32) {
        if !(strength.is_finite() && strength > 0.0) {
            return;
        }
        let hardness = hardness.clamp(0.0, 1.0);
        let between = |(soft, hard): (f32, f32)| soft * (hard / soft).powf(hardness);
        self.click_length = ((between(WIDTH) * self.sample_rate).round() as usize).max(2);
        self.click_at = 0;
        self.click_level = strength * CLICK_LEVEL;
        self.scrape = strength * SCRAPE_LEVEL;
        self.scrape_fall = (-1.0 / (between(SCRAPE) * self.sample_rate)).exp();
    }

    /// Whether anything is still sounding.
    pub(crate) fn is_active(&self) -> bool {
        self.click_at < self.click_length || self.scrape > SCRAPE_END
    }

    /// The next sample (0 once over).
    pub(crate) fn next(&mut self) -> f32 {
        let mut out = 0.0;
        if self.click_at < self.click_length {
            // One period of a sine over the pulse: a push and its release, no offset.
            let phase = self.click_at as f32 / self.click_length as f32;
            out += (TAU * phase).sin() * self.click_level;
            self.click_at = self.click_at.saturating_add(1);
        }
        if self.scrape > SCRAPE_END {
            out += self.filter.process(self.noise.next()) * self.scrape;
            self.scrape *= self.scrape_fall;
            if self.scrape <= SCRAPE_END {
                self.filter.reset();
            }
        }
        out
    }

    /// Stops at once.
    pub(crate) fn clear(&mut self) {
        self.click_at = self.click_length;
        self.scrape = 0.0;
        self.filter.reset();
    }
}

/// The pitch's rise at the start of a hard pluck, in cents, settling over time.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Tension {
    cents: f32,
}

impl Tension {
    /// A strike of `amplitude` (1 a full pick) on a string whose full-strength rise is
    /// `depth` cents: the rise grows with the square of the swing.
    pub(crate) fn strike(&mut self, amplitude: f32, depth: f32) {
        let swing = amplitude.clamp(0.0, 1.5);
        let cents = depth * swing * swing;
        self.cents = if cents.is_finite() && cents >= TENSION_END {
            cents
        } else {
            0.0
        };
    }

    /// The rise now.
    pub(crate) fn cents(&self) -> f32 {
        self.cents
    }

    /// Lets `samples` pass at `sample_rate`.
    pub(crate) fn settle(&mut self, samples: usize, sample_rate: f32) {
        if self.cents > 0.0 {
            self.cents *= (-(samples as f32) / (TENSION_SETTLE * sample_rate)).exp();
            if self.cents < TENSION_END {
                self.cents = 0.0;
            }
        }
    }
}
