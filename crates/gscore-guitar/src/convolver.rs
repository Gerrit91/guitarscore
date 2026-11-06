//! Convolution with an impulse response (a guitar body, a cabinet, a room), cheap enough
//! for real time: the response is cut into partitions whose spectra are made once
//! (partitioned overlap-save). The beginning of the response uses small partitions, so the
//! output lags the input by only [`PARTITION`] samples (2.7 ms at 48 kHz); later parts use
//! ever larger ones ([`STAGES`]), so a room's long tail costs a few hundred operations per
//! sample instead of thousands.
//!
//! A large partition's work (its transforms and a multiply-add per partition of the
//! response) would all fall on one sample and make that audio block late (the stutter of
//! long rooms). So the large stages are one block later than they need to be and spread
//! their work over that block, a slice per sample: the same work, no spike. A stage that is
//! `L` samples late (`T` for the first stage, `2 T` for a spread one, `T` being its
//! partition) covers the response from `L - PARTITION` on, so all stages line up at the
//! first stage's lag.

use std::sync::Arc;

use crate::fft::{Complex, Fft, FftProgress};

/// Samples per partition of the first stage: the convolver's lag.
pub(crate) const PARTITION: usize = 128;
/// Partition sizes of the stages, smallest first; all but the first spread their work.
const STAGES: [usize; 3] = [PARTITION, 1_024, 8_192];

/// A spread stage works on its frame every this many samples.
const SLICE: usize = 16;

/// How late a stage's output is: one partition, or two for a stage that spreads its work.
fn lag(index: usize, block: usize) -> usize {
    if index == 0 {
        block
    } else {
        block.saturating_mul(2)
    }
}

/// The partition spectra of one stage.
#[derive(Debug)]
struct StageSpectra {
    block: usize,
    spread: bool,
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
            let start = lag(index, block).saturating_sub(PARTITION);
            let end = STAGES
                .get(index.saturating_add(1))
                .map_or(response.len(), |&next| {
                    lag(index.saturating_add(1), next).saturating_sub(PARTITION)
                })
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
                spread: index > 0,
                fft,
                partitions,
            });
        }
        Spectra { stages }
    }
}

/// Where a stage's spread-out frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    Idle,
    Forward(FftProgress),
    Multiply { partition: usize, bin: usize },
    Inverse(FftProgress),
    Ready,
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
    /// The output being read out sample by sample.
    output: Vec<f32>,
    /// A spread stage's frame in progress, and how many steps it takes per sample.
    job: Job,
    quota: usize,
}

impl Stage {
    fn new(spectra: &StageSpectra) -> Self {
        let size = spectra.block.saturating_mul(2);
        let partitions = spectra.partitions.len().max(1);
        // Both transforms and every partition's multiply-add, spread over a block (a few
        // samples short of it, so it is surely done in time).
        let steps = spectra
            .fft
            .steps()
            .saturating_mul(2)
            .saturating_add(partitions.saturating_mul(size));
        let quota = steps
            .div_ceil(spectra.block.saturating_sub(8).max(1))
            .max(1);
        Stage {
            input: vec![0.0; size],
            filled: 0,
            history: vec![vec![Complex::default(); size]; partitions],
            newest: 0,
            work: vec![Complex::default(); size],
            sum: vec![Complex::default(); size],
            output: vec![0.0; spectra.block],
            job: Job::Idle,
            quota,
        }
    }

    fn process(&mut self, spectra: &StageSpectra, sample: f32) -> f32 {
        let block = spectra.block;
        let out = self.output.get(self.filled).copied().unwrap_or(0.0);
        if let Some(slot) = self.input.get_mut(block.saturating_add(self.filled)) {
            *slot = sample;
        }
        self.filled = self.filled.saturating_add(1);
        // Resumed every few samples, a few slices at a time: the same spread, less
        // bookkeeping.
        if spectra.spread && self.filled.is_multiple_of(SLICE) {
            self.advance(spectra, self.quota.saturating_mul(SLICE));
        }
        if self.filled >= block {
            self.filled = 0;
            if spectra.spread {
                // The frame begun a block ago is done (finish what is left, should the
                // quota have fallen short), and is read out during the next block.
                self.advance(spectra, usize::MAX);
                self.take_output(spectra);
                self.begin(spectra);
            } else {
                self.begin(spectra);
                self.advance(spectra, usize::MAX);
                self.take_output(spectra);
            }
        }
        out
    }

