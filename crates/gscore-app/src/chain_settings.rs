//! The effect chains of the modelled strings as the settings file keeps them: per kind of
//! string, five places after the strings, each empty or holding a block with its knobs
//! (and, for an impulse response, the WAV file it was loaded from).

use std::path::PathBuf;

use gscore_guitar::{
    AmpKnobs, AmpVoice, BANDS, BLOCKS, BlockSettings, CompRatio, CompressorKnobs, DriveKnobs,
    EqKnobs, ImpulseKnobs, LENGTH_RANGE, ReverbKnobs, RoomKnobs,
};
use gscore_play::StringKind;
use serde::{Deserialize, Serialize};

/// What a place holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockChoice {
    #[default]
    Empty,
    /// An impulse response of a cabinet or a guitar body.
    Cabinet,
    /// An impulse response of a room (a convolution reverb).
    Room,
    /// The computed reverb.
    Reverb,
    /// The computed guitar amp.
    Amp,
    /// The computed overdrive pedal.
    Drive,
    /// The computed FET compressor (CA-76).
    Compressor,
    /// The 4-band parametric EQ.
    Eq,
    /// The stereo chorus.
    Chorus,
}

impl BlockChoice {
    /// The name the chain window shows.
    pub fn label(self) -> &'static str {
        match self {
            BlockChoice::Empty => "Empty",
            BlockChoice::Cabinet => "Cabinet / body",
            BlockChoice::Room => "Room",
            BlockChoice::Reverb => "Reverb",
            BlockChoice::Amp => "Amp",
            BlockChoice::Drive => "Drive",
            BlockChoice::Compressor => "Compressor",
            BlockChoice::Eq => "EQ",
            BlockChoice::Chorus => "Chorus",
        }
    }

    /// Whether it plays an impulse response from a file.
    pub fn uses_file(self) -> bool {
        matches!(self, BlockChoice::Cabinet | BlockChoice::Room)
    }
}

/// An amp's voice as the settings file keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AmpVoiceSetting {
    Clean,
    #[default]
    Crunch,
    Lead,
}

impl AmpVoiceSetting {
    /// Every voice, in the order the chain window shows them.
    pub const ALL: [AmpVoiceSetting; 3] = [
        AmpVoiceSetting::Clean,
        AmpVoiceSetting::Crunch,
        AmpVoiceSetting::Lead,
    ];

    /// The engine's voice.
    pub fn voice(self) -> AmpVoice {
        match self {
            AmpVoiceSetting::Clean => AmpVoice::Clean,
            AmpVoiceSetting::Crunch => AmpVoice::Crunch,
            AmpVoiceSetting::Lead => AmpVoice::Lead,
        }
    }

    /// Its name.
    pub fn label(self) -> &'static str {
        self.voice().label()
    }
}

/// A compressor's ratio as the settings file keeps it (the CA-76's buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompRatioSetting {
    #[default]
    Four,
    Eight,
    Twelve,
    Twenty,
    /// Every button in: harder than 20, overshooting, more color.
    All,
}

impl CompRatioSetting {
    /// Every ratio, in the order the chain window shows them.
    pub const ALL: [CompRatioSetting; 5] = [
        CompRatioSetting::Four,
        CompRatioSetting::Eight,
        CompRatioSetting::Twelve,
        CompRatioSetting::Twenty,
        CompRatioSetting::All,
    ];

    /// The engine's ratio.
    pub fn ratio(self) -> CompRatio {
        match self {
            CompRatioSetting::Four => CompRatio::Four,
            CompRatioSetting::Eight => CompRatio::Eight,
            CompRatioSetting::Twelve => CompRatio::Twelve,
            CompRatioSetting::Twenty => CompRatio::Twenty,
            CompRatioSetting::All => CompRatio::All,
        }
    }

    /// Its button's label.
    pub fn label(self) -> &'static str {
        self.ratio().label()
    }
}

