//! The effect chain after the strings: [`BLOCKS`] places in a fixed order, each empty or
//! holding a block (an impulse response for a cabinet or a body, a room's impulse response,
//! an algorithmic reverb, a computed amp or overdrive pedal). The signal is mono from the strings and becomes
//! stereo at the first block that makes it so (a room or the reverb).
//!
//! Two kinds of change: which blocks are where (and which impulse response, or a room's
//! length) needs new buffers, so a [`ChainSpec`] is prepared off the audio thread
//! ([`PreparedChain`]) and swapped in; turning a knob only changes [`BlockSettings`], which
//! are plain values the audio thread takes at once ([`Chain::set_settings`]).

use std::sync::Arc;

use crate::amp::Amp;
use crate::chorus::Chorus;
use crate::compressor::Compressor;
use crate::convolver::{Convolver, PARTITION, Spectra};
use crate::drive::Drive;
use crate::eq::Eq;
use crate::filters::Biquad;
use crate::impulse::ImpulseResponse;
use crate::knobs::BlockSettings;
use crate::reverb::Reverb;

/// The places for blocks after the strings.
pub const BLOCKS: usize = 5;
/// Below this level the chain's output counts as silent...
const SILENCE: f32 = 1e-6;
/// ...and after this long of silence (seconds) the tail is over: a room's response may
/// have quiet stretches (a pre-delay) shorter than this.
const QUIET_HOLD: f32 = 0.5;
/// Quality of the cut filters (Butterworth).
const CUT_Q: f32 = 0.707;

/// What a place holds.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    /// An impulse response: a cabinet after an amp, or an acoustic guitar's body (mono).
    Impulse(Arc<ImpulseResponse>),
    /// A room's impulse response: a convolution reverb (stereo when the response is),
    /// its decay stretched by `length` (1 as recorded; see [`BlockKind::room`]).
    Room {
        response: Arc<ImpulseResponse>,
        length: f32,
    },
    /// A stereo room reverb, computed (no response needed).
    Reverb,
    /// A guitar amp, computed (see [`crate::AmpKnobs`]): mono, or each side alike once
    /// the signal is stereo.
    Amp,
    /// An overdrive pedal, computed (see [`crate::DriveKnobs`]): mono, or each side alike.
    Drive,
    /// A compressor, computed (see [`crate::CompressorKnobs`]): both sides linked.
    Compressor,
    /// The 4-band parametric EQ.
    Eq,
    /// The stereo chorus.
    Chorus,
}

impl BlockKind {
    /// A room block: `response`'s decay stretched by `length` (clamped to
    /// [`crate::LENGTH_RANGE`], 1 as recorded), as the Room block's Length knob sets it.
    pub fn room(response: Arc<ImpulseResponse>, length: f32) -> Self {
        BlockKind::Room { response, length }
    }
}

/// Which block is in each place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChainSpec {
    pub blocks: [Option<BlockKind>; BLOCKS],
}

/// A chain spec prepared for one sample rate: the impulse responses' spectra, shared by
/// every track that plays through it.
#[derive(Debug, Clone)]
pub struct PreparedChain {
    sample_rate: u32,
    blocks: [Option<PreparedBlock>; BLOCKS],
}

#[derive(Debug, Clone)]
enum PreparedBlock {
    Impulse(Arc<Spectra>),
    Room([Arc<Spectra>; 2]),
    Reverb,
    Amp,
    Drive,
    Compressor,
    Eq,
    Chorus,
}

impl ChainSpec {
    /// Prepares the chain for `sample_rate` (resamples and transforms the responses).
    /// Allocates; call it off the audio thread.
    pub fn prepare(&self, sample_rate: u32) -> PreparedChain {
        let blocks = self.blocks.clone().map(|block| {
            block.map(|block| match block {
                BlockKind::Impulse(response) => PreparedBlock::Impulse(Arc::new(Spectra::new(
                    &response.mono_at_rate(sample_rate),
                ))),
                BlockKind::Room { response, length } => {
                    // A length other than the recording's makes a new response, its decay
                    // stretched (see `ImpulseResponse::lengthened`). That needs a valid
                    // response, which this one is; should it ever fail, the recording plays
                    // as it is rather than the place falling silent.
                    let stretched = ((length - 1.0).abs() >= 1e-3)
                        .then(|| response.lengthened(length).ok())
                        .flatten();
                    let response = stretched.as_ref().unwrap_or(&response);
                    PreparedBlock::Room([0, 1].map(|side| {
                        Arc::new(Spectra::new(&response.channel_at_rate(side, sample_rate)))
                    }))
                }
                BlockKind::Reverb => PreparedBlock::Reverb,
                BlockKind::Amp => PreparedBlock::Amp,
                BlockKind::Drive => PreparedBlock::Drive,
                BlockKind::Compressor => PreparedBlock::Compressor,
                BlockKind::Eq => PreparedBlock::Eq,
                BlockKind::Chorus => PreparedBlock::Chorus,
            })
        });
        PreparedChain {
            sample_rate,
            blocks,
        }
    }
}

