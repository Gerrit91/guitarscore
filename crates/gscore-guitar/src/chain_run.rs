//! Running a [`Chain`]: the built-in blocks sample by sample, an external block (a
//! plugin) on the whole buffer. The blocks between two external ones form a run, so a
//! chain without external blocks is still one pass per sample.

use crate::chain::{Block, Chain, SILENCE};
use crate::convolver::PARTITION;
use crate::knobs::BlockSettings;

impl Chain {
    /// Runs `input` (mono, from the strings) through the chain into `left` and `right`
    /// (overwritten). The slices should be equally long; the shortest decides.
    pub fn process(&mut self, input: &[f32], left: &mut [f32], right: &mut [f32]) {
        for ((&x, l), r) in input.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
            *l = x;
            *r = x;
        }
        let frames = input.len().min(left.len()).min(right.len());
        if let (Some(left), Some(right)) = (left.get_mut(..frames), right.get_mut(..frames)) {
            self.process_in_place(left, right, false);
        }
    }

    /// Runs a stereo signal (a SoundFont track's) through the chain, in place.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_in_place(left, right, true);
    }

    /// The chain on `left` and `right`, in place: mono (`stereo_in` false: only `left` is
    /// read, as from the strings) until a block makes it stereo, or stereo from the start.
    pub fn process_in_place(&mut self, left: &mut [f32], right: &mut [f32], stereo_in: bool) {
        let frames = left.len().min(right.len());
        let (Some(left), Some(right)) = (left.get_mut(..frames), right.get_mut(..frames)) else {
            return;
        };
        if !stereo_in {
            right.copy_from_slice(left);
        }
        let mut stereo = stereo_in;
        let mut start = 0;
        while start < self.blocks.len() {
            // The built-in blocks up to the next external one, then that one.
            let end = self
                .blocks
                .iter()
                .skip(start)
                .position(|block| matches!(block, Block::External(_)))
                .map_or(self.blocks.len(), |offset| start.saturating_add(offset));
            if let (Some(blocks), Some(settings)) = (
                self.blocks.get_mut(start..end),
                self.settings.get(start..end),
            ) {
                stereo = run(blocks, settings, left, right, stereo);
            }
            if let (Some(Block::External(instance)), Some(settings)) =
                (self.blocks.get_mut(end), self.settings.get(end))
                && !settings.bypass
            {
                // A plugin must not be fed what is not a number (it could stick in it).
                for sample in left.iter_mut().chain(right.iter_mut()) {
                    if !sample.is_finite() {
                        *sample = 0.0;
                    }
                }
                instance.0.process(left, right);
                stereo = true;
            }
            start = end.saturating_add(1);
        }
        let mut peak = 0.0_f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let [out_l, out_r] = if l.is_finite() && r.is_finite() {
                [*l * self.output, *r * self.output]
            } else {
                [0.0, 0.0]
            };
            *l = out_l;
            *r = out_r;
            peak = peak.max(out_l.abs()).max(out_r.abs());
        }
        if !peak.is_finite() {
            self.reset();
            return;
        }
        self.quiet = if peak > SILENCE {
            0
        } else {
            self.quiet.saturating_add(frames)
        };
    }
}

/// Runs the built-in `blocks` sample by sample over `left` and `right` (`right` equal to
/// `left` while the signal is mono). Returns whether the signal is stereo after them.
fn run(
    blocks: &mut [Block],
    settings: &[BlockSettings],
    left: &mut [f32],
    right: &mut [f32],
    stereo_in: bool,
) -> bool {
    let mut stereo_out = stereo_in;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let mut signal = [*l, *r];
        let mut stereo = stereo_in;
        for (block, settings) in blocks.iter_mut().zip(settings) {
            if !settings.bypass {
                sample(block, settings, &mut signal, &mut stereo);
            }
        }
        *l = signal[0];
        *r = signal[1];
        stereo_out = stereo;
    }
    stereo_out
}