/// One place of a chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockSetting {
    pub choice: BlockChoice,
    /// The impulse response's WAV file (a cabinet, body or room).
    pub file: Option<PathBuf>,
    pub bypass: bool,
    pub mix: f32,
    pub gain_db: f32,
    pub low_cut: f32,
    pub high_cut: f32,
    pub width: f32,
    pub size: f32,
    /// A reverb's decay time (s), pre-delay (ms) and modulation.
    pub decay: f32,
    pub pre_delay: f32,
    pub modulation: f32,
    pub damping: f32,
    /// A room's decay time relative to the recording's (`gscore_guitar::LENGTH_RANGE`).
    /// It changes the response itself, so the chain is prepared again when it is set,
    /// not on every step of a drag.
    pub length: f32,
    /// An amp's voice and knobs (its output level is `gain_db`).
    pub voice: AmpVoiceSetting,
    pub drive: f32,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
    pub speaker: f32,
    /// A drive pedal's tone (its Drive is `drive`, its level `gain_db`).
    pub tone: f32,
    /// A compressor's input drive and its attack and release times (ms); its output level
    /// is `gain_db`, its dry/compressed blend `mix`.
    pub input_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub comp_ratio: CompRatioSetting,
    /// An EQ's four bands: their frequencies, gains and widths (the shelves' width is
    /// their slope). Its cuts are `low_cut` and `high_cut`.
    pub eq_band_freq: [f32; BANDS],
    pub eq_band_gain: [f32; BANDS],
    pub eq_band_q: [f32; BANDS],
    /// A chorus's speed and swing (its mix is `mix`).
    pub chorus_rate: f32,
    pub chorus_depth: f32,
}

impl Default for BlockSetting {
    fn default() -> Self {
        BlockSetting::of(BlockChoice::Empty)
    }
}

impl BlockSetting {
    /// A fresh block of `choice` with its usual knob values.
    pub fn of(choice: BlockChoice) -> Self {
        let (mix, gain_db, low_cut, high_cut, width) = match choice {
            BlockChoice::Room => {
                let room = RoomKnobs::default();
                (
                    room.mix,
                    room.gain_db,
                    room.low_cut,
                    room.high_cut,
                    room.width,
                )
            }
            BlockChoice::Reverb => {
                let reverb = ReverbKnobs::default();
                (
                    reverb.mix,
                    0.0,
                    reverb.low_cut,
                    reverb.high_cut,
                    reverb.width,
                )
            }
            BlockChoice::Amp => (1.0, AmpKnobs::default().level_db, 0.0, 0.0, 1.0),
            BlockChoice::Drive => (1.0, DriveKnobs::default().level_db, 0.0, 0.0, 1.0),
            BlockChoice::Compressor => {
                let compressor = CompressorKnobs::default();
                (compressor.mix, compressor.output_db, 0.0, 0.0, 1.0)
            }
            BlockChoice::Eq => {
                let eq = EqKnobs::default();
                (1.0, 0.0, eq.low_cut, eq.high_cut, 1.0)
            }
            BlockChoice::Chorus => {
                let chorus = gscore_guitar::ChorusKnobs::default();
                (chorus.mix, 0.0, 0.0, 0.0, 1.0)
            }
            BlockChoice::Cabinet | BlockChoice::Empty => {
                let impulse = ImpulseKnobs::default();
                (
                    impulse.mix,
                    impulse.gain_db,
                    impulse.low_cut,
                    impulse.high_cut,
                    1.0,
                )
            }
        };
        let reverb = ReverbKnobs::default();
        let amp = AmpKnobs::default();
        BlockSetting {
            choice,
            file: None,
            bypass: false,
            mix,
            gain_db,
            low_cut,
            high_cut,
            width,
            size: reverb.size,
            decay: reverb.decay,
            pre_delay: reverb.pre_delay,
            modulation: reverb.modulation,
            damping: reverb.damping,
            length: 1.0,
            voice: AmpVoiceSetting::default(),
            // The amp's Gain, or a drive's Drive.
            drive: if choice == BlockChoice::Drive {
                DriveKnobs::default().drive
            } else {
                amp.gain
            },
            bass: amp.bass,
            middle: amp.mid,
            treble: amp.treble,
            speaker: amp.speaker,
            tone: DriveKnobs::default().tone,
            input_db: CompressorKnobs::default().input_db,
            attack_ms: CompressorKnobs::default().attack_ms,
            release_ms: CompressorKnobs::default().release_ms,
            comp_ratio: CompRatioSetting::default(),
            eq_band_freq: EqKnobs::default().band_freq,
            eq_band_gain: EqKnobs::default().band_gain,
            eq_band_q: EqKnobs::default().band_q,
            chorus_rate: gscore_guitar::ChorusKnobs::default().rate_hz,
            chorus_depth: gscore_guitar::ChorusKnobs::default().depth_ms,
        }
    }

