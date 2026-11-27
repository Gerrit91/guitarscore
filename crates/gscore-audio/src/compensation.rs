//! Latency compensation (M24): a plugin hands its sound back late, so every other track
//! (and the metronome's click) is held back by as much, and they all stay together. Each
//! strip has a [`Delay`] after its chain; the mixer sets it to the largest latency of the
//! tracks playing minus the strip's own.
//!
//! The lines are made once with the mixer (nothing allocates while playing). A latency
//! longer than [`MAX_COMPENSATION`] is made up only that far.

use crate::BLOCK;

/// The longest delay made up, in frames: 8192 (170 ms at 48 kHz, 85 ms at 96 kHz). The
/// browser has no plugins, so its lines hold no more than a block.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_COMPENSATION: usize = 8192;
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_COMPENSATION: usize = 0;

/// The ring's length: room for the longest delay and one block written ahead of it.
const RING: usize = MAX_COMPENSATION + BLOCK;

/// A stereo delay line of up to [`MAX_COMPENSATION`] frames.
#[derive(Debug, Clone)]
pub(crate) struct Delay {
    sides: [Vec<f32>; 2],
    /// Where the next frame is written.
    at: usize,
    /// Frames written since the last one that was not silent: while fewer than the delay,
    /// the line still holds sound that must come out.
    quiet: usize,
}

impl Default for Delay {
    fn default() -> Self {
        Self {
            sides: [vec![0.0; RING], vec![0.0; RING]],
            at: 0,
            quiet: usize::MAX,
        }
    }
}

impl Delay {
    /// Delays `left` and `right` (equally long, at most a block) by `frames`, in place.
    /// Never allocates.
    pub(crate) fn process(&mut self, left: &mut [f32], right: &mut [f32], frames: usize) {
        let delay = frames.min(MAX_COMPENSATION);
        let len = left.len().min(right.len());
        let [ring_left, ring_right] = &mut self.sides;
        for (l, r) in left.iter_mut().zip(right.iter_mut()).take(len) {
            let read = (self.at + RING - delay) % RING;
            let (Some(write_l), Some(write_r)) =
                (ring_left.get_mut(self.at), ring_right.get_mut(self.at))
            else {
                return;
            };
            *write_l = *l;
            *write_r = *r;
            self.quiet = if *l != 0.0 || *r != 0.0 {
                0
            } else {
                self.quiet.saturating_add(1)
            };
            *l = ring_left.get(read).copied().unwrap_or(0.0);
            *r = ring_right.get(read).copied().unwrap_or(0.0);
            self.at = (self.at + 1) % RING;
        }
    }

    /// Whether sound is still on its way through a delay of `frames` (the strip must run
    /// on, with silence going in, until it is out).
    pub(crate) fn holds(&self, frames: usize) -> bool {
        frames > 0 && self.quiet < frames.min(MAX_COMPENSATION)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn sound_comes_out_the_set_number_of_frames_later() {
        let mut delay = Delay::default();
        let mut out = Vec::new();
        for block in 0..4 {
            let mut left: Vec<f32> = (0..BLOCK).map(|i| (block * BLOCK + i) as f32).collect();
            let mut right = left.clone();
            delay.process(&mut left, &mut right, 100);
            out.extend(left);
        }
        assert!(out[..100].iter().all(|&s| s == 0.0));
        for (index, sample) in out.iter().enumerate().skip(100) {
            assert_eq!(*sample, (index - 100) as f32);
        }
    }

    #[test]
    fn no_delay_passes_the_sound_and_a_long_one_is_capped() {
        let mut delay = Delay::default();
        let (mut left, mut right) = ([1.0_f32; BLOCK], [2.0_f32; BLOCK]);
        delay.process(&mut left, &mut right, 0);
        assert!(left.iter().all(|&s| s == 1.0) && right.iter().all(|&s| s == 2.0));
        assert!(!delay.holds(0));
        // Far past the maximum: made up as far as it goes, never a panic.
        delay.process(&mut left, &mut right, usize::MAX);
        assert!(delay.holds(usize::MAX));
    }

    #[test]
    fn a_line_holds_sound_until_it_is_out() {
        let mut delay = Delay::default();
        let (mut left, mut right) = ([0.5_f32; BLOCK], [0.5_f32; BLOCK]);
        delay.process(&mut left, &mut right, 3 * BLOCK);
        let mut blocks = 0;
        while delay.holds(3 * BLOCK) {
            let (mut left, mut right) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
            delay.process(&mut left, &mut right, 3 * BLOCK);
            blocks += 1;
            assert!(blocks < 10);
        }
        assert_eq!(blocks, 3);
    }
}
