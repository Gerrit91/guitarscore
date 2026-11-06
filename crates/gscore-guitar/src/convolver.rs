//! Convolution with an impulse response (a guitar body, a cabinet, a room), cheap enough
//! for real time: the response is cut into partitions whose spectra are made once
//! (partitioned overlap-save). The beginning of the response uses small partitions, so the
//! output lags the input by only [`PARTITION`] samples (2.7 ms at 48 kHz); later parts use
//! ever larger ones ([`STAGES`]), so a room's long tail costs a few hundred operations per
//! sample instead of thousands.
//!
//! A stage with partitions of `T` samples is `T` samples late; it covers the response from
//! `T - PARTITION` on (up to where the next stage starts), so all stages line up at the
//! first stage's lag.

use std::sync::Arc;

use crate::fft::{Complex, Fft};

/// Samples per partition of the first stage: the convolver's lag.
pub(crate) const PARTITION: usize = 128;
/// Partition sizes of the stages, smallest first.
const STAGES: [usize; 3] = [PARTITION, 1_024, 8_192];

/// The partition spectra of one stage.
#[derive(Debug)]
struct StageSpectra {
    block: usize,
    fft: Fft,
    partitions: Vec<Vec<Complex>>,
}

/// The spectra of an impulse response, shared by every track that uses it.
#[derive(Debug)]
pub(crate) struct Spectra {
    stages: Vec<StageSpectra>,
}

impl Spectra {
    /// The partition spectra of `response` (samples at the engine's rate).
    pub(crate) fn new(response: &[f32]) -> Self {
        let mut stages = Vec::new();
        for (index, &block) in STAGES.iter().enumerate() {
            let start = block.saturating_sub(PARTITION);
            let end = STAGES
                .get(index.saturating_add(1))
                .map_or(response.len(), |next| next.saturating_sub(PARTITION))
                .min(response.len());
            let Some(part) = response.get(start..end).filter(|part| !part.is_empty()) else {
                break;
            };
            let size = block.saturating_mul(2);
            let fft = Fft::new(size);
            let partitions = part
                .chunks(block)
                .map(|chunk| {
                    let mut spectrum = vec![Complex::default(); size];
                    for (slot, &sample) in spectrum.iter_mut().zip(chunk) {
                        slot.re = if sample.is_finite() { sample } else { 0.0 };
                    }
                    fft.forward(&mut spectrum);
                    spectrum
                })
                .collect();
            stages.push(StageSpectra {
                block,
                fft,
                partitions,
            });
        }
        Spectra { stages }
    }
}

/// One stage running.
#[derive(Debug)]
struct Stage {
    /// The last two blocks of input (the older first).
    input: Vec<f32>,
    filled: usize,
    /// The spectra of past input frames, newest at `newest`.
    history: Vec<Vec<Complex>>,
    newest: usize,
    work: Vec<Complex>,
    sum: Vec<Complex>,
    /// The last frame's output, read out sample by sample.
    output: Vec<f32>,
}

impl Stage {
    fn new(spectra: &StageSpectra) -> Self {
        let size = spectra.block.saturating_mul(2);
        Stage {
            input: vec![0.0; size],
            filled: 0,
            history: vec![vec![Complex::default(); size]; spectra.partitions.len().max(1)],
            newest: 0,
            work: vec![Complex::default(); size],
            sum: vec![Complex::default(); size],
            output: vec![0.0; spectra.block],
        }
    }

    fn process(&mut self, spectra: &StageSpectra, sample: f32) -> f32 {
        let block = spectra.block;
        let out = self.output.get(self.filled).copied().unwrap_or(0.0);
        if let Some(slot) = self.input.get_mut(block.saturating_add(self.filled)) {
            *slot = sample;
        }
        self.filled = self.filled.saturating_add(1);
        if self.filled >= block {
            self.filled = 0;
            self.frame(spectra);
        }
        out
    }

