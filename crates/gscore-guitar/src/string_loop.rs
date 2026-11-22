//! One vibrating loop of a string: a delay line with its loss filter and stiffness
//! allpasses, tuned exactly (the line's delay plus the filters' phase delay at the
//! fundamental is one period). A string has two of them, one per direction of vibration
//! ([`crate::StringVoice`]).

use std::f32::consts::TAU;

use crate::delay::{DelayLine, MIN_DELAY, Taps};
use crate::filters::{
    Allpass, Biquad, OnePole, SteepShelf, allpass_delay, one_pole_delay, one_pole_for_gain,
    one_pole_for_ratio, one_pole_gain, shelf_delay, shelf_for_ratio, shelf_gain,
};

/// Most allpass stages for stiffness.
pub(crate) const MAX_STIFFNESS_STAGES: usize = 4;
/// The partials an inharmonicity is fitted on: up to this many, and up to this frequency
/// (Hz) or 40 % of the sample rate.
const DISPERSION_PARTIALS: usize = 16;
const DISPERSION_TOP: f32 = 5_000.0;
/// The steps of the coefficient's search (golden section): a few millicents.
const DISPERSION_STEPS: usize = 30;
/// The frequency the high decay time is measured at, Hz.
const BRILLIANCE_AT: f32 = 2_000.0;
/// The most the loop may amplify per pass: the loop's zero-frequency mode must decay, so a
/// loss filter may not take more off the fundamental than this leaves room for. When the
/// brilliance asked for would need more, the highs are relaxed instead (see
/// [`Loop::update`]).
const MAX_LOOP_GAIN: f32 = 0.999_95;

/// The coefficient of [`MAX_STIFFNESS_STAGES`] stiffness allpasses that makes a loop at
/// `frequency` inharmonic by `b` (partial `n` at `n f sqrt((1 + b n²) / (1 + b))`, the
/// fundamental kept exact by the tuning): the coefficient whose phase delay, falling with
/// frequency, shortens the loop for each partial by what its target needs, fitted in the
/// least squares over the first partials. 0 (no allpass) for no inharmonicity. A search,
/// so made once per note (a bend keeps it). Allocates nothing.
pub(crate) fn dispersion(sample_rate: f32, frequency: f32, b: f32) -> f32 {
    // Not a number counts as none too.
    let usable = |value: f32| value.is_finite() && value > 0.0;
    if !usable(b) || !usable(frequency) || !usable(sample_rate) {
        return 0.0;
    }
    let w1 = TAU * frequency / sample_rate;
    let period = TAU / w1;
    let top = DISPERSION_TOP.min(0.4 * sample_rate);
    let partials = ((top / frequency) as usize).min(DISPERSION_PARTIALS);
    if partials < 2 {
        return 0.0;
    }
    let stages = MAX_STIFFNESS_STAGES as f32;
    let error = |c: f32| {
        let at_fundamental = allpass_delay(c, w1);
        let mut sum = 0.0_f32;
        for n in 2..=partials {
            let n = n as f32;
            let wn = w1 * n * ((1.0 + b * n * n) / (1.0 + b)).sqrt();
            if wn >= 3.0 {
                break;
            }
            // How much shorter the loop must be for this partial, and how much shorter the
            // allpasses make it.
            let wanted = period - TAU * n / wn;
            let made = stages * (at_fundamental - allpass_delay(c, wn));
            sum += (made - wanted) * (made - wanted);
        }
        sum
    };
    let ratio = 0.5 * (5.0_f32.sqrt() - 1.0);
    let (mut lo, mut hi) = (-0.95_f32, 0.0_f32);
    let mut a = hi - ratio * (hi - lo);
    let mut c = lo + ratio * (hi - lo);
    let (mut fa, mut fc) = (error(a), error(c));
    for _ in 0..DISPERSION_STEPS {
        if fa < fc {
            hi = c;
            c = a;
            fc = fa;
            a = hi - ratio * (hi - lo);
            fa = error(a);
        } else {
            lo = a;
            a = c;
            fa = fc;
            c = lo + ratio * (hi - lo);
            fc = error(c);
        }
    }
    let best = 0.5 * (lo + hi);
    if best.is_finite() { best } else { 0.0 }
}

