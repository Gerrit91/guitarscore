//! A large, lush stereo reverb, computed: a feedback delay network after the design
//! Geraint Luff published for Signalsmith Audio ("Let's write a reverb", MIT; no code
//! taken), the structure behind most modern algorithmic halls.
//!
//! ```text
//!  L,R ─► pre-delay ─► spread to 8 ─► diffuser ×4 ─┬──────────────────────► early ─┐
//!                                                  └─► + ─► 8 delays ─┬─► late ─┴─► L,R
//!                                                      ▲   (drifting) │
//!                                                      └─ mix ◄ decay ◄┘
//! ```
//!
//! - **Diffusers** smear each sound into a dense wash before it reaches the tail: each of
//!   four stages delays eight channels by different amounts, shuffles and flips them, and
//!   mixes them all into one another (a Hadamard matrix). Each stage doubles the echoes.
//! - **The tail** is eight delay lines feeding back into one another through a Householder
//!   matrix (which loses no energy); the loop gain per line sets the decay time exactly,
//!   and a shelf in the loop lets the highs die sooner (Damping). The lines' lengths
//!   drift slowly and differently (Modulation): the tail never rings at fixed pitches
//!   and gets the shimmer of a big hall instead of a metallic hum.
//! - **Size** scales every delay: small rooms to cathedrals. Size, decay and pre-delay
//!   can be turned while it plays; the lengths glide to their new values.
//!
//! Everything is allocated in [`Reverb::new`] for the largest size; processing never
//! allocates.

use std::f32::consts::{FRAC_1_SQRT_2, TAU};

use crate::delay::DelayLine;
use crate::knobs::ReverbKnobs;

/// The network's width.
const CHANNELS: usize = 8;
/// Diffusion stages.
const DIFFUSERS: usize = 4;
/// The tail's shortest line at size 0 and at size 1, in seconds; the others are up to
/// twice as long.
const ROOM: (f32, f32) = (0.025, 0.22);
/// The diffusion's length against the tail's shortest line.
const DIFFUSION: f32 = 0.6;
/// The longest pre-delay, seconds.
pub(crate) const MAX_PRE_DELAY: f32 = 0.25;
/// The deepest drift of a tail line, seconds (at Modulation 1).
const DRIFT: f32 = 0.004;
/// The drift's rates, Hz, one per line.
const DRIFT_RATES: [f32; CHANNELS] = [0.11, 0.17, 0.23, 0.31, 0.41, 0.53, 0.67, 0.83];
/// The corner of the damping shelf in the loop, Hz.
const DAMPING_CORNER: f32 = 2_500.0;
/// How quickly delay lengths glide to new values (seconds to get most of the way).
const GLIDE: f32 = 0.08;
/// The early part's level against the tail's.
const EARLY: f32 = 0.35;
/// The output's level: all wet, a few dB under the strings at any size and decay.
const OUTPUT: f32 = 0.6;
/// Below this a sample in the loop counts as zero (no denormals in a dying tail).
const FLUSH: f32 = crate::filters::FLUSH;

/// Where each channel's delay sits in its share of a diffusion stage (fixed, so the reverb
/// is the same every time; spread like a low-discrepancy sequence).
const SPREAD: [f32; CHANNELS] = [0.62, 0.24, 0.86, 0.48, 0.10, 0.72, 0.34, 0.96];
/// The channel order after each diffusion stage, and which channels flip.
const SHUFFLE: [[usize; CHANNELS]; DIFFUSERS] = [
    [3, 6, 1, 4, 7, 2, 5, 0],
    [5, 2, 7, 0, 3, 6, 1, 4],
    [6, 3, 0, 5, 2, 7, 4, 1],
    [1, 4, 7, 2, 5, 0, 3, 6],
];
const FLIPS: [u8; DIFFUSERS] = [0b1010_0110, 0b0101_1001, 0b1100_0011, 0b0011_1100];

