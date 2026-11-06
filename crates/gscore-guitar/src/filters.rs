//! The small filters in and after the string loop, and their phase delays (which the
//! tuning subtracts from the delay line so the pitch stays exact).

use std::f32::consts::TAU;

/// A one-pole lowpass `y = (1 - a) x + a y[-1]`: the loss filter. A larger `a` damps the
/// high harmonics faster (a darker, duller string).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    pub(crate) a: f32,
    state: f32,
}

impl OnePole {
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        self.state = (1.0 - self.a) * x + self.a * self.state;
        self.state
    }

    pub(crate) fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// The gain of [`OnePole`] with coefficient `a` at `w` radians per sample.
pub(crate) fn one_pole_gain(a: f32, w: f32) -> f32 {
    let re = 1.0 - a * w.cos();
    let im = a * w.sin();
    (1.0 - a) / (re * re + im * im).sqrt().max(1e-9)
}

/// The phase delay of [`OnePole`] with coefficient `a` at `w`, in samples.
pub(crate) fn one_pole_delay(a: f32, w: f32) -> f32 {
    (a * w.sin()).atan2(1.0 - a * w.cos()) / w.max(1e-9)
}

/// A first-order allpass `(c + z^-1) / (1 + c z^-1)`. With `c` below 0 it delays low
/// frequencies more than high ones: a stiff string's high partials run sharp.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Allpass {
    pub(crate) c: f32,
    x1: f32,
    y1: f32,
}

impl Allpass {
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = self.c * x + self.x1 - self.c * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub(crate) fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// The phase delay of [`Allpass`] with coefficient `c` at `w`, in samples.
pub(crate) fn allpass_delay(c: f32, w: f32) -> f32 {
    let numerator = (-w.sin()).atan2(c + w.cos());
    let denominator = (-c * w.sin()).atan2(1.0 + c * w.cos());
    -(numerator - denominator) / w.max(1e-9)
}

/// Finds the [`OnePole`] coefficient whose gain at `high` is `ratio` times its gain at
/// `low` (both in radians per sample, `ratio` from 0 to 1): the loss filter that makes high
/// harmonics die at the rate asked for. Bisection, so any input gives an answer.
pub(crate) fn one_pole_for_ratio(low: f32, high: f32, ratio: f32) -> f32 {
    if !(ratio.is_finite() && ratio < 1.0) || high <= low {
        return 0.0;
    }
    let ratio = ratio.max(1e-6);
    let (mut lo, mut hi) = (0.0_f32, 0.995_f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        let r = one_pole_gain(mid, high) / one_pole_gain(mid, low).max(1e-9);
        // A larger coefficient takes more off the highs.
        if r > ratio {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// A second-order lowpass with resonance (RBJ cookbook): the tone of the pickup or the
/// body on the string's output.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// A lowpass at `cutoff` Hz with quality `q` for `sample_rate`.
    pub(crate) fn lowpass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.45);
        let w = TAU * cutoff / sample_rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Biquad::default()
        }
    }

    /// The same filter with new coefficients, keeping what it is filtering.
    pub(crate) fn retuned(self, other: Biquad) -> Self {
        Biquad {
            x1: self.x1,
            x2: self.x2,
            y1: self.y1,
            y2: self.y2,
            ..other
        }
    }

    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub(crate) fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Removes the constant offset an excitation leaves behind (a 10 Hz highpass).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    pub(crate) fn new(sample_rate: f32) -> Self {
        DcBlocker {
            r: 1.0 - TAU * 10.0 / sample_rate.max(1.0),
            x1: 0.0,
            y1: 0.0,
        }
    }

    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub(crate) fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// A small deterministic noise source (xorshift), for pick noise: the same notes always
/// sound the same.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Noise(u32);

impl Noise {
    pub(crate) fn new(seed: u32) -> Self {
        Noise(seed.max(1))
    }

    /// The next value, -1 to 1.
    pub(crate) fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_and_gains_match_the_filters_at_low_frequency() {
        // At low frequency the one-pole delays a / (1 - a) samples, the allpass
        // (1 - c) / (1 + c).
        let w = 0.001;
        assert!((one_pole_delay(0.5, w) - 1.0).abs() < 1e-3);
        assert!((one_pole_gain(0.5, w) - 1.0).abs() < 1e-3);
        assert!((allpass_delay(-0.2, w) - 1.5).abs() < 1e-3);
        assert!(
            allpass_delay(-0.2, 2.5) < allpass_delay(-0.2, w) - 0.2,
            "high partials pass sooner"
        );
    }

    #[test]
    fn the_loss_design_meets_its_ratio() {
        let (low, high) = (0.01, 0.26);
        for ratio in [0.9_f32, 0.5, 0.1, 0.01] {
            let a = one_pole_for_ratio(low, high, ratio);
            let got = one_pole_gain(a, high) / one_pole_gain(a, low);
            assert!((got - ratio).abs() < 0.01 || a > 0.99, "{ratio}: {got}");
        }
        assert_eq!(
            one_pole_for_ratio(0.2, 0.1, 0.5),
            0.0,
            "highs below the fundamental"
        );
        assert_eq!(one_pole_for_ratio(0.01, 0.2, f32::NAN), 0.0);
    }

    #[test]
    fn noise_stays_in_range_and_repeats() {
        let mut a = Noise::new(7);
        let mut b = Noise::new(7);
        for _ in 0..1000 {
            let value = a.next();
            assert!((-1.0..=1.0).contains(&value));
            assert_eq!(value.to_bits(), b.next().to_bits());
        }
    }
}
