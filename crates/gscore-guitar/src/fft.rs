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
    pub(crate) fn inverse(&self, data: &mut [Complex]) {
        self.transform(data, true);
        let scale = 1.0 / self.size as f32;
        for value in data.iter_mut() {
            value.re *= scale;
            value.im *= scale;
        }
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
}
