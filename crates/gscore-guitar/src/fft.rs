//! A small in-place radix-2 FFT for the convolver. The tables are made once; transforming
//! neither allocates nor fails.

use std::f32::consts::TAU;

/// A complex number.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Complex {
    pub(crate) re: f32,
    pub(crate) im: f32,
}

impl Complex {
    pub(crate) fn new(re: f32, im: f32) -> Self {
        Complex { re, im }
    }

    pub(crate) fn mul(self, other: Complex) -> Complex {
        Complex {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn add(self, other: Complex) -> Complex {
        Complex::new(self.re + other.re, self.im + other.im)
    }

    fn sub(self, other: Complex) -> Complex {
        Complex::new(self.re - other.re, self.im - other.im)
    }
}

/// Where a transform spread over time ([`Fft::advance`]) is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FftProgress {
    phase: Phase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reorder(usize),
    Butterflies { length: usize, index: usize },
    Done,
}

impl FftProgress {
    /// At the start of a transform.
    pub(crate) fn start() -> Self {
        FftProgress {
            phase: Phase::Reorder(0),
        }
    }
}

/// The tables for transforms of one size (a power of two).
#[derive(Debug, Clone)]
pub(crate) struct Fft {
    size: usize,
    /// `e^(-i 2 pi k / size)` for `k` below `size / 2`.
    twiddles: Vec<Complex>,
    /// Where each index goes in bit-reversed order.
    reversed: Vec<usize>,
}

impl Fft {
    /// Tables for `size` points (rounded up to a power of two, at least 2).
    pub(crate) fn new(size: usize) -> Self {
        let size = size.max(2).next_power_of_two();
        let bits = size.trailing_zeros();
        let twiddles = (0..size / 2)
            .map(|k| {
                let angle = -TAU * k as f32 / size as f32;
                Complex::new(angle.cos(), angle.sin())
            })
            .collect();
        let reversed = (0..size)
            .map(|i| i.reverse_bits() >> (usize::BITS.saturating_sub(bits)))
            .collect();
        Fft {
            size,
            twiddles,
            reversed,
        }
    }

    /// Transforms `data` (of [`Fft::size`] points) forward in place.
    pub(crate) fn forward(&self, data: &mut [Complex]) {
        self.transform(data, false);
    }

    /// Transforms `data` back in place, scaled by `1 / size`.
    #[cfg(test)]
    pub(crate) fn inverse(&self, data: &mut [Complex]) {
        self.transform(data, true);
        let scale = 1.0 / self.size as f32;
        for value in data.iter_mut() {
            value.re *= scale;
            value.im *= scale;
        }
    }

    /// The steps a whole transform takes in [`Fft::advance`]: one per element for the
    /// reordering, one per butterfly.
    pub(crate) fn steps(&self) -> usize {
        let passes = self.size.trailing_zeros() as usize;
        self.size
            .saturating_add(passes.saturating_mul(self.size / 2))
    }

