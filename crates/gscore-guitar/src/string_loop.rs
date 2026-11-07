//! One vibrating loop of a string: a delay line with its loss filter and stiffness
//! allpasses, tuned exactly (the line's delay plus the filters' phase delay at the
//! fundamental is one period). A string has two of them, one per direction of vibration
//! ([`crate::StringVoice`]).

use std::f32::consts::TAU;

use crate::delay::{DelayLine, MIN_DELAY};
use crate::filters::{
    Allpass, OnePole, allpass_delay, one_pole_delay, one_pole_for_ratio, one_pole_gain,
};

/// Most allpass stages for stiffness.
pub(crate) const MAX_STIFFNESS_STAGES: usize = 4;
/// The frequency the high decay time is measured at, Hz.
const BRILLIANCE_AT: f32 = 2_000.0;

/// What a loop is tuned to: its pitch, how long its fundamental and its highs around
/// 2 kHz take to fall 60 dB, and its stiffness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tuning {
    pub(crate) frequency: f32,
    pub(crate) decay: f32,
    pub(crate) high_decay: f32,
    pub(crate) stiffness: f32,
    pub(crate) stages: usize,
}

/// One loop.
#[derive(Debug, Clone)]
pub(crate) struct Loop {
    line: DelayLine,
    loss: OnePole,
    stiffness: [Allpass; MAX_STIFFNESS_STAGES],
    stages: usize,
    gain: f32,
    delay: f32,
    delay_target: f32,
    delay_step: f32,
    glide_left: usize,
    /// The loss filter last designed, and for which pitch and decay times (the design is
    /// a search, so it is only redone when one of them changes).
    design: (f32, f32, f32),
    darkness: f32,
}

impl Loop {
    /// A loop holding at least `samples` of delay.
    pub(crate) fn new(samples: usize) -> Self {
        Loop {
            line: DelayLine::new(samples),
            loss: OnePole::default(),
            stiffness: [Allpass::default(); MAX_STIFFNESS_STAGES],
            stages: 0,
            gain: 0.0,
            delay: MIN_DELAY,
            delay_target: MIN_DELAY,
            delay_step: 0.0,
            glide_left: 0,
            design: (0.0, 0.0, 0.0),
            darkness: 0.0,
        }
    }

    /// The delay the loop is heading for, in samples (about one period).
    pub(crate) fn period(&self) -> f32 {
        self.delay_target
    }

    /// Retunes the loop; the delay glides to its new value over `glide` samples (at once
    /// if 0).
    pub(crate) fn update(&mut self, sample_rate: f32, tuning: Tuning, glide: usize) {
        let frequency = tuning.frequency.max(8.0);
        let w = (TAU * frequency / sample_rate).min(3.0);
        // The loop's gain per period that decays 60 dB in `decay` seconds, for the
        // fundamental and for the harmonics around 2 kHz: the loss filter takes the highs
        // down at their own rate, the same on every string (a low string goes round its
        // loop less often, so a fixed filter would let its highs ring far too long).
        let per_period = |decay: f32| 10.0_f32.powf(-3.0 / (decay.max(0.002) * frequency));
        let key = (frequency, tuning.decay, tuning.high_decay);
        if key != self.design {
            self.design = key;
            let high = (TAU * BRILLIANCE_AT / sample_rate).min(3.0);
            let ratio = per_period(tuning.high_decay) / per_period(tuning.decay);
            self.darkness = if high > w * 1.2 {
                one_pole_for_ratio(w, high, ratio)
            } else {
                0.0
            };
        }
        let darkness = self.darkness.clamp(0.0, 0.995);
        self.loss.a = darkness;
        self.gain = (per_period(tuning.decay) / one_pole_gain(darkness, w)).min(0.999_95);
        self.stages = tuning.stages.min(MAX_STIFFNESS_STAGES);
        for stage in &mut self.stiffness {
            stage.c = tuning.stiffness;
        }
        let filters =
            one_pole_delay(darkness, w) + self.stages as f32 * allpass_delay(tuning.stiffness, w);
        let target = (sample_rate / frequency - filters).clamp(MIN_DELAY, self.line.max_delay());
        self.delay_target = target;
        if glide == 0 {
            self.delay = target;
            self.glide_left = 0;
        } else {
            self.delay_step = (target - self.delay) / glide as f32;
            self.glide_left = glide;
        }
    }

    /// Forgets the loss design, so the next update makes it again (a new model).
    pub(crate) fn redesign(&mut self) {
        self.design = (0.0, 0.0, 0.0);
    }

    /// One sample: what leaves the line (before this pass's losses), or `None` when a
    /// value broke (the loop must then be muted).
    pub(crate) fn step(&mut self) -> Option<f32> {
        if self.glide_left > 0 {
            self.delay += self.delay_step;
            self.glide_left = self.glide_left.saturating_sub(1);
        }
        let tap = self.line.read(self.delay);
        let mut y = self.loss.process(tap) * self.gain;
        for stage in self.stiffness.iter_mut().take(self.stages) {
            y = stage.process(y);
        }
        if !y.is_finite() {
            return None;
        }
        self.line.push(y);
        Some(tap)
    }

    /// Changes the next `count` samples to be read (oldest first) with `change(position,
    /// old)`: an excitation added to the vibrating loop.
    pub(crate) fn reshape(&mut self, count: usize, change: impl FnMut(usize, f32) -> f32) {
        self.line.reshape(count, change);
    }

    /// Silence, nothing left in the loop.
    pub(crate) fn clear(&mut self) {
        self.line.clear();
        self.loss.reset();
        for stage in &mut self.stiffness {
            stage.reset();
        }
    }
}
