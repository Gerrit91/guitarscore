//! The effect chain after the strings: [`BLOCKS`] places in a fixed order, each empty or
//! holding a block (an impulse response for a cabinet or a body, a room's impulse response,
//! an algorithmic reverb; amps come later). The signal is mono from the strings and becomes
//! stereo at the first block that makes it so (a room or the reverb).
//!
//! Two kinds of change: which blocks are where (and which impulse response, or a room's
//! length) needs new buffers, so a [`ChainSpec`] is prepared off the audio thread
//! ([`PreparedChain`]) and swapped in; turning a knob only changes [`BlockSettings`], which
//! are plain values the audio thread takes at once ([`Chain::set_settings`]).

use std::sync::Arc;

use crate::convolver::{Convolver, PARTITION, Spectra};
use crate::filters::Biquad;
use crate::impulse::ImpulseResponse;
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

/// The knobs of an impulse response block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpulseKnobs {
    /// The share of the convolved sound, 0 dry to 1 only the response.
    pub mix: f32,
    /// Gain in dB.
    pub gain_db: f32,
    /// Low cut and high cut, in Hz.
    pub low_cut: f32,
    pub high_cut: f32,
}

impl Default for ImpulseKnobs {
    fn default() -> Self {
        ImpulseKnobs {
            mix: 1.0,
            gain_db: 0.0,
            low_cut: 40.0,
            high_cut: 12_000.0,
        }
    }
}

/// The knobs of a reverb block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverbKnobs {
    /// Room size, 0 small to 1 large.
    pub size: f32,
    /// How quickly the highs die in the room, 0 bright to 1 dull.
    pub damping: f32,
    /// Stereo width, 0 mono to 1 wide.
    pub width: f32,
    /// The share of reverb, 0 dry to 1 only the room.
    pub mix: f32,
    /// Low cut and high cut of the reverb's sound (not the dry guitar), in Hz.
    pub low_cut: f32,
    pub high_cut: f32,
}

impl Default for ReverbKnobs {
    fn default() -> Self {
        ReverbKnobs {
            size: 0.5,
            damping: 0.5,
            width: 1.0,
            mix: 0.2,
            low_cut: 150.0,
            high_cut: 7_000.0,
        }
    }
}

/// The knobs of a room block (a convolution reverb).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomKnobs {
    /// The share of the room, 0 dry to 1 only the room.
    pub mix: f32,
    /// Gain of the room's sound in dB.
    pub gain_db: f32,
    /// Low cut and high cut of the room's sound, in Hz.
    pub low_cut: f32,
    pub high_cut: f32,
    /// Stereo width of the room, 0 mono to 1 as recorded.
    pub width: f32,
}

impl Default for RoomKnobs {
    fn default() -> Self {
        RoomKnobs {
            mix: 0.25,
            gain_db: 0.0,
            low_cut: 120.0,
            high_cut: 9_000.0,
            width: 1.0,
        }
    }
}

/// The settings of one place: whether it is bypassed and the knobs of every block kind
/// (the ones for the kind it holds apply).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockSettings {
    pub bypass: bool,
    pub impulse: ImpulseKnobs,
    pub room: RoomKnobs,
    pub reverb: ReverbKnobs,
}

/// The knob ranges, for user interfaces and for sanitizing: `(lowest, highest)`.
pub mod knob_ranges {
    pub const MIX: (f32, f32) = (0.0, 1.0);
    /// dB.
    pub const GAIN: (f32, f32) = (-24.0, 12.0);
    /// Hz.
    pub const LOW_CUT: (f32, f32) = (20.0, 600.0);
    pub const HIGH_CUT: (f32, f32) = (1_500.0, 20_000.0);
    pub const SIZE: (f32, f32) = (0.0, 1.0);
    pub const DAMPING: (f32, f32) = (0.0, 1.0);
    pub const WIDTH: (f32, f32) = (0.0, 1.0);
}

fn fit(value: f32, (low, high): (f32, f32), fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

impl BlockSettings {
    /// Every value in its range.
    pub fn sanitized(self) -> Self {
        let i = ImpulseKnobs::default();
        let room = RoomKnobs::default();
        let r = ReverbKnobs::default();
        BlockSettings {
            bypass: self.bypass,
            impulse: ImpulseKnobs {
                mix: fit(self.impulse.mix, knob_ranges::MIX, i.mix),
                gain_db: fit(self.impulse.gain_db, knob_ranges::GAIN, i.gain_db),
                low_cut: fit(self.impulse.low_cut, knob_ranges::LOW_CUT, i.low_cut),
                high_cut: fit(self.impulse.high_cut, knob_ranges::HIGH_CUT, i.high_cut),
            },
            room: RoomKnobs {
                mix: fit(self.room.mix, knob_ranges::MIX, room.mix),
                gain_db: fit(self.room.gain_db, knob_ranges::GAIN, room.gain_db),
                low_cut: fit(self.room.low_cut, knob_ranges::LOW_CUT, room.low_cut),
                high_cut: fit(self.room.high_cut, knob_ranges::HIGH_CUT, room.high_cut),
                width: fit(self.room.width, knob_ranges::WIDTH, room.width),
            },
            reverb: ReverbKnobs {
                size: fit(self.reverb.size, knob_ranges::SIZE, r.size),
                damping: fit(self.reverb.damping, knob_ranges::DAMPING, r.damping),
                width: fit(self.reverb.width, knob_ranges::WIDTH, r.width),
                mix: fit(self.reverb.mix, knob_ranges::MIX, r.mix),
                low_cut: fit(self.reverb.low_cut, knob_ranges::LOW_CUT, r.low_cut),
                high_cut: fit(self.reverb.high_cut, knob_ranges::HIGH_CUT, r.high_cut),
            },
        }
    }
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
                Block::Reverb(reverb) => (
                    Some(&mut reverb.cuts),
                    (new.reverb.low_cut, new.reverb.high_cut),
                ),
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
        let mut peak = 0.0_f32;
        for ((&x, l), r) in input.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
            let mut signal = [x, x];
            let mut stereo = false;
            for (block, settings) in self.blocks.iter_mut().zip(&self.settings) {
                if settings.bypass {
                    continue;
                }
                match block {
                    Block::Empty => {}
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
                        let (wet_l, wet_r) =
                            block
                                .reverb
                                .process(signal[0], signal[1], knobs.size, knobs.damping);
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
            self.quiet.saturating_add(input.len())
        };
    }
}