impl PreparedChain {
    /// A chain for one track, its buffers made now. Allocates.
    pub fn instance(&self, settings: &[BlockSettings; BLOCKS]) -> Chain {
        let rate = self.sample_rate as f32;
        let blocks = self.blocks.clone().map(|block| match block {
            None => Block::Empty,
            Some(PreparedBlock::Impulse(spectra)) => Block::Impulse(Box::new(ImpulseBlock {
                convolvers: [
                    Convolver::new(Arc::clone(&spectra)),
                    Convolver::new(spectra),
                ],
                dry: [vec![0.0; PARTITION], vec![0.0; PARTITION]],
                at: 0,
                cuts: [[Biquad::default(); 2]; 2],
            })),
            Some(PreparedBlock::Room(sides)) => Block::Room(Box::new(ImpulseBlock {
                convolvers: sides.map(Convolver::new),
                dry: [vec![0.0; PARTITION], vec![0.0; PARTITION]],
                at: 0,
                cuts: [[Biquad::default(); 2]; 2],
            })),
            Some(PreparedBlock::Reverb) => Block::Reverb(Box::new(ReverbBlock {
                reverb: Reverb::new(self.sample_rate),
                cuts: [[Biquad::default(); 2]; 2],
            })),
            Some(PreparedBlock::Amp) => Block::Amp(Box::new(
                [(); 2].map(|()| Amp::new(rate, crate::AmpKnobs::default())),
            )),
            Some(PreparedBlock::Compressor) => Block::Compressor(Box::new(Compressor::new(
                rate,
                crate::CompressorKnobs::default(),
            ))),
            Some(PreparedBlock::Eq) => Block::Eq(Box::new(
                [(); 2].map(|()| Eq::new(rate, crate::EqKnobs::default())),
            )),
            Some(PreparedBlock::Chorus) => {
                Block::Chorus(Box::new(Chorus::new(rate, crate::ChorusKnobs::default())))
            }
            Some(PreparedBlock::Drive) => Block::Drive(Box::new(
                [(); 2].map(|()| Drive::new(rate, crate::DriveKnobs::default())),
            )),
        });
        let mut chain = Chain {
            rate,
            blocks,
            settings: [BlockSettings::default(); BLOCKS],
            quiet: usize::MAX,
        };
        chain.set_settings(settings);
        chain
    }
}

#[derive(Debug)]
struct ImpulseBlock {
    /// Left and right (for a cabinet or body the right only once the signal is stereo;
    /// for a room the room's two sides).
    convolvers: [Convolver; 2],
    /// The dry signal delayed by the convolvers' latency, so dry and wet line up.
    dry: [Vec<f32>; 2],
    at: usize,
    /// Low cut and high cut, left and right.
    cuts: [[Biquad; 2]; 2],
}

#[derive(Debug)]
enum Block {
    Empty,
    Impulse(Box<ImpulseBlock>),
    Room(Box<ImpulseBlock>),
    Reverb(Box<ReverbBlock>),
    /// Left and right (the right only once the signal is stereo).
    Amp(Box<[Amp; 2]>),
    /// Left and right, as an amp's.
    Drive(Box<[Drive; 2]>),
    /// Both sides at once (linked).
    Compressor(Box<Compressor>),
    /// Left and right, as an amp's.
    Eq(Box<[Eq; 2]>),
    /// Both sides at once.
    Chorus(Box<Chorus>),
}

/// The computed reverb and the cut filters on its sound, left and right.
#[derive(Debug)]
struct ReverbBlock {
    reverb: Reverb,
    cuts: [[Biquad; 2]; 2],
}

/// One track's chain.
#[derive(Debug)]
pub struct Chain {
    rate: f32,
    blocks: [Block; BLOCKS],
    settings: [BlockSettings; BLOCKS],
    /// Samples of silent output in a row, to know when the tail has died away.
    quiet: usize,
}

impl Chain {
    /// Takes new knob values at once (no allocation).
    pub fn set_settings(&mut self, settings: &[BlockSettings; BLOCKS]) {
        for ((block, slot), new) in self
            .blocks
            .iter_mut()
            .zip(self.settings.iter_mut())
            .zip(settings)
        {
            let new = new.sanitized();
            let (filters, cuts) = match block {
                Block::Impulse(impulse) => (
                    Some(&mut impulse.cuts),
                    (new.impulse.low_cut, new.impulse.high_cut),
                ),
                Block::Room(room) => (Some(&mut room.cuts), (new.room.low_cut, new.room.high_cut)),
                Block::Reverb(reverb) => {
                    reverb.reverb.set_knobs(&new.reverb);
                    (
                        Some(&mut reverb.cuts),
                        (new.reverb.low_cut, new.reverb.high_cut),
                    )
                }
                Block::Amp(sides) => {
                    for side in sides.iter_mut() {
                        side.set_knobs(new.amp);
                    }
                    (None, (0.0, 0.0))
                }
                Block::Drive(sides) => {
                    for side in sides.iter_mut() {
                        side.set_knobs(new.drive);
                    }
                    (None, (0.0, 0.0))
                }
                Block::Compressor(compressor) => {
                    compressor.set_knobs(new.compressor);
                    (None, (0.0, 0.0))
                }
                Block::Eq(sides) => {
                    for side in sides.iter_mut() {
                        side.set_knobs(new.eq);
                    }
                    (None, (0.0, 0.0))
                }
                Block::Chorus(chorus) => {
                    chorus.set_knobs(new.chorus);
                    (None, (0.0, 0.0))
                }
                Block::Empty => (None, (0.0, 0.0)),
            };
            if let Some(filters) = filters {
                let low = Biquad::highpass(self.rate, cuts.0, CUT_Q);
                let high = Biquad::lowpass(self.rate, cuts.1, CUT_Q);
                for [low_cut, high_cut] in filters {
                    *low_cut = low_cut.retuned(low);
                    *high_cut = high_cut.retuned(high);
                }
            }
            *slot = new;
        }
    }