    /// A room's decay time within its range, as the engine takes it: 1 is the recording's
    /// own. A settings file may hold anything, so the value is fitted here.
    pub fn room_length(&self) -> f32 {
        if self.length.is_finite() {
            self.length.clamp(LENGTH_RANGE.0, LENGTH_RANGE.1)
        } else {
            1.0
        }
    }

    /// The engine's knob values for this place.
    pub fn knobs(&self) -> BlockSettings {
        BlockSettings {
            bypass: self.bypass,
            impulse: ImpulseKnobs {
                mix: self.mix,
                gain_db: self.gain_db,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
            },
            room: RoomKnobs {
                mix: self.mix,
                gain_db: self.gain_db,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
                width: self.width,
            },
            reverb: ReverbKnobs {
                size: self.size,
                decay: self.decay,
                pre_delay: self.pre_delay,
                modulation: self.modulation,
                damping: self.damping,
                width: self.width,
                mix: self.mix,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
            },
            amp: AmpKnobs {
                voice: self.voice.voice(),
                gain: self.drive,
                bass: self.bass,
                mid: self.middle,
                treble: self.treble,
                level_db: self.gain_db,
                speaker: self.speaker,
            },
            drive: DriveKnobs {
                drive: self.drive,
                tone: self.tone,
                level_db: self.gain_db,
            },
            compressor: CompressorKnobs {
                input_db: self.input_db,
                output_db: self.gain_db,
                attack_ms: self.attack_ms,
                release_ms: self.release_ms,
                ratio: self.comp_ratio.ratio(),
                mix: self.mix,
            },
            eq: EqKnobs {
                low_cut: self.low_cut,
                high_cut: self.high_cut,
                band_freq: self.eq_band_freq,
                band_gain: self.eq_band_gain,
                band_q: self.eq_band_q,
            },
            chorus: gscore_guitar::ChorusKnobs {
                rate_hz: self.chorus_rate,
                depth_ms: self.chorus_depth,
                mix: self.mix,
            },
        }
        .sanitized()
    }

    /// Whether `other` needs the chain prepared again (another block or file), rather
    /// than only new knob values.
    ///
    /// A room's `length` is left out on purpose: it changes the prepared response, but is
    /// applied when the length knob is let go (`Playback::rebuild_chain`) instead of on
    /// every step of a drag, which would rebuild the room dozens of times.
    pub fn same_structure(&self, other: &BlockSetting) -> bool {
        self.choice == other.choice && (!self.choice.uses_file() || self.file == other.file)
    }
}

/// One kind's chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChainSetting {
    pub blocks: [BlockSetting; BLOCKS],
}

impl ChainSetting {
    /// The engine's knob values of every place.
    pub fn knobs(&self) -> [BlockSettings; BLOCKS] {
        std::array::from_fn(|index| {
            self.blocks
                .get(index)
                .map(BlockSetting::knobs)
                .unwrap_or_default()
        })
    }

    /// Whether `other` needs the chain prepared again.
    pub fn same_structure(&self, other: &ChainSetting) -> bool {
        self.blocks
            .iter()
            .zip(&other.blocks)
            .all(|(a, b)| a.same_structure(b))
    }
}

/// The four kinds' chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StringChains {
    pub nylon: ChainSetting,
    pub steel: ChainSetting,
    pub electric: ChainSetting,
    pub bass: ChainSetting,
}

impl StringChains {
    /// The chain of `kind`.
    pub fn get(&self, kind: StringKind) -> &ChainSetting {
        match kind {
            StringKind::Nylon => &self.nylon,
            StringKind::Steel => &self.steel,
            StringKind::Electric => &self.electric,
            StringKind::Bass => &self.bass,
        }
    }

    /// Sets the chain of `kind`.
    pub fn set(&mut self, kind: StringKind, chain: ChainSetting) {
        match kind {
            StringKind::Nylon => self.nylon = chain,
            StringKind::Steel => self.steel = chain,
            StringKind::Electric => self.electric = chain,
            StringKind::Bass => self.bass = chain,
        }
    }
}