    /// Starts a frame: the last two blocks of input to transform.
    fn begin(&mut self, spectra: &StageSpectra) {
        let block = spectra.block;
        for (slot, &sample) in self.work.iter_mut().zip(&self.input) {
            *slot = Complex::new(sample, 0.0);
        }
        let size = block.saturating_mul(2);
        self.input.copy_within(block..size, 0);
        self.job = Job::Forward(FftProgress::start());
    }

    /// Works on the frame for up to `budget` steps.
    fn advance(&mut self, spectra: &StageSpectra, budget: usize) {
        let mut left = budget;
        let count = self.history.len().max(1);
        while left > 0 {
            match self.job {
                Job::Idle | Job::Ready => return,
                Job::Forward(mut at) => {
                    let done = spectra.fft.advance(&mut self.work, &mut at, left, false);
                    left = left.saturating_sub(spectra.fft.steps().min(left));
                    if done {
                        self.newest = self
                            .newest
                            .saturating_add(1)
                            .checked_rem(count)
                            .unwrap_or(0);
                        if let Some(spectrum) = self.history.get_mut(self.newest) {
                            spectrum.copy_from_slice(&self.work);
                        }
                        self.sum.fill(Complex::default());
                        self.job = Job::Multiply {
                            partition: 0,
                            bin: 0,
                        };
                    } else {
                        self.job = Job::Forward(at);
                    }
                }
                Job::Multiply { partition, bin } => {
                    let Some(response) = spectra.partitions.get(partition) else {
                        self.job = Job::Inverse(FftProgress::start());
                        continue;
                    };
                    let index = self
                        .newest
                        .saturating_add(count)
                        .saturating_sub(partition.checked_rem(count).unwrap_or(0))
                        .checked_rem(count)
                        .unwrap_or(0);
                    let size = self.sum.len();
                    let end = bin.saturating_add(left).min(size);
                    if let (Some(frame), Some(sum), Some(response)) = (
                        self.history
                            .get(index)
                            .and_then(|frame| frame.get(bin..end)),
                        self.sum.get_mut(bin..end),
                        response.get(bin..end),
                    ) {
                        for ((sum, x), h) in sum.iter_mut().zip(frame).zip(response) {
                            let product = x.mul(*h);
                            sum.re += product.re;
                            sum.im += product.im;
                        }
                    }
                    left = left.saturating_sub(end.saturating_sub(bin).max(1));
                    self.job = if end >= size {
                        Job::Multiply {
                            partition: partition.saturating_add(1),
                            bin: 0,
                        }
                    } else {
                        Job::Multiply {
                            partition,
                            bin: end,
                        }
                    };
                }
                Job::Inverse(mut at) => {
                    let done = spectra.fft.advance(&mut self.sum, &mut at, left, true);
                    left = left.saturating_sub(spectra.fft.steps().min(left));
                    self.job = if done { Job::Ready } else { Job::Inverse(at) };
                }
            }
        }
    }

    /// The finished frame's second half, scaled, as the output to read out next.
    fn take_output(&mut self, spectra: &StageSpectra) {
        if self.job != Job::Ready {
            return;
        }
        let block = spectra.block;
        let scale = 1.0 / self.sum.len().max(1) as f32;
        for (out, value) in self
            .output
            .iter_mut()
            .zip(self.sum.get(block..).unwrap_or_default())
        {
            *out = value.re * scale;
        }
        self.job = Job::Idle;
    }

    fn reset(&mut self) {
        self.input.fill(0.0);
        for spectrum in &mut self.history {
            spectrum.fill(Complex::default());
        }
        self.output.fill(0.0);
        self.filled = 0;
        self.job = Job::Idle;
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
        // Past the third stage's start (16256 samples).
        check(20_000, 40_000);
    }
}