/// One sample through one built-in block.
fn sample(block: &mut Block, settings: &BlockSettings, signal: &mut [f32; 2], stereo: &mut bool) {
    match block {
        // An external block runs on whole buffers (see `Chain::process_in_place`).
        Block::Empty | Block::External(_) => {}
        Block::Amp(sides) => {
            let [left, right] = &mut **sides;
            signal[0] = left.process(signal[0]);
            signal[1] = if *stereo {
                right.process(signal[1])
            } else {
                signal[0]
            };
        }
        Block::Compressor(compressor) => {
            let (left, right) = compressor.process(signal[0], signal[1], *stereo);
            *signal = [left, right];
        }
        Block::Eq(sides) => {
            let [left, right] = &mut **sides;
            signal[0] = left.process(signal[0]);
            signal[1] = if *stereo {
                right.process(signal[1])
            } else {
                signal[0]
            };
        }
        Block::Chorus(chorus) => {
            // The chorus needs both sides: a mono signal is made stereo first.
            let mut left = [signal[0]];
            let mut right = [signal[1]];
            chorus.process(&mut left, &mut right);
            *signal = [left[0], right[0]];
            *stereo = true;
        }
        Block::Drive(sides) => {
            let [left, right] = &mut **sides;
            signal[0] = left.process(signal[0]);
            signal[1] = if *stereo {
                right.process(signal[1])
            } else {
                signal[0]
            };
        }
        Block::Delay(echo) => {
            let (left, right) = echo.process(signal[0], signal[1], *stereo);
            // Ping-pong makes the sides differ; without it a mono signal stays
            // mono.
            *stereo = *stereo || settings.delay.ping_pong > 0.0;
            *signal = [left, right];
        }
        Block::Impulse(impulse) => {
            let knobs = settings.impulse;
            let gain = 10.0_f32.powf(knobs.gain_db / 20.0);
            let at = impulse.at;
            for side in 0..if *stereo { 2 } else { 1 } {
                let (Some(convolver), Some(dry), Some([low_cut, high_cut]), Some(value)) = (
                    impulse.convolvers.get_mut(side),
                    impulse.dry.get_mut(side),
                    impulse.cuts.get_mut(side),
                    signal.get_mut(side),
                ) else {
                    continue;
                };
                let wet = high_cut.process(low_cut.process(convolver.process(*value)));
                let delayed = dry.get(at).copied().unwrap_or(0.0);
                if let Some(slot) = dry.get_mut(at) {
                    *slot = *value;
                }
                *value = (delayed * (1.0 - knobs.mix) + wet * knobs.mix) * gain;
            }
            impulse.at = (at.saturating_add(1)) % PARTITION;
            if !*stereo {
                signal[1] = signal[0];
            }
        }
        Block::Room(room) => {
            let knobs = settings.room;
            let gain = 10.0_f32.powf(knobs.gain_db / 20.0);
            let at = room.at;
            // Both sides of the room hear the guitar (its two channels summed).
            let input = 0.5 * (signal[0] + signal[1]);
            let mut wet = [0.0_f32; 2];
            for (side, out) in wet.iter_mut().enumerate() {
                let (Some(convolver), Some([low_cut, high_cut])) =
                    (room.convolvers.get_mut(side), room.cuts.get_mut(side))
                else {
                    continue;
                };
                *out = high_cut.process(low_cut.process(convolver.process(input))) * gain;
            }
            let a = 0.5 + 0.5 * knobs.width;
            let b = 0.5 - 0.5 * knobs.width;
            let wet = [wet[0] * a + wet[1] * b, wet[1] * a + wet[0] * b];
            for ((value, dry), wet) in signal.iter_mut().zip(&mut room.dry).zip(wet) {
                let delayed = dry.get(at).copied().unwrap_or(0.0);
                if let Some(slot) = dry.get_mut(at) {
                    *slot = *value;
                }
                *value = delayed * (1.0 - knobs.mix) + wet * knobs.mix;
            }
            room.at = (at.saturating_add(1)) % PARTITION;
            *stereo = true;
        }
        Block::Reverb(block) => {
            let knobs = settings.reverb;
            let (wet_l, wet_r) = block.reverb.process(signal[0], signal[1]);
            let [[low_l, high_l], [low_r, high_r]] = &mut block.cuts;
            let wet_l = high_l.process(low_l.process(wet_l));
            let wet_r = high_r.process(low_r.process(wet_r));
            // Width blends the sides: 1 keeps them apart, 0 makes them one.
            let a = 0.5 + 0.5 * knobs.width;
            let b = 0.5 - 0.5 * knobs.width;
            let (wl, wr) = (wet_l * a + wet_r * b, wet_r * a + wet_l * b);
            *signal = [
                signal[0] * (1.0 - knobs.mix) + wl * knobs.mix,
                signal[1] * (1.0 - knobs.mix) + wr * knobs.mix,
            ];
            *stereo = true;
        }
    }
}