    /// The deepest gain reduction of its compressors since the last call, in dB (0 or
    /// below): what a compressor's meter shows.
    pub fn take_gain_reduction(&mut self) -> f32 {
        self.blocks
            .iter_mut()
            .filter_map(|block| match block {
                Block::Compressor(compressor) => Some(compressor.take_deepest()),
                _ => None,
            })
            .fold(0.0, f32::min)
    }

    /// Whether a tail (a reverb, a long response) may still sound after the strings fell
    /// silent: then the chain must still be run, with silence going in.
    pub fn ringing(&self) -> bool {
        (self.quiet as f32) < QUIET_HOLD * self.rate
    }

    /// Silence in every block.
    pub fn reset(&mut self) {
        for block in &mut self.blocks {
            match block {
                Block::Empty => {}
                Block::Impulse(impulse) | Block::Room(impulse) => {
                    for convolver in &mut impulse.convolvers {
                        convolver.reset();
                    }
                    for dry in &mut impulse.dry {
                        dry.fill(0.0);
                    }
                    for filter in impulse.cuts.iter_mut().flatten() {
                        filter.reset();
                    }
                }
                Block::Amp(sides) => {
                    for side in sides.iter_mut() {
                        side.reset();
                    }
                }
                Block::Drive(sides) => {
                    for side in sides.iter_mut() {
                        side.reset();
                    }
                }
                Block::Compressor(compressor) => compressor.reset(),
                Block::Eq(sides) => {
                    for side in sides.iter_mut() {
                        side.reset();
                    }
                }
                Block::Chorus(chorus) => chorus.reset(),
                Block::Reverb(reverb) => {
                    reverb.reverb.clear();
                    for filter in reverb.cuts.iter_mut().flatten() {
                        filter.reset();
                    }
                }
            }
        }
        self.quiet = usize::MAX;
    }

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
        let mut peak = 0.0_f32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mut signal = if stereo_in { [*l, *r] } else { [*l, *l] };
            let mut stereo = stereo_in;
            for (block, settings) in self.blocks.iter_mut().zip(&self.settings) {
                if settings.bypass {
                    continue;
                }
                match block {
                    Block::Empty => {}
                    Block::Amp(sides) => {
                        let [left, right] = &mut **sides;
                        signal[0] = left.process(signal[0]);
                        signal[1] = if stereo {
                            right.process(signal[1])
                        } else {
                            signal[0]
                        };
                    }
                    Block::Compressor(compressor) => {
                        let (left, right) = compressor.process(signal[0], signal[1], stereo);
                        signal = [left, right];
                    }
                    Block::Eq(sides) => {
                        let [left, right] = &mut **sides;
                        signal[0] = left.process(signal[0]);
                        signal[1] = if stereo {
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
                        signal = [left[0], right[0]];
                        stereo = true;
                    }
                    Block::Drive(sides) => {
                        let [left, right] = &mut **sides;
                        signal[0] = left.process(signal[0]);
                        signal[1] = if stereo {
                            right.process(signal[1])
                        } else {
                            signal[0]
                        };
                    }
                    Block::Impulse(impulse) => {
                        let knobs = settings.impulse;
                        let gain = 10.0_f32.powf(knobs.gain_db / 20.0);
                        let at = impulse.at;
                        for side in 0..if stereo { 2 } else { 1 } {
                            let (
                                Some(convolver),
                                Some(dry),
                                Some([low_cut, high_cut]),
                                Some(value),
                            ) = (
                                impulse.convolvers.get_mut(side),
                                impulse.dry.get_mut(side),
                                impulse.cuts.get_mut(side),
                                signal.get_mut(side),
                            )
                            else {
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
                        if !stereo {
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
                            *out =
                                high_cut.process(low_cut.process(convolver.process(input))) * gain;
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
                        stereo = true;
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
                        signal = [
                            signal[0] * (1.0 - knobs.mix) + wl * knobs.mix,
                            signal[1] * (1.0 - knobs.mix) + wr * knobs.mix,
                        ];
                        stereo = true;
                    }
                }
            }
            let [out_l, out_r] = if signal.iter().all(|v| v.is_finite()) {
                signal
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