/// A track's own tone: the sound of its strings and its chain, dialed in on one kind of
/// string. On another kind (the track switched to other strings) that kind's default
/// tone plays instead, until one is dialed in there.
#[derive(Debug, Clone, PartialEq)]
pub struct Tone {
    pub kind: StringKind,
    pub sound: crate::settings::StringSound,
    pub chain: ChainSetting,
}

/// Every track's own tone, by track index (tracks without one play their kind's default).
pub type TrackTones = std::collections::BTreeMap<usize, Tone>;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn a_knob_change_keeps_the_structure_and_a_new_file_does_not() {
        let mut chain = ChainSetting::default();
        chain.blocks[0] = BlockSetting::of(BlockChoice::Cabinet);
        chain.blocks[0].file = Some(PathBuf::from("body.wav"));
        let mut knob = chain.clone();
        knob.blocks[0].mix = 0.3;
        assert!(chain.same_structure(&knob));
        // A room's length rebuilds the response, but is applied when the knob is let go,
        // so it does not count as a structural change either.
        let mut room = chain.clone();
        room.blocks[0] = BlockSetting::of(BlockChoice::Room);
        room.blocks[0].file = Some(PathBuf::from("hall.wav"));
        let mut longer = room.clone();
        longer.blocks[0].length = 2.5;
        assert!(room.same_structure(&longer));
        let mut file = chain.clone();
        file.blocks[0].file = Some(PathBuf::from("other.wav"));
        assert!(!chain.same_structure(&file));
        let mut moved = chain.clone();
        moved.blocks[1] = BlockSetting::of(BlockChoice::Reverb);
        assert!(!chain.same_structure(&moved));
    }

    #[test]
    fn older_chains_take_the_full_length_for_their_rooms() {
        let mut chains = StringChains::default();
        let mut room = BlockSetting::of(BlockChoice::Room);
        room.file = Some(PathBuf::from("/irs/hall.wav"));
        room.length = 2.5;
        chains.steel.blocks[0] = room;
        assert_eq!(chains.steel.blocks[0].length, 2.5, "kept");
        // Settings written before the knob existed have no `length`: the recording's own.
        let text = serde_json::to_string(&chains).unwrap();
        let older = text.replace(",\"length\":2.5", "");
        assert!(older.len() < text.len(), "the field was there: {text}");
        let back: StringChains = serde_json::from_str(&older).unwrap();
        assert_eq!(back.steel.blocks[0].length, 1.0);
        assert_eq!(back.steel.blocks[0].file, chains.steel.blocks[0].file);
    }

    #[test]
    fn an_amp_keeps_its_voice_and_knobs() {
        let mut amp = BlockSetting::of(BlockChoice::Amp);
        assert_eq!(amp.knobs().amp, AmpKnobs::default());
        amp.voice = AmpVoiceSetting::Lead;
        amp.drive = 0.8;
        amp.gain_db = -3.0;
        // A voice is chosen, not built: the chain is not prepared again.
        assert!(amp.same_structure(&BlockSetting::of(BlockChoice::Amp)));
        let knobs = amp.knobs().amp;
        assert_eq!(knobs.voice, AmpVoice::Lead);
        assert_eq!((knobs.gain, knobs.level_db), (0.8, -3.0));
        let mut chains = StringChains::default();
        chains.electric.blocks[0] = amp;
        let text = serde_json::to_string(&chains).unwrap();
        assert!(text.contains("\"voice\":\"lead\""), "{text}");
        let back: StringChains = serde_json::from_str(&text).unwrap();
        assert_eq!(back, chains);
    }

    #[test]
    fn a_drive_and_the_reverb_keep_their_knobs_and_old_reverbs_get_the_new_ones() {
        let drive = BlockSetting::of(BlockChoice::Drive);
        assert_eq!(drive.knobs().drive, DriveKnobs::default());
        let reverb = BlockSetting::of(BlockChoice::Reverb);
        assert_eq!(reverb.knobs().reverb, ReverbKnobs::default());
        // A reverb saved before decay, pre-delay and modulation existed.
        let mut chains = StringChains::default();
        chains.steel.blocks[0] = reverb;
        let text = serde_json::to_string(&chains).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let block = &mut value["steel"]["blocks"][0];
        for field in ["decay", "pre_delay", "modulation"] {
            assert!(
                block.as_object_mut().unwrap().remove(field).is_some(),
                "{field}"
            );
        }
        let back: StringChains = serde_json::from_value(value).unwrap();
        assert_eq!(back.steel.blocks[0].knobs().reverb, ReverbKnobs::default());
    }

    #[test]
    fn chains_round_trip_through_the_settings_file() {
        let mut chains = StringChains::default();
        let mut room = BlockSetting::of(BlockChoice::Room);
        room.file = Some(PathBuf::from("/irs/hall.wav"));
        chains.steel.blocks[2] = room;
        let text = serde_json::to_string(&chains).unwrap();
        let back: StringChains = serde_json::from_str(&text).unwrap();
        assert_eq!(back, chains);
        // Older settings without chains read as empty chains.
        let empty: StringChains = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, StringChains::default());
    }
}