/// What a loop is tuned to: its pitch, how long its fundamental and its highs around
/// 2 kHz take to fall 60 dB, its stiffness, and where the loss filter is a shelf (`corner`,
/// Hz: flat below it, and above it falling to a floor, so the top rings about as long as
/// the highs around 2 kHz; 0 for a plain lowpass). `sections` 2 is a palm's second-order
/// shelf (flat almost to its corner, then steep), 1 a ringing string's first-order treble
/// shelf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tuning {
    pub(crate) frequency: f32,
    pub(crate) decay: f32,
    pub(crate) high_decay: f32,
    pub(crate) stiffness: f32,
    pub(crate) stages: usize,
    pub(crate) corner: f32,
    pub(crate) sections: usize,
}

/// One loop.
#[derive(Debug, Clone)]
pub(crate) struct Loop {
    line: DelayLine,
    loss: OnePole,
    /// A palm's loss filter instead of `loss`: the second-order shelf, and its design.
    steep: Option<Biquad>,
    steep_design: Option<SteepShelf>,
    stiffness: [Allpass; MAX_STIFFNESS_STAGES],
    stages: usize,
    gain: f32,
    delay: f32,
    /// The line's read position for `taps_for`, the delay it was worked out for (so a
    /// delay that holds is not worked out again every sample).
    taps: Taps,
    taps_for: f32,
    delay_target: f32,
    delay_step: f32,
    glide_left: usize,
    /// The loss filter last designed, and for which pitch and decay times (the design is
    /// a search, so it is only redone when one of them changes).
    design: (f32, f32, f32, f32, usize),
    darkness: f32,
    zero: f32,
}

