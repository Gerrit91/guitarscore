//! The sound of a song for a synthesizer that runs somewhere else: the browser's
//! AudioWorklet has its own [`SynthEngine`] and gets the string models, the effect chains,
//! the buses and the drum parts as bytes ([`encode_sound`], [`decode_sound`]). The desktop
//! player hands the same things over in memory.
//!
//! An impulse response travels as its samples: the built-in cabinets and bodies are
//! computed in the app and a third of a second long.

use std::sync::Arc;

use gscore_guitar::{
    AmpKnobs, AmpVoice, BANDS, BLOCKS, BandShape, BlockKind, BlockSettings, ChainSpec, ChorusKnobs,
    CompRatio, CompressorKnobs, DelayKnobs, DelayNote, DriveKnobs, EqKnobs, ImpulseKnobs,
    ImpulseResponse, Pickup, ReverbKnobs, RoomKnobs, StringModel,
};
use gscore_play::StringKind;

#[cfg(test)]
use crate::SoundBank;
use crate::{Bus, ChainKnobs, SynthEngine, prepare_chain};

/// The first bytes of an encoded message, with the format's version.
const MAGIC: &[u8; 4] = b"GSS1";
/// The longest impulse response taken, in samples per channel (far above a real one).
const MAX_RESPONSE: usize = 4_000_000;

/// One change to how a synthesizer sounds.
#[derive(Debug, Clone)]
pub enum SoundMessage {
    /// The modelled strings of a kind.
    StringModel(StringKind, StringModel),
    /// A track's own string model (`None`: its kind's again).
    TrackModel(usize, Option<StringModel>),
    /// A track's effect chain, to be prepared.
    Chain(usize, ChainSpec, ChainKnobs),
    /// New knob values for a track's chain.
    ChainSettings(usize, ChainKnobs),
    /// The chain of a part of a drum track, to be prepared.
    PartChain(usize, usize, ChainSpec, ChainKnobs),
    /// New knob values for a drum part's chain.
    PartChainSettings(usize, usize, ChainKnobs),
    /// A drum part's level (0-127), pan (0-127), mute and solo.
    PartLevel(usize, usize, u8),
    PartPan(usize, usize, u8),
    PartMute(usize, usize, bool),
    PartSolo(usize, usize, bool),
    /// The master's chain, to be prepared.
    MasterChain(ChainSpec, ChainKnobs),
    /// New knob values for the master's chain.
    MasterChainSettings(ChainKnobs),
    /// A bus's chain, to be prepared, with its knobs, level (0-127) and mute.
    Bus(Bus, ChainSpec, ChainKnobs, u8, bool),
    /// New knob values, level and mute for a bus.
    BusSettings(Bus, ChainKnobs, u8, bool),
    /// A track's send into the compressor bus (0-127).
    CompSend(usize, u8),
}

impl SynthEngine {
    /// Carries out `message` now, preparing a chain where it brings one. For a
    /// synthesizer without a thread of its own to prepare chains on (the browser's).
    pub fn apply_sound(&mut self, message: SoundMessage) {
        let rate = self.sample_rate();
        match message {
            SoundMessage::StringModel(kind, model) => self.set_string_model(kind, model),
            SoundMessage::TrackModel(track, model) => self.set_track_model(track, model),
            SoundMessage::Chain(track, spec, knobs) => {
                let chain = prepare_chain(&spec, &knobs, rate);
                drop(self.set_chain(track, chain, knobs));
            }
            SoundMessage::ChainSettings(track, knobs) => self.set_chain_settings(track, knobs),
            SoundMessage::PartChain(track, part, spec, knobs) => {
                let chain = prepare_chain(&spec, &knobs, rate);
                drop(self.set_part_chain(track, part, chain, knobs));
            }
            SoundMessage::PartChainSettings(track, part, knobs) => {
                self.set_part_chain_settings(track, part, knobs);
            }
            SoundMessage::PartLevel(track, part, level) => self.set_part_level(track, part, level),
            SoundMessage::PartPan(track, part, pan) => self.set_part_pan(track, part, pan),
            SoundMessage::PartMute(track, part, muted) => self.set_part_mute(track, part, muted),
            SoundMessage::PartSolo(track, part, soloed) => self.set_part_solo(track, part, soloed),
            SoundMessage::MasterChain(spec, knobs) => {
                let chain = prepare_chain(&spec, &knobs, rate);
                drop(self.set_master_chain(chain, knobs));
            }
            SoundMessage::MasterChainSettings(knobs) => self.set_master_chain_settings(knobs),
            SoundMessage::Bus(bus, spec, knobs, level, muted) => {
                let chain = prepare_chain(&spec, &knobs, rate);
                drop(self.set_bus(bus, chain));
                self.set_bus_settings(bus, knobs, level, muted);
            }
            SoundMessage::BusSettings(bus, knobs, level, muted) => {
                self.set_bus_settings(bus, knobs, level, muted);
            }
            SoundMessage::CompSend(track, value) => self.set_comp_send(track, value),
        }
    }
}