    /// Runs up to `budget` steps of a transform of `data` from `at`, so a long transform
    /// can be spread over time. Returns whether it is done; `at` keeps where it is. The
    /// inverse is not scaled.
    pub(crate) fn advance(
        &self,
        data: &mut [Complex],
        at: &mut FftProgress,
        budget: usize,
        inverse: bool,
    ) -> bool {
        if data.len() != self.size {
            return true;
        }
        let half_size = self.size / 2;
        let mut left = budget;
        while left > 0 {
            match at.phase {
                Phase::Reorder(i) => {
                    if i >= self.size {
                        at.phase = Phase::Butterflies {
                            length: 2,
                            index: 0,
                        };
                        continue;
                    }
                    if let Some(&j) = self.reversed.get(i)
                        && i < j
                    {
                        data.swap(i, j);
                    }
                    at.phase = Phase::Reorder(i.saturating_add(1));
                }
                Phase::Butterflies { length, index } => {
                    if length > self.size {
                        at.phase = Phase::Done;
                        continue;
                    }
                    if index >= half_size {
                        at.phase = Phase::Butterflies {
                            length: length.saturating_mul(2),
                            index: 0,
                        };
                        continue;
                    }
                    let half = length / 2;
                    let chunk = index.checked_div(half).unwrap_or(0);
                    let k = index.checked_rem(half).unwrap_or(0);
                    let a = chunk.saturating_mul(length).saturating_add(k);
                    let b = a.saturating_add(half);
                    let step = self.size.checked_div(length).unwrap_or(1);
                    let twiddle = self
                        .twiddles
                        .get(k.saturating_mul(step))
                        .copied()
                        .unwrap_or_default();
                    let twiddle = if inverse {
                        Complex::new(twiddle.re, -twiddle.im)
                    } else {
                        twiddle
                    };
                    if let (Some(&x), Some(&y)) = (data.get(a), data.get(b)) {
                        let t = y.mul(twiddle);
                        if let Some(slot) = data.get_mut(a) {
                            *slot = x.add(t);
                        }
                        if let Some(slot) = data.get_mut(b) {
                            *slot = x.sub(t);
                        }
                    }
                    at.phase = Phase::Butterflies {
                        length,
                        index: index.saturating_add(1),
                    };
                }
                Phase::Done => return true,
            }
            left = left.saturating_sub(1);
        }
        at.phase == Phase::Done
    }

    fn transform(&self, data: &mut [Complex], inverse: bool) {
        if data.len() != self.size {
            return;
        }
        for (i, &j) in self.reversed.iter().enumerate() {
            if i < j {
                data.swap(i, j);
            }
        }
        let mut length = 2;
        while length <= self.size {
            let half = length / 2;
            let step = self.size.checked_div(length).unwrap_or(1);
            for chunk in data.chunks_exact_mut(length) {
                let (low, high) = chunk.split_at_mut(half);
                for (k, (a, b)) in low.iter_mut().zip(high.iter_mut()).enumerate() {
                    let twiddle = self
                        .twiddles
                        .get(k.saturating_mul(step))
                        .copied()
                        .unwrap_or_default();
                    let twiddle = if inverse {
                        Complex::new(twiddle.re, -twiddle.im)
                    } else {
                        twiddle
                    };
                    let t = b.mul(twiddle);
                    *b = a.sub(t);
                    *a = a.add(t);
                }
            }
            length = length.saturating_mul(2);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn the_fft_matches_a_plain_dft_and_inverts() {
        let fft = Fft::new(16);
        let input: Vec<Complex> = (0..16)
            .map(|i| Complex::new((i as f32 * 0.7).sin(), (i as f32 * 0.3).cos()))
            .collect();
        let mut data = input.clone();
        fft.forward(&mut data);
        for (k, value) in data.iter().enumerate() {
            let mut sum = Complex::default();
            for (n, x) in input.iter().enumerate() {
                let angle = -TAU * (k * n) as f32 / 16.0;
                sum = sum.add(x.mul(Complex::new(angle.cos(), angle.sin())));
            }
            assert!((sum.re - value.re).abs() < 1e-3 && (sum.im - value.im).abs() < 1e-3);
        }
        fft.inverse(&mut data);
        for (a, b) in data.iter().zip(&input) {
            assert!((a.re - b.re).abs() < 1e-4 && (a.im - b.im).abs() < 1e-4);
        }
    }

    #[test]
    fn a_transform_spread_over_time_gives_the_same_result() {
        let fft = Fft::new(64);
        let input: Vec<Complex> = (0..64)
            .map(|i| Complex::new((i as f32 * 0.7).sin(), 0.0))
            .collect();
        let mut whole = input.clone();
        fft.forward(&mut whole);
        let mut spread = input.clone();
        let mut at = FftProgress::start();
        let mut calls = 0;
        while !fft.advance(&mut spread, &mut at, 7, false) {
            calls += 1;
        }
        assert!(calls > 10, "it took many calls");
        assert!(fft.steps() >= 64 + 6 * 32);
        for (a, b) in spread.iter().zip(&whole) {
            assert!((a.re - b.re).abs() < 1e-5 && (a.im - b.im).abs() < 1e-5);
        }
    }
}