/// The parts a drum track's sound is split into (the engine's `PARTS`, in its order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumPart {
    Kick,
    Snare,
    HiHat,
    Toms,
    Cymbals,
    Percussion,
}

impl DrumPart {
    /// Every part, in the order the kit is mixed in.
    pub const ALL: [DrumPart; 6] = [
        DrumPart::Kick,
        DrumPart::Snare,
        DrumPart::HiHat,
        DrumPart::Toms,
        DrumPart::Cymbals,
        DrumPart::Percussion,
    ];

    /// Its name, for the window.
    pub fn label(self) -> &'static str {
        match self {
            DrumPart::Kick => "Kick",
            DrumPart::Snare => "Snare",
            DrumPart::HiHat => "Hi-hat",
            DrumPart::Toms => "Toms",
            DrumPart::Cymbals => "Cymbals",
            DrumPart::Percussion => "Percussion",
        }
    }

    /// Its id in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            DrumPart::Kick => "kick",
            DrumPart::Snare => "snare",
            DrumPart::HiHat => "hi_hat",
            DrumPart::Toms => "toms",
            DrumPart::Cymbals => "cymbals",
            DrumPart::Percussion => "percussion",
        }
    }

    /// The part an `id` names, if it names one.
    pub fn from_id(id: &str) -> Option<DrumPart> {
        DrumPart::ALL.into_iter().find(|part| part.id() == id)
    }

    /// Its place in the engine's parts.
    pub fn index(self) -> usize {
        DrumPart::ALL
            .iter()
            .position(|part| *part == self)
            .unwrap_or(0)
    }

    /// The part at `index` (0 to 5), if there is one.
    pub fn at(index: usize) -> Option<DrumPart> {
        DrumPart::ALL.get(index).copied()
    }
}

/// An EQ block with `low_cut` and a high cut, then `bands` of (frequency, gain, width).
fn eq_block(low_cut: f32, high_cut: f32, bands: [(f32, f32, f32); BANDS]) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Eq);
    block.low_cut = low_cut;
    block.high_cut = high_cut;
    for (band, (freq, level, width)) in bands.into_iter().enumerate() {
        if let (Some(slot), Some(gain), Some(q)) = (
            block.eq_band_freq.get_mut(band),
            block.eq_band_gain.get_mut(band),
            block.eq_band_q.get_mut(band),
        ) {
            *slot = freq;
            *gain = level;
            *q = width;
        }
    }
    block
}

/// A compressor block: ratio, input and output (dB), attack and release (ms).
fn comp_block(
    ratio: CompRatioSetting,
    input: f32,
    output: f32,
    attack: f32,
    release: f32,
) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Compressor);
    block.comp_ratio = ratio;
    block.input_db = input;
    block.gain_db = output;
    block.attack_ms = attack;
    block.release_ms = release;
    block
}

/// A short room, mixed in under the drum.
fn room_block(size: f32, decay: f32, mix: f32) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Reverb);
    block.size = size;
    block.decay = decay;
    block.pre_delay = 8.0;
    block.mix = mix;
    block.high_cut = 6_000.0;
    block
}

/// A chain of `blocks` in the first places.
fn chain_of(blocks: Vec<BlockSetting>) -> ChainSetting {
    let mut chain = ChainSetting::default();
    for (slot, block) in chain.blocks.iter_mut().zip(blocks) {
        *slot = block;
    }
    chain
}

