//! Running a nonlinearity (a clipping stage) at four times the sample rate: the harmonics
//! it makes above half the sample rate are filtered away instead of folding back down as
//! harsh, inharmonic fizz. Zero-stuffing up, a sixth-order Butterworth lowpass on the way
//! up and on the way down.

use crate::filters::Biquad;

/// The oversampling factor.
pub(crate) const FACTOR: usize = 4;
/// The anti-aliasing lowpass's corner, as a share of the base rate.
const ALIAS_CUT: f32 = 0.42;
/// Three Butterworth sections: sixth order.
const BUTTERWORTH_6: [f32; 3] = [0.517_638, std::f32::consts::FRAC_1_SQRT_2, 1.931_852];

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Oversampler {
    up: [Biquad; 3],
    down: [Biquad; 3],
}

impl Oversampler {
    /// An oversampler for `sample_rate` (the base rate).
    pub(crate) fn new(sample_rate: f32) -> Self {
        let fast = sample_rate * FACTOR as f32;
        let cut = (sample_rate * ALIAS_CUT).min(20_000.0);
        let lowpass = BUTTERWORTH_6.map(|q| Biquad::lowpass(fast, cut, q));
        Oversampler {
            up: lowpass,
            down: lowpass,
        }
    }

    /// `x` through `stage` at the fast rate, back at the base rate.
    pub(crate) fn run(&mut self, x: f32, mut stage: impl FnMut(f32) -> f32) -> f32 {
        let mut out = 0.0;
        for step in 0..FACTOR {
            // One sample in four, four times as loud, then the lowpass.
            let stuffed = if step == 0 { x * FACTOR as f32 } else { 0.0 };
            let up = self
                .up
                .iter_mut()
                .fold(stuffed, |value, filter| filter.process(value));
            let shaped = stage(up);
            out = self
                .down
                .iter_mut()
                .fold(shaped, |value, filter| filter.process(value));
        }
        out
    }

    /// Silence in its filters.
    pub(crate) fn reset(&mut self) {
        for filter in self.up.iter_mut().chain(&mut self.down) {
            filter.reset();
        }
    }
}
