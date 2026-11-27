//! The effect chain after the strings: [`BLOCKS`] places in a fixed order, each empty or
//! holding a block (an impulse response for a cabinet or a body, a room's impulse response,
//! an algorithmic reverb, a computed amp or overdrive pedal, a delay). The signal is mono from the strings and becomes
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
use crate::echo::Echo;
use crate::eq::Eq;
use crate::external::{External, Instance};
use crate::filters::Biquad;
use crate::impulse::ImpulseResponse;
use crate::knobs::BlockSettings;
use crate::reverb::Reverb;

/// The places for blocks after the strings.
pub const BLOCKS: usize = 5;
/// Below this level the chain's output counts as silent...
pub(crate) const SILENCE: f32 = 1e-6;
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
    /// A digital delay, its time free or synced to the tempo ([`Chain::set_tempo`]).
    Delay,
    /// A block made outside this crate (a plugin): stereo from here on.
    External(External),
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
    Delay,
    External(External),
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
                BlockKind::Delay => PreparedBlock::Delay,
                BlockKind::External(external) => PreparedBlock::External(external),
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
    pub fn instance(&self, settings: &[BlockSettings; BLOCKS], output_db: f32) -> Chain {
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
            Some(PreparedBlock::Delay) => {
                Block::Delay(Box::new(Echo::new(rate, crate::DelayKnobs::default())))
            }
            Some(PreparedBlock::Drive) => Block::Drive(Box::new(
                [(); 2].map(|()| Drive::new(rate, crate::DriveKnobs::default())),
            )),
            // One that cannot be made leaves its place empty: the sound passes.
            Some(PreparedBlock::External(external)) => external
                .0
                .instance(self.sample_rate)
                .map_or(Block::Empty, |instance| Block::External(Instance(instance))),
        });
        let mut chain = Chain {
            rate,
            blocks,
            settings: [BlockSettings::default(); BLOCKS],
            output: 1.0,
            quiet: usize::MAX,
        };
        chain.set_settings(settings);
        chain.set_output_db(output_db);
        chain
    }
}

#[derive(Debug)]
pub(crate) struct ImpulseBlock {
    /// Left and right (for a cabinet or body the right only once the signal is stereo;
    /// for a room the room's two sides).
    pub(crate) convolvers: [Convolver; 2],
    /// The dry signal delayed by the convolvers' latency, so dry and wet line up.
    pub(crate) dry: [Vec<f32>; 2],
    pub(crate) at: usize,
    /// Low cut and high cut, left and right.
    pub(crate) cuts: [[Biquad; 2]; 2],
}

#[derive(Debug)]
pub(crate) enum Block {
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
    /// Both sides at once.
    Delay(Box<Echo>),
    /// A plugin: whole buffers at once, stereo (see `chain_run`).
    External(Instance),
}

/// The computed reverb and the cut filters on its sound, left and right.
#[derive(Debug)]
pub(crate) struct ReverbBlock {
    pub(crate) reverb: Reverb,
    pub(crate) cuts: [[Biquad; 2]; 2],
}

/// One track's chain.
#[derive(Debug)]
pub struct Chain {
    rate: f32,
    pub(crate) blocks: [Block; BLOCKS],
    pub(crate) settings: [BlockSettings; BLOCKS],
    /// The output level of the chain, linear (a scalar after every block, before the
    /// mixer's fader).
    pub(crate) output: f32,
    /// Samples of silent output in a row, to know when the tail has died away.
    pub(crate) quiet: usize,
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
                Block::Delay(echo) => {
                    echo.set_knobs(new.delay);
                    (None, (0.0, 0.0))
                }
                Block::Empty | Block::External(_) => (None, (0.0, 0.0)),
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

    /// Sets the chain's output level in dB, after every block and before the mixer's
    /// fader. Kept in its range; a value that is not a number means unity.
    pub fn set_output_db(&mut self, db: f32) {
        let (low, high) = crate::knob_ranges::OUTPUT;
        let db = if db.is_finite() {
            db.clamp(low, high)
        } else {
            0.0
        };
        self.output = 10.0_f32.powf(db / 20.0);
    }

    /// How many frames late the chain's sound comes out: the latencies of its external
    /// blocks that play (a bypassed one's sound does not pass it, so is not late).
    pub fn latency(&self) -> u32 {
        self.blocks
            .iter()
            .zip(&self.settings)
            .filter_map(|(block, settings)| match block {
                Block::External(instance) if !settings.bypass => Some(instance.0.latency()),
                _ => None,
            })
            .fold(0, u32::saturating_add)
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

    /// Whether a tail (a reverb, a long response, a delay's echoes) may still sound after
    /// the strings fell silent: then the chain must still be run, with silence going in.
    /// A delay's next echo may come after a longer silence than the others' tails have.
    pub fn ringing(&self) -> bool {
        (self.quiet as f32) < QUIET_HOLD * self.rate
            || self.blocks.iter().any(|block| match block {
                Block::Delay(echo) => echo.ringing(),
                _ => false,
            })
    }

    /// The tempo the song plays at now, quarter notes a minute (the playback speed
    /// included): a delay synced to it follows. No allocation.
    pub fn set_tempo(&mut self, bpm: f32) {
        for block in &mut self.blocks {
            if let Block::Delay(echo) = block {
                echo.set_tempo(bpm);
            }
        }
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
                Block::Delay(echo) => echo.clear(),
                Block::Reverb(reverb) => {
                    reverb.reverb.clear();
                    for filter in reverb.cuts.iter_mut().flatten() {
                        filter.reset();
                    }
                }
                Block::External(instance) => instance.0.reset(),
            }
        }
        self.quiet = usize::MAX;
    }
}