/// The Hadamard matrix of size 8, in place, scaled to keep energy.
fn hadamard(values: &mut [f32; CHANNELS]) {
    for span in [1_usize, 2, 4] {
        for chunk in values.chunks_exact_mut(span.saturating_mul(2)) {
            let (a, b) = chunk.split_at_mut(span);
            for (x, y) in a.iter_mut().zip(b.iter_mut()) {
                let (sum, difference) = (*x + *y, *x - *y);
                *x = sum;
                *y = difference;
            }
        }
    }
    // 1 / sqrt(8).
    let scale = FRAC_1_SQRT_2 * 0.5;
    for value in values.iter_mut() {
        *value *= scale;
    }
}

/// The Householder reflection `I - 2/N 1 1ᵀ`, in place: every line hears all the others.
fn householder(values: &mut [f32; CHANNELS]) {
    let share = values.iter().sum::<f32>() * (2.0 / CHANNELS as f32);
    for value in values.iter_mut() {
        *value -= share;
    }
}

/// A diffusion stage's length as a share of the whole diffusion: each stage twice as
/// long as the one before, the last the whole.
fn stage_share(stage: usize) -> f32 {
    let halvings = DIFFUSERS.saturating_sub(stage.saturating_add(1));
    0.5_f32.powi(i32::try_from(halvings).unwrap_or(0))
}

/// `value` gliding toward `target`.
fn glide(value: &mut f32, target: f32, rate: f32) {
    *value += (target - *value) * rate;
}

/// One diffusion stage: eight delays.
#[derive(Debug, Clone)]
struct Diffuser {
    lines: [DelayLine; CHANNELS],
    lengths: [f32; CHANNELS],
    targets: [f32; CHANNELS],
}

/// The stereo reverb of one track (or of the reverb bus).
#[derive(Debug, Clone)]
pub(crate) struct Reverb {
    rate: f32,
    glide: f32,
    pre: [DelayLine; 2],
    pre_length: f32,
    diffusers: [Diffuser; DIFFUSERS],
    tail: [DelayLine; CHANNELS],
    lengths: [f32; CHANNELS],
    targets: [f32; CHANNELS],
    /// The knobs' targets: lengths in samples, decay in seconds.
    target_pre: f32,
    target_room: f32,
    /// Each tail line's loop gain for the lows and for the highs, from the decay time
    /// (and Damping) for its length.
    gains: [(f32, f32); CHANNELS],
    drift_depth: f32,
    /// Each line's drift, as a turning phasor (cos, sin), and its turn per sample.
    drift: [(f32, f32); CHANNELS],
    turn: [(f32, f32); CHANNELS],
    /// The damping shelf's lowpass in each line, and its coefficient.
    shelf: [f32; CHANNELS],
    shelf_a: f32,
}