/// The built-in tone of one part of a drum kit: EQ, a compressor, sometimes a little room.
/// The owner's first version (2026-11-07); a part can be given a tone of its own later.
pub fn default_drum_part_chain(part: DrumPart) -> ChainSetting {
    match part {
        // The kick: body low, the boxy mids out, the beater's click in, held steady.
        DrumPart::Kick => chain_of(vec![
            eq_block(
                30.0,
                18_000.0,
                [
                    (80.0, 3.0, 0.707),
                    (350.0, -4.0, 1.2),
                    (3_500.0, 3.0, 1.0),
                    (8_000.0, 0.0, 0.707),
                ],
            ),
            comp_block(CompRatioSetting::Four, 6.0, -6.0, 0.3, 150.0),
        ]),
        // The snare: body, the honk out, the crack in, a little room for air.
        DrumPart::Snare => chain_of(vec![
            eq_block(
                80.0,
                18_000.0,
                [
                    (200.0, 3.0, 0.707),
                    (420.0, -2.0, 1.2),
                    (5_000.0, 4.0, 1.0),
                    (8_000.0, 0.0, 0.707),
                ],
            ),
            comp_block(CompRatioSetting::Eight, 6.0, -6.0, 0.2, 200.0),
            room_block(0.25, 0.8, 0.12),
        ]),
        // The hi-hat: the lows out, a little quieter, no room (it would smear the time).
        DrumPart::HiHat => chain_of(vec![
            eq_block(
                300.0,
                18_000.0,
                [
                    (300.0, 0.0, 0.707),
                    (1_000.0, 0.0, 1.0),
                    (6_000.0, 0.0, 1.0),
                    (8_000.0, 2.0, 0.707),
                ],
            ),
            comp_block(CompRatioSetting::Four, 3.0, -3.0, 0.2, 120.0),
        ]),
        // The toms: the boxy mids out, a little room, held like the kick.
        DrumPart::Toms => chain_of(vec![
            eq_block(
                60.0,
                18_000.0,
                [
                    (120.0, 0.0, 0.707),
                    (400.0, -4.0, 1.2),
                    (2_500.0, 2.0, 1.0),
                    (8_000.0, 0.0, 0.707),
                ],
            ),
            comp_block(CompRatioSetting::Four, 4.0, -4.0, 0.4, 250.0),
            room_block(0.3, 1.2, 0.1),
        ]),
        // The cymbals: the lows out, the harshness a little down, the air up.
        DrumPart::Cymbals => chain_of(vec![eq_block(
            400.0,
            18_000.0,
            [
                (400.0, 0.0, 0.707),
                (2_000.0, 0.0, 1.0),
                (6_000.0, -2.0, 1.0),
                (10_000.0, 1.5, 0.707),
            ],
        )]),
        // Everything else: only the rumble out.
        DrumPart::Percussion => chain_of(vec![eq_block(
            40.0,
            18_000.0,
            [
                (40.0, 0.0, 0.707),
                (1_000.0, 0.0, 1.0),
                (4_000.0, 0.0, 1.0),
                (8_000.0, 0.0, 0.707),
            ],
        )]),
    }
}

/// The built-in chain of the drum track itself (the drum bus): a light compressor that
/// glues the kit together.
pub fn default_drum_bus_chain() -> ChainSetting {
    chain_of(vec![comp_block(
        CompRatioSetting::Four,
        6.0,
        -5.0,
        0.6,
        120.0,
    )])
}

#[cfg(test)]
mod drum_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn every_drum_part_has_a_usable_tone() {
        for part in DrumPart::ALL {
            let chain = default_drum_part_chain(part);
            assert!(
                chain
                    .blocks
                    .iter()
                    .any(|block| block.choice != BlockChoice::Empty),
                "{} has blocks",
                part.label()
            );
            // Its knobs are in range.
            for block in &chain.blocks {
                assert!(block.low_cut.is_finite() && block.high_cut.is_finite());
                let settings = block.knobs();
                assert_eq!(settings, settings.sanitized());
            }
            assert_eq!(
                part.index(),
                DrumPart::ALL
                    .iter()
                    .position(|other| *other == part)
                    .unwrap()
            );
            assert_eq!(DrumPart::at(part.index()), Some(part));
        }
        // The bus is a compressor, and its parts are in the engine's order.
        let bus = default_drum_bus_chain();
        assert_eq!(bus.blocks[0].choice, BlockChoice::Compressor);
        for (index, part) in DrumPart::ALL.into_iter().enumerate() {
            assert_eq!(part.index(), index);
        }
    }
}
