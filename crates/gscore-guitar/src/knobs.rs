//! The knobs of the chain's blocks: plain values the audio thread takes at once, and
//! their ranges.

use crate::amp::AmpKnobs;
use crate::compressor::CompressorKnobs;
use crate::drive::DriveKnobs;

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
    /// Room size, 0 small to 1 a cathedral.
    pub size: f32,
    /// The decay time: seconds for the tail to fall by 60 dB.
    pub decay: f32,
    /// The wait before the reverb starts, in ms.
    pub pre_delay: f32,
    /// How much sooner the highs die than the lows, 0 bright to 1 dull.
    pub damping: f32,
    /// How much the tail's delays drift, 0 still to 1 a lush shimmer.
    pub modulation: f32,
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
            size: 0.6,
            decay: 3.0,
            pre_delay: 20.0,
            damping: 0.4,
            modulation: 0.5,
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
    pub amp: AmpKnobs,
    pub drive: DriveKnobs,
    pub compressor: CompressorKnobs,
    pub eq: crate::EqKnobs,
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
    /// A reverb's decay time, seconds.
    pub const DECAY: (f32, f32) = (0.3, 30.0);
    /// A reverb's pre-delay, ms.
    pub const PRE_DELAY: (f32, f32) = (0.0, 250.0);
    pub const MODULATION: (f32, f32) = (0.0, 1.0);
    pub const WIDTH: (f32, f32) = (0.0, 1.0);
    /// An amp's Gain, Bass, Mid, Treble and Speaker; a drive's Drive and Tone.
    pub const AMP: (f32, f32) = (0.0, 1.0);
    /// A compressor's input and output gain, dB.
    pub const COMP_GAIN: (f32, f32) = (-24.0, 24.0);
    /// A compressor's attack, ms.
    pub const ATTACK: (f32, f32) = (0.02, 0.8);
    /// A compressor's release, ms.
    pub const RELEASE: (f32, f32) = (50.0, 1_100.0);
    /// An amp's or a drive's output level, dB.
    pub const LEVEL: (f32, f32) = (-24.0, 12.0);
    /// An EQ band's gain, dB.
    pub const EQ_GAIN: (f32, f32) = (-18.0, 18.0);
    /// An EQ band's frequency, Hz.
    pub const BAND_FREQ: (f32, f32) = (20.0, 20_000.0);
    /// A bell's width.
    pub const BAND_Q: (f32, f32) = (0.3, 6.0);
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
        let a = AmpKnobs::default();
        let d = DriveKnobs::default();
        let c = CompressorKnobs::default();
        let amp = |value: f32, fallback: f32| fit(value, knob_ranges::AMP, fallback);
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
                decay: fit(self.reverb.decay, knob_ranges::DECAY, r.decay),
                pre_delay: fit(self.reverb.pre_delay, knob_ranges::PRE_DELAY, r.pre_delay),
                damping: fit(self.reverb.damping, knob_ranges::DAMPING, r.damping),
                modulation: fit(
                    self.reverb.modulation,
                    knob_ranges::MODULATION,
                    r.modulation,
                ),
                width: fit(self.reverb.width, knob_ranges::WIDTH, r.width),
                mix: fit(self.reverb.mix, knob_ranges::MIX, r.mix),
                low_cut: fit(self.reverb.low_cut, knob_ranges::LOW_CUT, r.low_cut),
                high_cut: fit(self.reverb.high_cut, knob_ranges::HIGH_CUT, r.high_cut),
            },
            amp: AmpKnobs {
                voice: self.amp.voice,
                gain: amp(self.amp.gain, a.gain),
                bass: amp(self.amp.bass, a.bass),
                mid: amp(self.amp.mid, a.mid),
                treble: amp(self.amp.treble, a.treble),
                level_db: fit(self.amp.level_db, knob_ranges::LEVEL, a.level_db),
                speaker: amp(self.amp.speaker, a.speaker),
            },
            drive: DriveKnobs {
                drive: amp(self.drive.drive, d.drive),
                tone: amp(self.drive.tone, d.tone),
                level_db: fit(self.drive.level_db, knob_ranges::LEVEL, d.level_db),
            },
            compressor: CompressorKnobs {
                input_db: fit(self.compressor.input_db, knob_ranges::COMP_GAIN, c.input_db),
                output_db: fit(
                    self.compressor.output_db,
                    knob_ranges::COMP_GAIN,
                    c.output_db,
                ),
                attack_ms: fit(self.compressor.attack_ms, knob_ranges::ATTACK, c.attack_ms),
                release_ms: fit(
                    self.compressor.release_ms,
                    knob_ranges::RELEASE,
                    c.release_ms,
                ),
                ratio: self.compressor.ratio,
                mix: fit(self.compressor.mix, knob_ranges::MIX, c.mix),
            },
            eq: self.eq.sanitized(),
        }
    }
}
