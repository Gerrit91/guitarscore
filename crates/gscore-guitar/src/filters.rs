//! The small filters in and after the string loop, and their phase delays (which the
//! tuning subtracts from the delay line so the pitch stays exact).

use std::f32::consts::{SQRT_2, TAU};

/// Below this a filter's output counts as zero: a filter fed silence decays toward ever
/// tinier numbers, and below about 1e-38 (denormals) the processor takes up to a hundred
/// times longer for each one. A drive and an amp before a long reverb tail, run on the
/// silence after the strings stop, took half a core per track that way. 1e-20 is 400 dB
/// down: nothing anyone hears.
pub(crate) const FLUSH: f32 = 1e-20;

/// `x`, or exactly zero when it is too small to matter (see [`FLUSH`]).
#[inline]
pub(crate) fn flushed(x: f32) -> f32 {
    if x.abs() < FLUSH { 0.0 } else { x }
}

/// A one-pole lowpass `y = (1 - a) x + a y[-1]`: the loss filter. A larger `a` damps the
/// high harmonics faster (a darker, duller string). With a `zero` (0 to `a`) it becomes a
/// shelf, `y = k (x - zero x[-1]) + a y[-1]` with `k = (1 - a) / (1 - zero)`: flat at the
/// low end and down to a floor above it, so the highs go much sooner than the fundamental
/// and the first harmonics (a palm mute). With `zero` 0 it is exactly the lowpass.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    pub(crate) a: f32,
    zero: f32,
    k: f32,
    x1: f32,
    state: f32,
}

impl OnePole {
    /// Sets the pole `a` and the zero (see the type).
    pub(crate) fn set(&mut self, a: f32, zero: f32) {
        self.a = a;
        self.zero = zero;
        self.k = (1.0 - a) / (1.0 - zero).max(1e-6);
    }

    pub(crate) fn process(&mut self, x: f32) -> f32 {
        // Not flushed: it runs only inside a sounding string's loop, and a string stops
        // being computed once it is silent (far above the denormal range).
        self.state = if self.zero == 0.0 {
            (1.0 - self.a) * x + self.a * self.state
        } else {
            self.k * (x - self.zero * self.x1) + self.a * self.state
        };
        self.x1 = x;
        self.state
    }

    pub(crate) fn reset(&mut self) {
        self.x1 = 0.0;
        self.state = 0.0;
    }

    /// The last input and output.
    pub(crate) fn last(&self) -> (f32, f32) {
        (self.x1, self.state)
    }

    /// Takes `x` and `y` as its last input and output (handed over from another filter).
    pub(crate) fn prime(&mut self, x: f32, y: f32) {
        self.x1 = x;
        self.state = y;
    }
}

/// The gain of the shelf [`OnePole`] with pole `a` and `zero` at `w` radians per sample.
pub(crate) fn shelf_gain(a: f32, zero: f32, w: f32) -> f32 {
    let (sin, cos) = w.sin_cos();
    let numerator = ((1.0 - zero * cos).powi(2) + (zero * sin).powi(2)).sqrt();
    let denominator = ((1.0 - a * cos).powi(2) + (a * sin).powi(2)).sqrt();
    (1.0 - a) / (1.0 - zero).max(1e-6) * numerator / denominator.max(1e-9)
}

/// The phase delay of the shelf [`OnePole`] with pole `a` and `zero` at `w`, in samples.
pub(crate) fn shelf_delay(a: f32, zero: f32, w: f32) -> f32 {
    let (sin, cos) = w.sin_cos();
    ((a * sin).atan2(1.0 - a * cos) - (zero * sin).atan2(1.0 - zero * cos)) / w.max(1e-9)
}