impl Loop {
    /// A loop holding at least `samples` of delay.
    pub(crate) fn new(samples: usize) -> Self {
        let line = DelayLine::new(samples);
        Loop {
            taps: line.taps(MIN_DELAY),
            taps_for: MIN_DELAY,
            line,
            loss: OnePole::default(),
            steep: None,
            steep_design: None,
            stiffness: [Allpass::default(); MAX_STIFFNESS_STAGES],
            stages: 0,
            gain: 0.0,
            delay: MIN_DELAY,
            delay_target: MIN_DELAY,
            delay_step: 0.0,
            glide_left: 0,
            design: (0.0, 0.0, 0.0, 0.0, 0),
            darkness: 0.0,
            zero: 0.0,
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
        let sections = if tuning.corner > 0.0 {
            tuning.sections.clamp(1, 2)
        } else {
            0
        };
        let key = (
            frequency,
            tuning.decay,
            tuning.high_decay,
            tuning.corner,
            sections,
        );
        if key != self.design {
            self.design = key;
            let high = (TAU * BRILLIANCE_AT / sample_rate).min(3.0);
            let ratio = per_period(tuning.high_decay) / per_period(tuning.decay);
            let corner = (TAU * tuning.corner / sample_rate).min(3.0);
            // The least the loss filter may take off the fundamental if the loop's gain is
            // to stay under [`MAX_LOOP_GAIN`]. A dark string (its highs at 2 kHz dying
            // quickly) needs a steep filter, and at a high pitch the fundamental is only an
            // octave or so below the brilliance, so that filter takes a lot off the
            // fundamental too; more than the loop can make up. Then the highs ring longer
            // than asked: the fundamental's sustain is what the player hears.
            let wanted = per_period(tuning.decay) / MAX_LOOP_GAIN;
            let shelved = high > w * 1.2 && corner > w && corner < high;
            self.steep_design = None;
            let (mut darkness, mut zero) = if high <= w * 1.2 {
                (0.0, 0.0)
            } else if sections == 2 && shelved {
                // A palm: the second-order shelf (no first-order loss).
                self.steep_design = Some(SteepShelf::for_ratio(
                    w,
                    high,
                    corner,
                    ratio,
                    wanted.min(1.0),
                ));
                (0.0, 0.0)
            } else if sections == 1 && shelved {
                shelf_for_ratio(w, high, corner, ratio)
            } else {
                (one_pole_for_ratio(w, high, ratio), 0.0)
            };
            if self.steep_design.is_none() && zero > 0.0 {
                if shelf_gain(darkness, zero, w) < wanted {
                    if wanted >= 1.0 {
                        zero = darkness;
                    } else {
                        // A larger zero takes less off the fundamental, up to the pole
                        // itself (where the shelf is flat and its gain is 1).
                        let (mut lo, mut hi) = (zero, darkness);
                        for _ in 0..24 {
                            let mid = 0.5 * (lo + hi);
                            if shelf_gain(darkness, mid, w) < wanted {
                                lo = mid;
                            } else {
                                hi = mid;
                            }
                        }
                        zero = hi;
                    }
                }
            } else if one_pole_gain(darkness, w) < wanted {
                // Less dark, so the highs ring longer than asked (the only way to keep the
                // fundamental's decay).
                darkness = darkness.min(one_pole_for_gain(w, wanted));
                zero = zero.min(darkness);
            }
            self.darkness = darkness;
            self.zero = zero;
        }
        let darkness = self.darkness.clamp(0.0, 0.995);
        let zero = self.zero.clamp(0.0, darkness);
        self.loss.set(darkness, zero);
        let (loss_gain, loss_delay) = if let Some(shelf) = self.steep_design {
            // Switching filters hands over what the other was filtering, so it adds no
            // click.
            let (x, y) = self.loss.last();
            let biquad = shelf.biquad();
            self.steep = Some(match self.steep.take() {
                Some(old) => old.retuned(biquad),
                None => biquad.primed(x, y),
            });
            (shelf.gain(w), shelf.delay(w))
        } else {
            if let Some(old) = self.steep.take() {
                let (x, y) = old.last();
                self.loss.prime(x, y);
            }
            if zero == 0.0 {
                (one_pole_gain(darkness, w), one_pole_delay(darkness, w))
            } else {
                (
                    shelf_gain(darkness, zero, w),
                    shelf_delay(darkness, zero, w),
                )
            }
        };
        self.gain = (per_period(tuning.decay) / loss_gain).min(MAX_LOOP_GAIN);
        self.stages = tuning.stages.min(MAX_STIFFNESS_STAGES);
        for stage in &mut self.stiffness {
            stage.c = tuning.stiffness;
        }
        let filters = loss_delay + self.stages as f32 * allpass_delay(tuning.stiffness, w);
        let target = (sample_rate / frequency - filters).clamp(MIN_DELAY, self.line.max_delay());
        self.delay_target = target;
        // Already there: no glide (adding a step of 0 every sample would change nothing).
        if glide == 0 || target.to_bits() == self.delay.to_bits() {
            self.delay = target;
            self.glide_left = 0;
        } else {
            self.delay_step = (target - self.delay) / glide as f32;
            self.glide_left = glide;
        }
    }

    /// Forgets the loss design, so the next update makes it again (a new model).
    pub(crate) fn redesign(&mut self) {
        self.design = (0.0, 0.0, 0.0, 0.0, 0);
    }

    /// One sample: what leaves the line (before this pass's losses), or `None` when a
    /// value broke (the loop must then be muted).
    pub(crate) fn step(&mut self) -> Option<f32> {
        if self.glide_left > 0 {
            self.delay += self.delay_step;
            self.glide_left = self.glide_left.saturating_sub(1);
        }
        // The read position is worked out once for a delay that holds; a slow settle
        // moves the delay by less than a bit for many samples, so it holds then too. While
        // it really moves (a bend, a slide) it is worked out every sample without keeping
        // it (keeping it costs more than it saves). Compared bit for bit, so a delay that
        // is not a number (never equal to itself) goes to `taps`.
        let tap = if self.delay.to_bits() == self.taps_for.to_bits() {
            self.line.read_at(&self.taps)
        } else if self.glide_left > 0 {
            self.line.read(self.delay)
        } else {
            self.taps = self.line.taps(self.delay);
            self.taps_for = self.delay;
            self.line.read_at(&self.taps)
        };
        let mut y = match &mut self.steep {
            Some(steep) => steep.process(tap),
            None => self.loss.process(tap),
        };
        y *= self.gain;
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
        if let Some(steep) = &mut self.steep {
            steep.reset();
        }
        for stage in &mut self.stiffness {
            stage.reset();
        }
    }
}