/// `message` as bytes for [`decode_sound`].
pub fn encode_sound(message: &SoundMessage) -> Vec<u8> {
    let mut out = Writer(MAGIC.to_vec());
    match message {
        SoundMessage::StringModel(kind, model) => {
            out.u8(0);
            out.u8(kind_code(*kind));
            out.model(model);
        }
        SoundMessage::TrackModel(track, model) => {
            out.u8(1);
            out.index(*track);
            out.bool(model.is_some());
            if let Some(model) = model {
                out.model(model);
            }
        }
        SoundMessage::Chain(track, spec, knobs) => {
            out.u8(2);
            out.index(*track);
            out.spec(spec);
            out.knobs(knobs);
        }
        SoundMessage::ChainSettings(track, knobs) => {
            out.u8(3);
            out.index(*track);
            out.knobs(knobs);
        }
        SoundMessage::PartChain(track, part, spec, knobs) => {
            out.u8(4);
            out.index(*track);
            out.index(*part);
            out.spec(spec);
            out.knobs(knobs);
        }
        SoundMessage::PartChainSettings(track, part, knobs) => {
            out.u8(5);
            out.index(*track);
            out.index(*part);
            out.knobs(knobs);
        }
        SoundMessage::PartLevel(track, part, level) => {
            out.u8(6);
            out.index(*track);
            out.index(*part);
            out.u8(*level);
        }
        SoundMessage::PartPan(track, part, pan) => {
            out.u8(7);
            out.index(*track);
            out.index(*part);
            out.u8(*pan);
        }
        SoundMessage::PartMute(track, part, muted) => {
            out.u8(8);
            out.index(*track);
            out.index(*part);
            out.bool(*muted);
        }
        SoundMessage::PartSolo(track, part, soloed) => {
            out.u8(9);
            out.index(*track);
            out.index(*part);
            out.bool(*soloed);
        }
        SoundMessage::MasterChain(spec, knobs) => {
            out.u8(10);
            out.spec(spec);
            out.knobs(knobs);
        }
        SoundMessage::MasterChainSettings(knobs) => {
            out.u8(11);
            out.knobs(knobs);
        }
        SoundMessage::Bus(bus, spec, knobs, level, muted) => {
            out.u8(12);
            out.u8(bus_code(*bus));
            out.spec(spec);
            out.knobs(knobs);
            out.u8(*level);
            out.bool(*muted);
        }
        SoundMessage::BusSettings(bus, knobs, level, muted) => {
            out.u8(13);
            out.u8(bus_code(*bus));
            out.knobs(knobs);
            out.u8(*level);
            out.bool(*muted);
        }
        SoundMessage::CompSend(track, value) => {
            out.u8(14);
            out.index(*track);
            out.u8(*value);
        }
    }
    out.0
}

