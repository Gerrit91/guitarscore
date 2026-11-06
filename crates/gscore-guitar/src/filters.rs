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