    /// Transform the last two blocks of input, multiply with every response partition
    /// against the matching past frame, sum, transform back, keep the second half.
    fn frame(&mut self, spectra: &StageSpectra) {
        let block = spectra.block;
        let count = self.history.len().max(1);
        self.newest = self
            .newest
            .saturating_add(1)
            .checked_rem(count)
            .unwrap_or(0);
        for (slot, &sample) in self.work.iter_mut().zip(&self.input) {
            *slot = Complex::new(sample, 0.0);
        }
        spectra.fft.forward(&mut self.work);
        if let Some(spectrum) = self.history.get_mut(self.newest) {
            spectrum.copy_from_slice(&self.work);
        }
        self.sum.fill(Complex::default());
        for (age, response) in spectra.partitions.iter().enumerate() {
            let index = self
                .newest
                .saturating_add(count)
                .saturating_sub(age.checked_rem(count).unwrap_or(0))
                .checked_rem(count)
                .unwrap_or(0);
            let Some(frame) = self.history.get(index) else {
                continue;
            };
            for ((sum, x), h) in self.sum.iter_mut().zip(frame).zip(response) {
                let product = x.mul(*h);
                sum.re += product.re;
                sum.im += product.im;
            }
        }
        spectra.fft.inverse(&mut self.sum);
        for (out, value) in self
            .output
            .iter_mut()
            .zip(self.sum.get(block..).unwrap_or_default())
        {
            *out = value.re;
        }
        let size = block.saturating_mul(2);
        self.input.copy_within(block..size, 0);
    }

    fn reset(&mut self) {
        self.input.fill(0.0);
        for spectrum in &mut self.history {
            spectrum.fill(Complex::default());
        }
        self.output.fill(0.0);
        self.filled = 0;
    }
}

/// One running convolution.
#[derive(Debug)]
pub(crate) struct Convolver {
    spectra: Arc<Spectra>,
    stages: Vec<Stage>,
}

impl Convolver {
    /// A convolver with `spectra`, its buffers made now.
    pub(crate) fn new(spectra: Arc<Spectra>) -> Self {
        let stages = spectra.stages.iter().map(Stage::new).collect();
        Convolver { spectra, stages }
    }

    /// Convolves one sample (the output is [`PARTITION`] samples late).
    pub(crate) fn process(&mut self, sample: f32) -> f32 {
        let mut out = 0.0;
        for (stage, spectra) in self.stages.iter_mut().zip(&self.spectra.stages) {
            out += stage.process(spectra, sample);
        }
        out
    }

    /// Silence in the convolution's memory.
    pub(crate) fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]

    use super::*;

    fn check(response_length: usize, input_length: usize) {
        let response: Vec<f32> = (0..response_length)
            .map(|i| ((i as f32 * 0.37).sin()) * (-(i as f32) / 3_000.0).exp())
            .collect();
        let input: Vec<f32> = (0..input_length)
            .map(|i| ((i * 7919) % 97) as f32 / 48.0 - 1.0)
            .collect();
        let mut convolver = Convolver::new(Arc::new(Spectra::new(&response)));
        let output: Vec<f32> = input.iter().map(|&x| convolver.process(x)).collect();
        for n in (PARTITION..input.len()).step_by(7) {
            let t = n - PARTITION;
            let expected: f32 = (0..=t.min(response.len() - 1))
                .map(|k| response[k] * input[t - k])
                .sum();
            let tolerance = 2e-3 * expected.abs().max(1.0);
            assert!(
                (output[n] - expected).abs() < tolerance,
                "response {response_length}, sample {n}: {} vs {expected}",
                output[n]
            );
        }
    }

    #[test]
    fn a_short_response_matches_a_direct_convolution() {
        check(400, 1_500);
    }

    #[test]
    fn a_long_response_over_every_stage_matches_too() {
        // Past the third stage's start (8064 samples).
        check(9_500, 12_000);
    }
}