/// The message [`encode_sound`] made of `bytes`, or what is wrong with them.
pub fn decode_sound(bytes: &[u8]) -> Result<SoundMessage, String> {
    let mut input = Reader { bytes, at: 0 };
    if input.take(MAGIC.len())? != MAGIC {
        return Err("not a sound message of this version".to_owned());
    }
    let message = match input.u8()? {
        0 => SoundMessage::StringModel(kind_of(input.u8()?)?, input.model()?),
        1 => {
            let track = input.index()?;
            let model = if input.bool()? {
                Some(input.model()?)
            } else {
                None
            };
            SoundMessage::TrackModel(track, model)
        }
        2 => SoundMessage::Chain(input.index()?, input.spec()?, input.knobs()?),
        3 => SoundMessage::ChainSettings(input.index()?, input.knobs()?),
        4 => SoundMessage::PartChain(
            input.index()?,
            input.index()?,
            input.spec()?,
            input.knobs()?,
        ),
        5 => SoundMessage::PartChainSettings(input.index()?, input.index()?, input.knobs()?),
        6 => SoundMessage::PartLevel(input.index()?, input.index()?, input.u8()?),
        7 => SoundMessage::PartPan(input.index()?, input.index()?, input.u8()?),
        8 => SoundMessage::PartMute(input.index()?, input.index()?, input.bool()?),
        9 => SoundMessage::PartSolo(input.index()?, input.index()?, input.bool()?),
        10 => SoundMessage::MasterChain(input.spec()?, input.knobs()?),
        11 => SoundMessage::MasterChainSettings(input.knobs()?),
        12 => SoundMessage::Bus(
            bus_of(input.u8()?)?,
            input.spec()?,
            input.knobs()?,
            input.u8()?,
            input.bool()?,
        ),
        13 => SoundMessage::BusSettings(
            bus_of(input.u8()?)?,
            input.knobs()?,
            input.u8()?,
            input.bool()?,
        ),
        14 => SoundMessage::CompSend(input.index()?, input.u8()?),
        other => return Err(format!("unknown sound message {other}")),
    };
    if input.at != bytes.len() {
        return Err("bytes left over after the sound message".to_owned());
    }
    Ok(message)
}

fn kind_code(kind: StringKind) -> u8 {
    match kind {
        StringKind::Nylon => 0,
        StringKind::Steel => 1,
        StringKind::Electric => 2,
        StringKind::Bass => 3,
    }
}

fn kind_of(code: u8) -> Result<StringKind, String> {
    Ok(match code {
        0 => StringKind::Nylon,
        1 => StringKind::Steel,
        2 => StringKind::Electric,
        3 => StringKind::Bass,
        other => return Err(format!("unknown kind of string {other}")),
    })
}

fn bus_code(bus: Bus) -> u8 {
    match bus {
        Bus::Reverb => 0,
        Bus::Compressor => 1,
        Bus::Chorus => 2,
    }
}

fn bus_of(code: u8) -> Result<Bus, String> {
    Ok(match code {
        0 => Bus::Reverb,
        1 => Bus::Compressor,
        2 => Bus::Chorus,
        other => return Err(format!("unknown bus {other}")),
    })
}

/// The choices of an enum, in the order their codes count.
const PICKUPS: [Pickup; 3] = [Pickup::None, Pickup::SingleCoil, Pickup::Humbucker];
const VOICES: [AmpVoice; 3] = [AmpVoice::Clean, AmpVoice::Crunch, AmpVoice::Lead];
const RATIOS: [CompRatio; 5] = [
    CompRatio::Four,
    CompRatio::Eight,
    CompRatio::Twelve,
    CompRatio::Twenty,
    CompRatio::All,
];
const SHAPES: [BandShape; 3] = [BandShape::LowShelf, BandShape::Bell, BandShape::HighShelf];
const NOTES: [DelayNote; 10] = [
    DelayNote::Half,
    DelayNote::DottedQuarter,
    DelayNote::Quarter,
    DelayNote::QuarterTriplet,
    DelayNote::DottedEighth,
    DelayNote::Eighth,
    DelayNote::EighthTriplet,
    DelayNote::DottedSixteenth,
    DelayNote::Sixteenth,
    DelayNote::SixteenthTriplet,
];

/// The code of `value` among `choices` (0 for one that is not listed: a new choice must
/// be added to its list above, at the end).
fn code_of<T: PartialEq>(choices: &[T], value: &T) -> u8 {
    choices
        .iter()
        .position(|choice| choice == value)
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0)
}