/// The shelf whose pole sits at `corner` (radians per sample) and whose gain at `high` is
/// `ratio` times its gain at `low`: `(pole, zero)`. When even the plain lowpass at that
/// corner does not take that much off, it is that lowpass: the highs go as fast as they
/// can without taking the low end (moving the pole down would dull the fundamental too).
pub(crate) fn shelf_for_ratio(low: f32, high: f32, corner: f32, ratio: f32) -> (f32, f32) {
    if !(ratio.is_finite() && ratio < 1.0) || high <= low {
        return (0.0, 0.0);
    }
    let a = (-corner.max(1e-6)).exp().clamp(0.0, 0.995);
    let r = |zero: f32| shelf_gain(a, zero, high) / shelf_gain(a, zero, low).max(1e-9);
    if r(0.0) >= ratio {
        return (a, 0.0);
    }
    let (mut lo, mut hi) = (0.0_f32, a);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        // A larger zero takes less off the highs.
        if r(mid) < ratio {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (a, 0.5 * (lo + hi))
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

/// A second-order shelf, maximally flat on both its poles and its zeros: unity below
/// `corner`, falling 12 dB an octave above it to `floor`. Its loss below the corner grows
/// with the fourth power of the frequency (a first-order shelf's with the square), so it
/// can take a string's highs down hard while hardly touching the fundamental a few
/// harmonics below: a palm mute's loss filter. Designed through the analog prototype
/// `floor (s^2 + sqrt2 Wz s + Wz^2) / (s^2 + sqrt2 Wp s + Wp^2)` (`Wz = Wp / sqrt(floor)`)
/// and the bilinear transform with prewarped frequencies, so its gain and phase delay at a
/// frequency are exact. Frequencies are radians per sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SteepShelf {
    pub(crate) corner: f32,
    pub(crate) floor: f32,
}

impl SteepShelf {
    /// The prototype's frequency for `w` (prewarped), kept clear of Nyquist.
    fn warp(w: f32) -> f32 {
        2.0 * (0.5 * w.clamp(1e-6, 3.1)).tan()
    }

    fn zeros(self) -> (f32, f32) {
        let pole = Self::warp(self.corner);
        (pole, pole / self.floor.clamp(1e-6, 1.0).sqrt())
    }

    /// Its gain at `w`.
    pub(crate) fn gain(self, w: f32) -> f32 {
        let (pole, zero) = self.zeros();
        let x = Self::warp(w);
        ((1.0 + (x / zero).powi(4)) / (1.0 + (x / pole).powi(4))).sqrt()
    }

    /// Its phase delay at `w`, in samples.
    pub(crate) fn delay(self, w: f32) -> f32 {
        let (pole, zero) = self.zeros();
        let x = Self::warp(w);
        let angle = |corner: f32| (SQRT_2 * corner * x).atan2(corner * corner - x * x);
        (angle(pole) - angle(zero)) / w.max(1e-9)
    }

    /// Its coefficients, for a [`Biquad`] (which keeps what it is filtering on
    /// [`Biquad::retuned`]).
    pub(crate) fn biquad(self) -> Biquad {
        let (pole, zero) = self.zeros();
        let side = |corner: f32| {
            let r = SQRT_2 * corner * 2.0;
            let c2 = corner * corner;
            (4.0 + r + c2, 2.0 * c2 - 8.0, 4.0 - r + c2)
        };
        let (n0, n1, n2) = side(zero);
        let (d0, d1, d2) = side(pole);
        let k = self.floor.clamp(1e-6, 1.0) / d0.max(1e-9);
        Biquad {
            b0: k * n0,
            b1: k * n1,
            b2: k * n2,
            a1: d1 / d0.max(1e-9),
            a2: d2 / d0.max(1e-9),
            ..Biquad::default()
        }
    }

    /// The shelf at `corner` whose gain at `high` is `ratio` times its gain at `low`,
    /// keeping at least `keep` of the gain at `low` (a loop's limit on how much its loss
    /// filter may take off the fundamental). When even the deepest floor does not make the
    /// ratio, the deepest; when the ratio would take too much off `low`, the deepest floor
    /// that does not (the highs then ring longer than asked).
    pub(crate) fn for_ratio(low: f32, high: f32, corner: f32, ratio: f32, keep: f32) -> Self {
        const DEEPEST: f32 = 1e-4;
        let at = |floor: f32| SteepShelf { corner, floor };
        if !(ratio.is_finite() && ratio < 1.0 && high > low && corner > low) {
            return at(1.0);
        }
        let r = |floor: f32| at(floor).gain(high) / at(floor).gain(low).max(1e-9);
        // The floor's log, searched: a deeper floor makes a smaller ratio.
        let (mut lo, mut hi) = (DEEPEST.ln(), 0.0_f32);
        if r(DEEPEST) < ratio {
            for _ in 0..30 {
                let mid = 0.5 * (lo + hi);
                if r(mid.exp()) < ratio {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
        } else {
            hi = lo;
        }
        let mut floor = hi.exp();
        if at(floor).gain(low) < keep {
            let (mut lo, mut hi) = (floor.ln(), 0.0_f32);
            for _ in 0..30 {
                let mid = 0.5 * (lo + hi);
                if at(mid.exp()).gain(low) < keep {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            floor = hi.exp();
        }
        at(floor)
    }
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
        // Not flushed, as the one-pole (inside a sounding string's loop only).
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
/// harmonics die at the rate asked for. Any input gives an answer from 0 to 0.995.
///
/// Solved, not searched (a string redesigns it every block while it bends): the squared
/// gain is `(1 - a)² / (1 - 2a cos w + a²)`, so the squared ratio `R` asks for
/// `a² - 2qa + 1 = 0` with `q = (cos low - R cos high) / (1 - R)`. The roots multiply to 1;
/// the one below 1 is `1 / (q + sqrt(q² - 1))`. With `q` under 1 not even the darkest
/// filter takes that much off, and the darkest is the answer.
pub(crate) fn one_pole_for_ratio(low: f32, high: f32, ratio: f32) -> f32 {
    if !(ratio.is_finite() && ratio < 1.0) || high <= low {
        return 0.0;
    }
    let ratio = f64::from(ratio.max(1e-6));
    let squared = ratio * ratio;
    let q = (f64::from(low).cos() - squared * f64::from(high).cos()) / (1.0 - squared);
    if !(q.is_finite() && q > 1.0) {
        return MAX_ONE_POLE;
    }
    let a = 1.0 / (q + (q * q - 1.0).sqrt());
    (a as f32).clamp(0.0, MAX_ONE_POLE)
}

/// The [`OnePole`] coefficient whose gain at `w` radians per sample is exactly `gain`
/// (0 to 1), or 0 when `gain` is 1 or more. The loss filter that keeps a loop's
/// fundamental at the decay it was asked for when even the design it would pick takes too
/// much off the fundamental (see [`crate::string_loop`]).
///
/// The same solved equation as [`one_pole_for_ratio`], with the low frequency at DC (gain
/// 1): `q = (1 - G² cos w) / (1 - G²)`, and the root below 1 is
/// `1 / (q + sqrt(q² - 1))`.
pub(crate) fn one_pole_for_gain(w: f32, gain: f32) -> f32 {
    if !(gain.is_finite() && gain > 0.0 && gain < 1.0) || !w.is_finite() {
        return 0.0;
    }
    let squared = f64::from(gain) * f64::from(gain);
    let q = (1.0 - squared * f64::from(w).cos()) / (1.0 - squared);
    if !(q.is_finite() && q > 1.0) {
        return 0.0;
    }
    let a = 1.0 / (q + (q * q - 1.0).sqrt());
    (a as f32).clamp(0.0, MAX_ONE_POLE)
}

/// The darkest loss filter [`one_pole_for_ratio`] gives.
const MAX_ONE_POLE: f32 = 0.995;

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

    /// A highpass at `cutoff` Hz with quality `q` for `sample_rate`.
    pub(crate) fn highpass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let cutoff = cutoff.clamp(10.0, sample_rate * 0.45);
        let w = TAU * cutoff / sample_rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Biquad::default()
        }
    }

    /// A band-pass at `centre` Hz of quality `q`, 0 dB at its centre (RBJ, constant peak
    /// gain): one resonance of a body, ringing on for about `q / (pi * centre)` seconds.
    pub(crate) fn bandpass(sample_rate: f32, centre: f32, q: f32) -> Self {
        let centre = centre.clamp(20.0, sample_rate * 0.45);
        let w = TAU * centre / sample_rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Biquad {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha) / a0,
            ..Biquad::default()
        }
    }

    /// A bell at `centre` Hz, `gain_db` up or down, of quality `q` (RBJ peaking EQ).
    pub(crate) fn peaking(sample_rate: f32, centre: f32, q: f32, gain_db: f32) -> Self {
        let centre = centre.clamp(20.0, sample_rate * 0.45);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w = TAU * centre / sample_rate;
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let cos = w.cos();
        let a0 = 1.0 + alpha / a;
        Biquad {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha / a) / a0,
            ..Biquad::default()
        }
    }

    /// A shelf at `corner` Hz raising or lowering everything below it (`high` false) or
    /// above it (`high` true) by `gain_db` (RBJ shelves, slope 1).
    pub(crate) fn shelf(sample_rate: f32, corner: f32, gain_db: f32, high: bool) -> Self {
        let corner = corner.clamp(20.0, sample_rate * 0.45);
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w = TAU * corner / sample_rate;
        let cos = w.cos();
        // Slope 1: alpha = sin(w) / 2 * sqrt(2).
        let alpha = w.sin() / 2.0 * SQRT_2;
        let root = 2.0 * a.sqrt() * alpha;
        let (p, m) = (a + 1.0, a - 1.0);
        let (b0, b1, b2, a0, a1, a2) = if high {
            (
                a * (p + m * cos + root),
                -2.0 * a * (m + p * cos),
                a * (p + m * cos - root),
                p - m * cos + root,
                2.0 * (m - p * cos),
                p - m * cos - root,
            )
        } else {
            (
                a * (p - m * cos + root),
                2.0 * a * (m - p * cos),
                a * (p - m * cos - root),
                p + m * cos + root,
                -2.0 * (m + p * cos),
                p + m * cos - root,
            )
        };
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            ..Biquad::default()
        }
    }

    /// The gain at `frequency` Hz for `sample_rate` (for tests, level matching and the
    /// EQ's drawn curve).
    pub(crate) fn gain_at(&self, sample_rate: f32, frequency: f32) -> f32 {
        let w = TAU * frequency / sample_rate;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let num = (self.b0 + self.b1 * c1 + self.b2 * c2).hypot(self.b1 * s1 + self.b2 * s2);
        let den = (1.0 + self.a1 * c1 + self.a2 * c2).hypot(self.a1 * s1 + self.a2 * s2);
        num / den.max(1e-12)
    }

    /// The last input and output.
    pub(crate) fn last(&self) -> (f32, f32) {
        (self.x1, self.y1)
    }

    /// This filter as if it had been taking `x` and giving `y` (handed over from another).
    pub(crate) fn primed(self, x: f32, y: f32) -> Self {
        Biquad {
            x1: x,
            x2: x,
            y1: y,
            y2: y,
            ..self
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
        let y = flushed(
            self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1
                - self.a2 * self.y2,
        );
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
        let y = flushed(x - self.x1 + self.r * self.y1);
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
    fn the_solved_loss_design_is_the_bisection_it_replaced() {
        // The search the string used until 2026-11-13 (24 halvings of 0 to 0.995).
        let bisection = |low: f32, high: f32, ratio: f32| {
            let (mut lo, mut hi) = (0.0_f32, 0.995_f32);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if one_pole_gain(mid, high) / one_pole_gain(mid, low).max(1e-9) > ratio {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            0.5 * (lo + hi)
        };
        let high = TAU * 2_000.0 / 48_000.0;
        for note in [30.0_f32, 82.4, 110.0, 440.0, 1_200.0] {
            let low = TAU * note / 48_000.0;
            for ratio in [0.999_f32, 0.9, 0.5, 0.1, 1e-3, 1e-5] {
                let (solved, searched) = (
                    one_pole_for_ratio(low, high, ratio),
                    bisection(low, high, ratio),
                );
                assert!(
                    (solved - searched).abs() < 1e-5,
                    "{note} Hz, {ratio}: {solved} {searched}"
                );
            }
        }
    }

    #[test]
    fn the_solved_absolute_loss_is_met() {
        // The filter whose gain at the fundamental is exactly this (see the string loop's
        // fallback when a string's brightness would kill its sustain).
        for w in [0.01_f32, 0.05, 0.13, 0.26] {
            for gain in [0.999_f32, 0.99, 0.9, 0.5] {
                let a = one_pole_for_gain(w, gain);
                let got = one_pole_gain(a, w);
                assert!(
                    (got - gain).abs() < 1e-3 || a > 0.99,
                    "{w} rad, gain {gain}: {got} (a {a})"
                );
            }
        }
        assert_eq!(one_pole_for_gain(0.1, 1.0), 0.0, "flat is enough");
        assert_eq!(one_pole_for_gain(0.1, f32::NAN), 0.0);
        assert_eq!(one_pole_for_gain(0.1, 0.0), 0.0);
    }

    #[test]
    fn the_shelf_is_flat_below_its_corner_and_meets_its_ratio() {
        let (low, corner, high) = (0.01, 0.05, 0.26);
        for ratio in [0.6_f32, 0.4, 0.3] {
            let (a, zero) = shelf_for_ratio(low, high, corner, ratio);
            assert!(zero > 0.0 && zero < a, "{ratio}: a shelf ({a}, {zero})");
            let got = shelf_gain(a, zero, high) / shelf_gain(a, zero, low);
            assert!((got - ratio).abs() < 0.01, "{ratio}: {got}");
            assert!(
                (shelf_gain(a, zero, 1e-4) - 1.0).abs() < 1e-3,
                "unity at DC"
            );
        }
        // Too steep for a shelf at that corner: the plain lowpass at the corner.
        let (a, zero) = shelf_for_ratio(low, high, corner, 0.05);
        assert_eq!(zero, 0.0);
        assert!((a - (-corner).exp()).abs() < 1e-6);
        // Without a zero the shelf is the lowpass, and the filter computes the same.
        assert_eq!(shelf_gain(0.7, 0.0, 0.2), one_pole_gain(0.7, 0.2));
        let (mut plain, mut shelf) = (OnePole::default(), OnePole::default());
        plain.a = 0.7;
        shelf.set(0.7, 0.0);
        for x in [1.0_f32, -0.5, 0.25, 0.0] {
            assert_eq!(plain.process(x).to_bits(), shelf.process(x).to_bits());
        }
        let _ = shelf_delay(0.7, 0.3, 0.2);
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

    #[test]
    fn the_steep_shelf_is_what_it_says() {
        let rate = 48_000.0;
        let w = |hz: f32| TAU * hz / rate;
        let shelf = SteepShelf {
            corner: w(250.0),
            floor: 0.01,
        };
        let mut filter = shelf.biquad();
        // Its formula is the filter's response.
        for hz in [40.0_f32, 82.0, 250.0, 1_000.0, 4_000.0] {
            let got = filter.gain_at(rate, hz);
            assert!((got - shelf.gain(w(hz))).abs() < 1e-3, "{hz} Hz: {got}");
        }
        // Flat a third below the corner (a first-order shelf there takes about 5 %), at
        // its floor far above it.
        assert!(shelf.gain(w(82.0)) > 0.99);
        assert!((shelf.gain(w(8_000.0)) - 0.01).abs() < 0.002);
        // Its phase delay, measured: a sine's lag through it.
        let hz = 300.0;
        let n = 48_000;
        let (mut re, mut im) = (0.0_f32, 0.0_f32);
        for i in 0..n {
            let y = filter.process((w(hz) * i as f32).sin());
            if i >= n / 2 {
                re += y * (w(hz) * i as f32).sin();
                im += y * (w(hz) * i as f32).cos();
            }
        }
        let lag = -im.atan2(re) / w(hz);
        assert!(
            (lag - shelf.delay(w(hz))).abs() < 0.05,
            "{lag} {}",
            shelf.delay(w(hz))
        );
    }

    #[test]
    fn the_steep_shelf_design_meets_its_ratio_or_its_limit() {
        let (low, high, corner) = (0.0108, 0.262, 0.0324);
        for ratio in [0.9_f32, 0.5, 0.2] {
            let shelf = SteepShelf::for_ratio(low, high, corner, ratio, 0.0);
            let got = shelf.gain(high) / shelf.gain(low);
            assert!((got - ratio).abs() < 0.01, "{ratio}: {got}");
        }
        // Kept from taking too much off the fundamental.
        let kept = SteepShelf::for_ratio(low, high, low * 1.2, 0.01, 0.99);
        assert!(kept.gain(low) >= 0.99 - 1e-4, "{kept:?}");
        // Nonsense asks for nothing.
        assert_eq!(
            SteepShelf::for_ratio(low, high, corner, f32::NAN, 0.9).floor,
            1.0
        );
    }
}