impl Reverb {
    /// A reverb for `sample_rate`, with room for the largest size. Allocates.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let rate = sample_rate.max(1) as f32;
        let seconds = |s: f32| (s * rate).ceil() as usize;
        let longest_room = ROOM.1 * 2.0;
        let diffusion = ROOM.1 * DIFFUSION;
        let stage_seconds = |stage: usize| diffusion * stage_share(stage);
        let diffusers = std::array::from_fn(|stage| Diffuser {
            lines: std::array::from_fn(|_| DelayLine::new(seconds(stage_seconds(stage)))),
            lengths: [2.0; CHANNELS],
            targets: [2.0; CHANNELS],
        });
        let mut reverb = Reverb {
            rate,
            glide: 1.0 - (-1.0 / (GLIDE * rate)).exp(),
            pre: std::array::from_fn(|_| DelayLine::new(seconds(MAX_PRE_DELAY))),
            pre_length: 2.0,
            diffusers,
            tail: std::array::from_fn(|_| DelayLine::new(seconds(longest_room + DRIFT * 1.5))),
            lengths: [2.0; CHANNELS],
            targets: [2.0; CHANNELS],
            target_pre: 2.0,
            target_room: 0.0,
            gains: [(0.0, 0.0); CHANNELS],
            drift_depth: 0.0,
            drift: [(1.0, 0.0); CHANNELS],
            turn: DRIFT_RATES.map(|hz| {
                let w = TAU * hz / rate;
                (w.cos(), w.sin())
            }),
            shelf: [0.0; CHANNELS],
            shelf_a: (-TAU * DAMPING_CORNER / rate).exp(),
        };
        // Each line's drift starts at its own phase.
        for (index, phasor) in reverb.drift.iter_mut().enumerate() {
            let phase = TAU * index as f32 / CHANNELS as f32;
            *phasor = (phase.cos(), phase.sin());
        }
        reverb.set_knobs(&ReverbKnobs::default());
        // Start at the knobs' lengths rather than gliding there from nothing.
        reverb.pre_length = reverb.target_pre;
        reverb.lengths = reverb.targets;
        for diffuser in &mut reverb.diffusers {
            diffuser.lengths = diffuser.targets;
        }
        reverb
    }

    /// The tail line `index`'s length for a shortest line of `room` samples: spread
    /// between one and two times it, never in simple ratios.
    fn tail_length(room: f32, index: usize) -> f32 {
        room * (index as f32 / CHANNELS as f32).exp2() * (1.0 + 0.013 * index as f32)
    }

    /// The diffuser `stage`'s delay for channel `index`: each channel in its own share
    /// of the stage's length, so the echoes spread evenly.
    fn diffuser_length(room: f32, stage: usize, index: usize) -> f32 {
        let stage_length = room * DIFFUSION * stage_share(stage);
        let spread = SPREAD.get(index).copied().unwrap_or(0.5);
        (stage_length * (index as f32 + spread) / CHANNELS as f32).max(2.0)
    }

    /// New knob values (no allocation): the lengths glide there.
    pub(crate) fn set_knobs(&mut self, knobs: &ReverbKnobs) {
        let size = knobs.size.clamp(0.0, 1.0);
        // Rooms grow in proportion: equal steps of the knob, equal ratios of size.
        let room = ROOM.0 * (ROOM.1 / ROOM.0).powf(size);
        self.target_room = room * self.rate;
        self.target_pre =
            (knobs.pre_delay.clamp(0.0, MAX_PRE_DELAY * 1_000.0) / 1_000.0 * self.rate).max(2.0);
        let decay = knobs.decay.max(0.05);
        // The share of the decay time the highs keep (1 as long as the lows).
        let high_decay = 1.0 - 0.9 * knobs.damping.clamp(0.0, 1.0);
        self.drift_depth = knobs.modulation.clamp(0.0, 1.0) * DRIFT * self.rate;
        let room = self.target_room;
        for (index, target) in self.targets.iter_mut().enumerate() {
            *target = Self::tail_length(room, index);
        }
        for (stage, diffuser) in self.diffusers.iter_mut().enumerate() {
            for (index, target) in diffuser.targets.iter_mut().enumerate() {
                *target = Self::diffuser_length(room, stage, index);
            }
        }
        for (index, gains) in self.gains.iter_mut().enumerate() {
            // The length the line glides to, plus its drift's middle.
            let length = Self::tail_length(self.target_room, index) + self.drift_depth;
            let seconds = length / self.rate;
            *gains = (
                10.0_f32.powf(-3.0 * seconds / decay),
                10.0_f32.powf(-3.0 * seconds / (decay * high_decay)),
            );
        }
    }

    /// Silence throughout.
    pub(crate) fn clear(&mut self) {
        for line in self
            .pre
            .iter_mut()
            .chain(self.diffusers.iter_mut().flat_map(|d| d.lines.iter_mut()))
            .chain(self.tail.iter_mut())
        {
            line.clear();
        }
        self.shelf = [0.0; CHANNELS];
    }

    /// The wet signal for one input sample pair.
    pub(crate) fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let rate = self.glide;
        glide(&mut self.pre_length, self.target_pre, rate);
        let [pre_l, pre_r] = &mut self.pre;
        pre_l.push(left);
        pre_r.push(right);
        let (left, right) = (pre_l.read(self.pre_length), pre_r.read(self.pre_length));
        // Spread to eight: left on the even channels, right on the odd.
        let mut signal: [f32; CHANNELS] =
            std::array::from_fn(|i| if i % 2 == 0 { left } else { right });
        for (stage, diffuser) in self.diffusers.iter_mut().enumerate() {
            let mut delayed = [0.0_f32; CHANNELS];
            for (((line, length), target), (out, input)) in diffuser
                .lines
                .iter_mut()
                .zip(diffuser.lengths.iter_mut())
                .zip(diffuser.targets)
                .zip(delayed.iter_mut().zip(signal))
            {
                glide(length, target, rate);
                line.push(input);
                *out = line.read_linear(*length);
            }
            let order = SHUFFLE.get(stage).copied().unwrap_or([0; CHANNELS]);
            let flips = FLIPS.get(stage).copied().unwrap_or(0);
            for (index, (value, from)) in signal.iter_mut().zip(order).enumerate() {
                let sign = if flips & (1 << index) != 0 { -1.0 } else { 1.0 };
                *value = delayed.get(from).copied().unwrap_or(0.0) * sign;
            }
            hadamard(&mut signal);
        }
        let early = signal;
        // The tail: read every line, let it decay, mix, and feed it back with the input.
        let mut late = [0.0_f32; CHANNELS];
        let mut feedback = [0.0_f32; CHANNELS];
        for (index, ((((line, length), target), (out, back)), ((phasor, turn), (low, high)))) in
            self.tail
                .iter_mut()
                .zip(self.lengths.iter_mut())
                .zip(self.targets)
                .zip(late.iter_mut().zip(feedback.iter_mut()))
                .zip(self.drift.iter_mut().zip(self.turn).zip(self.gains))
                .enumerate()
        {
            glide(length, target, rate);
            let (c, s) = *phasor;
            let (tc, ts) = turn;
            let (c, s) = (c * tc - s * ts, c * ts + s * tc);
            // Keep the phasor on the unit circle.
            let norm = 1.5 - 0.5 * (c * c + s * s);
            *phasor = (c * norm, s * norm);
            let read = *length + self.drift_depth * (1.0 + phasor.1);
            *out = line.read(read);
            let Some(shelf) = self.shelf.get_mut(index) else {
                continue;
            };
            *shelf = crate::filters::flushed(*out + (*shelf - *out) * self.shelf_a);
            *back = *shelf * low + (*out - *shelf) * high;
        }
        householder(&mut feedback);
        for ((line, back), input) in self.tail.iter_mut().zip(feedback).zip(early) {
            let value = back + input;
            line.push(if value.abs() < FLUSH { 0.0 } else { value });
        }
        // Down to stereo: even channels left, odd right (four each).
        let mut out = [0.0_f32; 2];
        for (index, (late, early)) in late.iter().zip(early).enumerate() {
            if let Some(side) = out.get_mut(index % 2) {
                *side += late + early * EARLY;
            }
        }
        (out[0] * OUTPUT * 0.5, out[1] * OUTPUT * 0.5)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    const RATE: u32 = 48_000;

    fn knobs(size: f32, decay: f32) -> ReverbKnobs {
        ReverbKnobs {
            size,
            decay,
            ..ReverbKnobs::default()
        }
    }

    /// The impulse response's energy in windows of `window` samples.
    fn energy_curve(reverb: &mut Reverb, frames: usize, window: usize) -> Vec<f32> {
        let mut energies = Vec::new();
        let mut sum = 0.0;
        for i in 0..frames {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = reverb.process(x, x);
            sum += l * l + r * r;
            if (i + 1) % window == 0 {
                energies.push(sum);
                sum = 0.0;
            }
        }
        energies
    }

    #[test]
    fn the_matrices_keep_energy() {
        let mut values = [1.0, -2.0, 0.5, 3.0, 0.0, -1.0, 2.5, 0.25];
        let energy = |v: &[f32; CHANNELS]| v.iter().map(|x| x * x).sum::<f32>();
        let before = energy(&values);
        hadamard(&mut values);
        assert!((energy(&values) - before).abs() < 1e-4);
        householder(&mut values);
        assert!((energy(&values) - before).abs() < 1e-4);
    }

    #[test]
    fn the_decay_knob_sets_the_decay_time() {
        // Between 0.5 s and 1.5 s the level falls by 60 dB per decay time.
        for decay in [2.0_f32, 6.0] {
            let mut reverb = Reverb::new(RATE);
            reverb.set_knobs(&ReverbKnobs {
                damping: 0.0,
                modulation: 0.0,
                ..knobs(0.7, decay)
            });
            let curve = energy_curve(&mut reverb, 2 * RATE as usize, RATE as usize / 2);
            let fallen_db = 10.0 * (curve[1] / curve[3]).log10();
            let expected = 60.0 / decay;
            assert!(
                (fallen_db - expected).abs() < expected * 0.35,
                "decay {decay}: fell {fallen_db} dB in a second, expected {expected}"
            );
        }
    }

    #[test]
    fn a_tail_is_dense_soon_and_stays_finite() {
        // A cathedral: after 100 ms nearly every sample carries sound (no gaps between
        // echoes), the sides differ, and a long run stays finite and dies.
        let mut reverb = Reverb::new(RATE);
        reverb.set_knobs(&knobs(1.0, 20.0));
        let mut left = Vec::new();
        let mut right = Vec::new();
        for i in 0..RATE as usize {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = reverb.process(x, x);
            left.push(l);
            right.push(r);
        }
        let start = 4_800 + (MAX_PRE_DELAY * RATE as f32) as usize;
        let window = &left[start..start + 4_800];
        let peak = window.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
        let busy = window.iter().filter(|x| x.abs() > peak * 0.01).count();
        assert!(
            busy > window.len() * 8 / 10,
            "dense: {busy} of {}",
            window.len()
        );
        let difference: f32 = left.iter().zip(&right).map(|(l, r)| (l - r).abs()).sum();
        assert!(difference > 1.0, "the sides differ: {difference}");
        reverb.set_knobs(&knobs(1.0, 0.5));
        for _ in 0..(RATE as usize * 5) {
            let (l, r) = reverb.process(0.0, 0.0);
            assert!(l.is_finite() && r.is_finite());
        }
        let (l, r) = reverb.process(0.0, 0.0);
        assert!(l.abs() < 1e-6 && r.abs() < 1e-6, "died: {l} {r}");
    }

    #[test]
    fn damping_takes_the_highs_first_and_pre_delay_waits() {
        let tail_brightness = |damping: f32| {
            let mut reverb = Reverb::new(RATE);
            reverb.set_knobs(&ReverbKnobs {
                damping,
                ..knobs(0.6, 4.0)
            });
            let mut previous = 0.0;
            let (mut diff, mut total) = (0.0_f32, 0.0_f32);
            for i in 0..(RATE as usize * 2) {
                let x = if i == 0 { 1.0 } else { 0.0 };
                let (l, _) = reverb.process(x, x);
                if i > RATE as usize {
                    diff += (l - previous).powi(2);
                    total += l * l;
                }
                previous = l;
            }
            diff / total.max(1e-20)
        };
        assert!(tail_brightness(0.9) < 0.5 * tail_brightness(0.0));
        let mut reverb = Reverb::new(RATE);
        reverb.set_knobs(&ReverbKnobs {
            pre_delay: 100.0,
            ..ReverbKnobs::default()
        });
        // Let the pre-delay glide to its length.
        for _ in 0..RATE {
            reverb.process(0.0, 0.0);
        }
        let first = (0..RATE as usize)
            .position(|i| {
                let x = if i == 0 { 1.0 } else { 0.0 };
                let (l, r) = reverb.process(x, x);
                l.abs().max(r.abs()) > 1e-4
            })
            .unwrap_or(usize::MAX);
        assert!(first >= 4_800, "nothing before 100 ms: {first}");
    }
}