fn choice_of<T: Copy>(choices: &[T], code: u8, what: &str) -> Result<T, String> {
    choices
        .get(usize::from(code))
        .copied()
        .ok_or_else(|| format!("unknown {what} {code}"))
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    /// A track, part or stage count: small, sent as 32 bits.
    fn index(&mut self, value: usize) {
        self.u32(u32::try_from(value).unwrap_or(u32::MAX));
    }

    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn model(&mut self, model: &StringModel) {
        // Destructured in full, so a new knob cannot be forgotten here.
        let StringModel {
            sustain,
            sustain_exponent,
            brilliance,
            stiffness,
            stiffness_stages,
            pick_position,
            hardness,
            softness,
            tone,
            resonance,
            level,
            bridge,
            low_cut,
            second,
            detune,
            pickup,
            pickup_position,
            palm_mute,
            pick_noise,
            attack_pitch,
        } = *model;
        for value in [sustain, sustain_exponent, brilliance, stiffness] {
            self.f32(value);
        }
        self.index(stiffness_stages);
        for value in [
            pick_position,
            hardness,
            softness,
            tone,
            resonance,
            level,
            bridge,
            low_cut,
            second,
            detune,
        ] {
            self.f32(value);
        }
        self.u8(code_of(&PICKUPS, &pickup));
        for value in [pickup_position, palm_mute, pick_noise, attack_pitch] {
            self.f32(value);
        }
    }

    fn response(&mut self, response: &ImpulseResponse) {
        self.u32(response.sample_rate());
        let channels = response.channel_count().min(2);
        self.u8(u8::try_from(channels).unwrap_or(0));
        for index in 0..channels {
            let samples = response.channel(index).unwrap_or_default();
            self.index(samples.len());
            for &sample in samples {
                self.f32(sample);
            }
        }
    }

    fn spec(&mut self, spec: &ChainSpec) {
        for block in &spec.blocks {
            match block {
                None => self.u8(0),
                Some(BlockKind::Impulse(response)) => {
                    self.u8(1);
                    self.response(response);
                }
                Some(BlockKind::Room { response, length }) => {
                    self.u8(2);
                    self.f32(*length);
                    self.response(response);
                }
                Some(BlockKind::Reverb) => self.u8(3),
                Some(BlockKind::Amp) => self.u8(4),
                Some(BlockKind::Drive) => self.u8(5),
                Some(BlockKind::Compressor) => self.u8(6),
                Some(BlockKind::Eq) => self.u8(7),
                Some(BlockKind::Chorus) => self.u8(8),
                Some(BlockKind::Delay) => self.u8(9),
            }
        }
    }

    fn knobs(&mut self, knobs: &ChainKnobs) {
        for settings in &knobs.settings {
            self.settings(settings);
        }
        self.f32(knobs.output_db);
    }

    fn settings(&mut self, settings: &BlockSettings) {
        let BlockSettings {
            bypass,
            impulse,
            room,
            reverb,
            amp,
            drive,
            compressor,
            eq,
            chorus,
            delay,
        } = *settings;
        self.bool(bypass);
        let ImpulseKnobs {
            mix,
            gain_db,
            low_cut,
            high_cut,
        } = impulse;
        for value in [mix, gain_db, low_cut, high_cut] {
            self.f32(value);
        }
        let RoomKnobs {
            mix,
            gain_db,
            low_cut,
            high_cut,
            width,
        } = room;
        for value in [mix, gain_db, low_cut, high_cut, width] {
            self.f32(value);
        }
        let ReverbKnobs {
            size,
            decay,
            pre_delay,
            damping,
            modulation,
            width,
            mix,
            low_cut,
            high_cut,
        } = reverb;
        for value in [
            size, decay, pre_delay, damping, modulation, width, mix, low_cut, high_cut,
        ] {
            self.f32(value);
        }
        let AmpKnobs {
            voice,
            gain,
            bass,
            mid,
            treble,
            level_db,
        } = amp;
        self.u8(code_of(&VOICES, &voice));
        for value in [gain, bass, mid, treble, level_db] {
            self.f32(value);
        }
        let DriveKnobs {
            drive,
            tone,
            level_db,
            tight,
        } = drive;
        for value in [drive, tone, level_db, tight] {
            self.f32(value);
        }
        let CompressorKnobs {
            input_db,
            output_db,
            attack_ms,
            release_ms,
            ratio,
            mix,
        } = compressor;
        for value in [input_db, output_db, attack_ms, release_ms, mix] {
            self.f32(value);
        }
        self.u8(code_of(&RATIOS, &ratio));
        let EqKnobs {
            low_cut,
            high_cut,
            band_freq,
            band_gain,
            band_q,
            band_shape,
        } = eq;
        self.f32(low_cut);
        self.f32(high_cut);
        for band in [band_freq, band_gain, band_q] {
            for value in band {
                self.f32(value);
            }
        }
        for shape in band_shape {
            self.u8(code_of(&SHAPES, &shape));
        }
        let ChorusKnobs {
            rate_hz,
            depth_ms,
            mix,
        } = chorus;
        for value in [rate_hz, depth_ms, mix] {
            self.f32(value);
        }
        let DelayKnobs {
            sync,
            note,
            time_ms,
            feedback,
            level,
            low_cut,
            high_cut,
            ping_pong,
        } = delay;
        self.bool(sync);
        self.u8(code_of(&NOTES, &note));
        for value in [time_ms, feedback, level, low_cut, high_cut, ping_pong] {
            self.f32(value);
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(count)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| "the sound message is cut short".to_owned())?;
        let taken = self.bytes.get(self.at..end).unwrap_or_default();
        self.at = end;
        Ok(taken)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?.first().copied().unwrap_or(0))
    }

    fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let mut four = [0_u8; 4];
        four.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(four))
    }

    fn index(&mut self) -> Result<usize, String> {
        usize::try_from(self.u32()?).map_err(|_| "a number is too large".to_owned())
    }

    fn f32(&mut self) -> Result<f32, String> {
        let mut four = [0_u8; 4];
        four.copy_from_slice(self.take(4)?);
        Ok(f32::from_le_bytes(four))
    }

    fn floats<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut values = [0.0_f32; N];
        for value in &mut values {
            *value = self.f32()?;
        }
        Ok(values)
    }

    fn model(&mut self) -> Result<StringModel, String> {
        let [sustain, sustain_exponent, brilliance, stiffness] = self.floats()?;
        let stiffness_stages = self.index()?;
        let [
            pick_position,
            hardness,
            softness,
            tone,
            resonance,
            level,
            bridge,
            low_cut,
            second,
            detune,
        ] = self.floats()?;
        let pickup = choice_of(&PICKUPS, self.u8()?, "pickup")?;
        let [pickup_position, palm_mute, pick_noise, attack_pitch] = self.floats()?;
        Ok(StringModel {
            sustain,
            sustain_exponent,
            brilliance,
            stiffness,
            stiffness_stages,
            pick_position,
            hardness,
            softness,
            tone,
            resonance,
            level,
            bridge,
            low_cut,
            second,
            detune,
            pickup,
            pickup_position,
            palm_mute,
            pick_noise,
            attack_pitch,
        })
    }

    fn response(&mut self) -> Result<Arc<ImpulseResponse>, String> {
        let rate = self.u32()?;
        let count = usize::from(self.u8()?);
        let mut channels = Vec::with_capacity(count.min(2));
        for _ in 0..count {
            let length = self.index()?;
            if length > MAX_RESPONSE {
                return Err("an impulse response is too long".to_owned());
            }
            let raw = self.take(length.saturating_mul(4))?;
            channels.push(
                raw.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|four| f32::from_le_bytes(*four))
                    .collect(),
            );
        }
        ImpulseResponse::from_channels(channels, rate)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    }

    fn spec(&mut self) -> Result<ChainSpec, String> {
        let mut spec = ChainSpec::default();
        for place in &mut spec.blocks {
            *place = match self.u8()? {
                0 => None,
                1 => Some(BlockKind::Impulse(self.response()?)),
                2 => {
                    let length = self.f32()?;
                    Some(BlockKind::room(self.response()?, length))
                }
                3 => Some(BlockKind::Reverb),
                4 => Some(BlockKind::Amp),
                5 => Some(BlockKind::Drive),
                6 => Some(BlockKind::Compressor),
                7 => Some(BlockKind::Eq),
                8 => Some(BlockKind::Chorus),
                9 => Some(BlockKind::Delay),
                other => return Err(format!("unknown block {other}")),
            };
        }
        Ok(spec)
    }

    fn knobs(&mut self) -> Result<ChainKnobs, String> {
        let mut settings = [BlockSettings::default(); BLOCKS];
        for place in &mut settings {
            *place = self.settings()?;
        }
        Ok(ChainKnobs {
            settings,
            output_db: self.f32()?,
        })
    }

    fn settings(&mut self) -> Result<BlockSettings, String> {
        let bypass = self.bool()?;
        let [mix, gain_db, low_cut, high_cut] = self.floats()?;
        let impulse = ImpulseKnobs {
            mix,
            gain_db,
            low_cut,
            high_cut,
        };
        let [mix, gain_db, low_cut, high_cut, width] = self.floats()?;
        let room = RoomKnobs {
            mix,
            gain_db,
            low_cut,
            high_cut,
            width,
        };
        let [
            size,
            decay,
            pre_delay,
            damping,
            modulation,
            width,
            mix,
            low_cut,
            high_cut,
        ] = self.floats()?;
        let reverb = ReverbKnobs {
            size,
            decay,
            pre_delay,
            damping,
            modulation,
            width,
            mix,
            low_cut,
            high_cut,
        };
        let voice = choice_of(&VOICES, self.u8()?, "amp voice")?;
        let [gain, bass, mid, treble, level_db] = self.floats()?;
        let amp = AmpKnobs {
            voice,
            gain,
            bass,
            mid,
            treble,
            level_db,
        };
        let [drive, tone, level_db, tight] = self.floats()?;
        let drive = DriveKnobs {
            drive,
            tone,
            level_db,
            tight,
        };
        let [input_db, output_db, attack_ms, release_ms, mix] = self.floats()?;
        let compressor = CompressorKnobs {
            input_db,
            output_db,
            attack_ms,
            release_ms,
            ratio: choice_of(&RATIOS, self.u8()?, "compressor ratio")?,
            mix,
        };
        let [low_cut, high_cut] = self.floats()?;
        let band_freq: [f32; BANDS] = self.floats()?;
        let band_gain: [f32; BANDS] = self.floats()?;
        let band_q: [f32; BANDS] = self.floats()?;
        let mut band_shape = [BandShape::Bell; BANDS];
        for shape in &mut band_shape {
            *shape = choice_of(&SHAPES, self.u8()?, "band shape")?;
        }
        let eq = EqKnobs {
            low_cut,
            high_cut,
            band_freq,
            band_gain,
            band_q,
            band_shape,
        };
        let [rate_hz, depth_ms, mix] = self.floats()?;
        let chorus = ChorusKnobs {
            rate_hz,
            depth_ms,
            mix,
        };
        let sync = self.bool()?;
        let note = choice_of(&NOTES, self.u8()?, "delay note")?;
        let [time_ms, feedback, level, low_cut, high_cut, ping_pong] = self.floats()?;
        let delay = DelayKnobs {
            sync,
            note,
            time_ms,
            feedback,
            level,
            low_cut,
            high_cut,
            ping_pong,
        };
        Ok(BlockSettings {
            bypass,
            impulse,
            room,
            reverb,
            amp,
            drive,
            compressor,
            eq,
            chorus,
            delay,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    /// A chain with every kind of block the wire knows, and knobs off their defaults.
    fn chain() -> (ChainSpec, ChainKnobs) {
        let response =
            Arc::new(ImpulseResponse::from_samples(&[1.0, 0.5, -0.25, 0.125], 48_000).unwrap());
        let mut spec = ChainSpec::default();
        spec.blocks[0] = Some(BlockKind::Drive);
        spec.blocks[1] = Some(BlockKind::Amp);
        spec.blocks[2] = Some(BlockKind::Impulse(Arc::clone(&response)));
        spec.blocks[3] = Some(BlockKind::room(response, 1.5));
        spec.blocks[4] = Some(BlockKind::Delay);
        let mut knobs = ChainKnobs {
            output_db: -6.5,
            ..ChainKnobs::default()
        };
        knobs.settings[0].drive.tight = 0.8;
        knobs.settings[1].amp.voice = AmpVoice::Lead;
        knobs.settings[1].amp.gain = 0.77;
        knobs.settings[2].impulse.high_cut = 12_000.0;
        knobs.settings[3].bypass = true;
        knobs.settings[4].delay.note = DelayNote::SixteenthTriplet;
        knobs.settings[4].compressor.ratio = CompRatio::All;
        knobs.settings[4].eq.band_shape[1] = BandShape::HighShelf;
        (spec, knobs)
    }

    fn messages() -> Vec<SoundMessage> {
        let (spec, knobs) = chain();
        let model = StringModel {
            pickup: Pickup::Humbucker,
            stiffness_stages: 3,
            palm_mute: 0.63,
            ..gscore_guitar::GuitarModel::Electric.strings()
        };
        vec![
            SoundMessage::StringModel(StringKind::Bass, model),
            SoundMessage::TrackModel(3, Some(model)),
            SoundMessage::TrackModel(4, None),
            SoundMessage::Chain(2, spec.clone(), knobs),
            SoundMessage::ChainSettings(2, knobs),
            SoundMessage::PartChain(5, 1, spec.clone(), knobs),
            SoundMessage::PartChainSettings(5, 1, knobs),
            SoundMessage::PartLevel(5, 1, 100),
            SoundMessage::PartPan(5, 1, 20),
            SoundMessage::PartMute(5, 1, true),
            SoundMessage::PartSolo(5, 1, true),
            SoundMessage::MasterChain(spec.clone(), knobs),
            SoundMessage::MasterChainSettings(knobs),
            SoundMessage::Bus(Bus::Chorus, spec, knobs, 90, true),
            SoundMessage::BusSettings(Bus::Compressor, knobs, 64, false),
            SoundMessage::CompSend(7, 33),
        ]
    }

    #[test]
    fn every_message_comes_back_as_it_was_sent() {
        for message in messages() {
            let bytes = encode_sound(&message);
            let back = decode_sound(&bytes).unwrap();
            // The debug text names every field, the responses' samples too.
            assert_eq!(format!("{back:?}"), format!("{message:?}"));
            assert_eq!(encode_sound(&back), bytes);
        }
    }

    #[test]
    fn damaged_bytes_are_refused_not_trusted() {
        let bytes = encode_sound(&messages().remove(3));
        assert!(
            decode_sound(&bytes[..bytes.len() - 1]).is_err(),
            "cut short"
        );
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(decode_sound(&longer).is_err(), "bytes left over");
        let mut other = bytes.clone();
        other[3] = b'9';
        assert!(decode_sound(&other).is_err(), "another version");
        let mut unknown = bytes;
        unknown[4] = 200;
        assert!(decode_sound(&unknown).is_err(), "an unknown message");
        assert!(decode_sound(&[]).is_err());
    }

    #[test]
    fn every_choice_keeps_its_code() {
        for (index, pickup) in PICKUPS.iter().enumerate() {
            assert_eq!(usize::from(code_of(&PICKUPS, pickup)), index);
        }
        for note in NOTES {
            assert_eq!(
                choice_of(&NOTES, code_of(&NOTES, &note), "note").unwrap(),
                note
            );
        }
        for ratio in CompRatio::ALL_RATIOS {
            assert_eq!(
                choice_of(&RATIOS, code_of(&RATIOS, &ratio), "ratio").unwrap(),
                ratio
            );
        }
        assert!(choice_of(&VOICES, 3, "amp voice").is_err());
    }

    #[test]
    fn an_engine_takes_a_decoded_chain_and_plays_through_it() {
        let bank = SoundBank::from_bytes(&crate::testing::sine_soundfont("Sine")).unwrap();
        let mut engine = SynthEngine::new(&bank, 48_000).unwrap();
        for message in messages() {
            engine.apply_sound(decode_sound(&encode_sound(&message)).unwrap());
        }
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        engine.render(&mut left, &mut right);
        assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
    }
}
