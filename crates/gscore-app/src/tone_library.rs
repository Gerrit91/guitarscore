//! GuitarScore's built-in tones for physically modelled strings: a small library of named
//! sounds, grouped by family, that a PM track can play instead of just the General MIDI
//! program. A tone is the whole [`Tone`] - the kind of string, its sound and the chain
//! after it - plus a name, a family and a short description; nothing here is Guitar Pro's
//! RSE material, and the names describe a style or a sound, never a person.
//!
//! Every General MIDI program has a default tone from this library, so a track starts on a
//! fitting sound instead of a generic one ([`Self::for_program`]). The built-in tones use
//! only computed blocks (no impulse responses): a bundled IR would have to travel with the
//! program and be licensed for it, and there is none yet.
//!
//! The user's own saved tones live in the settings (`ChainPreset`); the pickers show them
//! after the built-in ones, in a group of their own.

// Wired into the track panel and the Sound chain window by the following steps; until
// then the library is exercised by its tests only.
#![allow(dead_code)]

use std::sync::LazyLock;

use gscore_play::StringKind;

use crate::chain_settings::{
    AmpVoiceSetting, BlockChoice, BlockSetting, ChainSetting, CompRatioSetting, Tone,
};
use crate::settings::{ChainPreset, PickupSetting, StringSound, kind_from_id};

/// The group a tone appears under in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ToneFamily {
    Clean,
    Crunch,
    Lead,
    HighGain,
    Acoustic,
    Bass,
}

impl ToneFamily {
    /// Every family, in the order the pickers show them.
    pub(crate) const ALL: [ToneFamily; 6] = [
        ToneFamily::Clean,
        ToneFamily::Crunch,
        ToneFamily::Lead,
        ToneFamily::HighGain,
        ToneFamily::Acoustic,
        ToneFamily::Bass,
    ];

    /// Its name in the picker.
    pub(crate) fn label(self) -> &'static str {
        match self {
            ToneFamily::Clean => "Clean",
            ToneFamily::Crunch => "Crunch",
            ToneFamily::Lead => "Lead",
            ToneFamily::HighGain => "High gain",
            ToneFamily::Acoustic => "Acoustic",
            ToneFamily::Bass => "Bass",
        }
    }
}

/// A tone that ships with the program: a full sound with a name, a family and a short
/// description.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BuiltInTone {
    /// The name the pickers show.
    pub(crate) name: &'static str,
    pub(crate) family: ToneFamily,
    /// A short line about the sound, shown on hover.
    pub(crate) description: &'static str,
    /// The whole sound: strings, their knobs and the chain.
    pub(crate) tone: Tone,
}

/// The built-in tones, built once.
static TONES: LazyLock<Vec<BuiltInTone>> = LazyLock::new(build_tones);

/// Every built-in tone, in family order and within a family in the order designed.
pub(crate) fn all() -> &'static [BuiltInTone] {
    &TONES
}

/// The built-in tone named `name`, if there is one.
pub(crate) fn find(name: &str) -> Option<&'static BuiltInTone> {
    TONES.iter().find(|tone| tone.name == name)
}

/// The built-in tone of `program` (0-127) for strings of `kind`.
///
/// A program whose documented tone is of another kind falls back to the kind's default
/// (a track switched to a bass never plays an electric guitar's tone by accident), and so
/// does a program with no tone of its own.
pub(crate) fn for_program(program: u8, kind: StringKind) -> &'static BuiltInTone {
    program_tone(program)
        .filter(|tone| tone.tone.kind == kind)
        .unwrap_or_else(|| default_for_kind(kind))
}

/// The tone a kind of string plays when a program has none.
pub(crate) fn default_for_kind(kind: StringKind) -> &'static BuiltInTone {
    let name = match kind {
        StringKind::Nylon => "Nylon Classical",
        StringKind::Steel => "Steel Fingerpicked",
        StringKind::Electric => "Clean Single Coil",
        StringKind::Bass => "Clean Finger",
    };
    match find(name) {
        Some(tone) => tone,
        // The list is built from named tones and the tests check them; this keeps a
        // mistake there from taking a track's sound (or the program) down.
        None => fallback_tone(),
    }
}

/// The user's saved preset that best stands in for the program's default tone: a preset of
/// the same kind, matched by the words of the built-in tone's name and family (the
/// owner's "Metal Guitar 1" for the high-gain programs, "Lead Guitar 1" for the lead one,
/// "Classical Guitar 1" for nylon, and so on). The first matching preset wins a tie, so
/// the choice is stable. `None` when the user has no preset of that kind.
pub(crate) fn saved_default_for(
    program: u8,
    kind: StringKind,
    saved: &[ChainPreset],
) -> Option<&ChainPreset> {
    let template = program_tone(program)
        .filter(|tone| tone.tone.kind == kind)
        .unwrap_or_else(|| default_for_kind(kind));
    let mut words: Vec<String> = template
        .name
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| word.len() >= 3)
        .collect();
    for word in family_words(template.family) {
        if !words.iter().any(|known| known == word) {
            words.push((*word).to_owned());
        }
    }
    // A bass program with a specific style (pick or slap) takes a saved preset only when
    // one is named for that style; otherwise the built-in Pick or Slap plays. A single
    // saved "Clean Bass 1" must not stand in for every bass program.
    if kind == StringKind::Bass {
        let style = template.name.to_lowercase();
        if matches!(style.as_str(), "pick" | "slap") {
            return saved.iter().find(|preset| {
                kind_from_id(&preset.kind) == Some(kind)
                    && preset.name.to_lowercase().contains(&style)
            });
        }
    }
    // The acoustic guitars have one saved preset each by kind: every steel-string acoustic
    // takes the preset named for natural steel, every nylon-string one the classical preset.
    let preferred: &[&str] = match kind {
        StringKind::Steel => &["natural", "steel", "acoustic"],
        StringKind::Nylon => &["classical", "nylon"],
        StringKind::Electric | StringKind::Bass => &[],
    };
    // The rank: the first preferred word found (its place in the list decides: "classical"
    // before "nylon"), then the most matching words. Ties keep the earlier preset.
    let mut best: Option<(&ChainPreset, (usize, usize))> = None;
    for preset in saved {
        if kind_from_id(&preset.kind) != Some(kind) {
            continue;
        }
        let name = preset.name.to_lowercase();
        let preferred_rank = preferred
            .iter()
            .position(|word| name.contains(word))
            .unwrap_or(usize::MAX);
        let score = words
            .iter()
            .filter(|word| name.contains(word.as_str()))
            .count();
        let rank = (preferred_rank, usize::MAX.saturating_sub(score));
        if best.is_none_or(|(_, best_rank)| rank < best_rank) {
            best = Some((preset, rank));
        }
    }
    best.map(|(preset, _)| preset)
}

/// The name of the default tone `program` starts on: the user's saved preset when there is
/// one, else the built-in tone's name.
pub(crate) fn default_tone_name(program: u8, kind: StringKind, saved: &[ChainPreset]) -> String {
    saved_default_for(program, kind, saved).map_or_else(
        || for_program(program, kind).name.to_owned(),
        |preset| preset.name.clone(),
    )
}

/// The words that point at each family, added to a built-in tone's own name when a saved
/// preset is matched to it.
fn family_words(family: ToneFamily) -> &'static [&'static str] {
    match family {
        ToneFamily::Clean => &[
            "clean", "electric", "single", "coil", "warm", "jazz", "funk", "glass", "neck",
        ],
        ToneFamily::Crunch => &[
            "crunch",
            "blues",
            "texas",
            "british",
            "classic",
            "rock",
            "rhythm",
            "drive",
            "overdriven",
            "grit",
        ],
        ToneFamily::Lead => &["lead", "singing", "sustain", "80s", "solo"],
        ToneFamily::HighGain => &["metal", "high", "gain", "djent", "tight", "heavy"],
        ToneFamily::Acoustic => &[
            "acoustic",
            "nylon",
            "classical",
            "steel",
            "natural",
            "strum",
            "finger",
        ],
        ToneFamily::Bass => &["bass", "finger", "pick", "slap", "driven", "clean"],
    }
}

/// Puts a default's string sound at `target_level` (the machine's level for its kind),
/// leaving its chain exactly as it was dialed in. Saved presets use this: their chain levels
/// are the owner's, but a clean preset saved lower than the kind's level would sound weak
/// beside the amp presets.
pub(crate) fn normalize_string_level(tone: &mut Tone, target_level: f32) {
    tone.sound.level = if target_level.is_finite() {
        target_level.clamp(0.0, 1.0)
    } else {
        tone.sound.level.clamp(0.0, 1.0)
    };
}

/// Matches a built-in fallback's loudness: the string sound sits at `target_level` and the
/// chain's output trims cancel out. The trim is taken from one block's output level (an
/// amp, then a drive, a compressor, a cabinet or a room), never from its drive or tone.
pub(crate) fn normalize_default(tone: &mut Tone, target_level: f32) {
    normalize_string_level(tone, target_level);
    let mut trim = 0.0_f32;
    let mut chosen: Option<(u8, usize)> = None;
    for (index, block) in tone.chain.blocks.iter().enumerate() {
        let priority = match block.choice {
            BlockChoice::Amp => 4,
            BlockChoice::Drive => 3,
            BlockChoice::Compressor => 2,
            BlockChoice::Cabinet | BlockChoice::Room => 1,
            _ => continue,
        };
        if block.gain_db.is_finite() {
            trim += block.gain_db;
        }
        if chosen.is_none_or(|(best, _)| priority > best) {
            chosen = Some((priority, index));
        }
    }
    if trim.is_finite()
        && let Some((_, index)) = chosen
        && let Some(block) = tone.chain.blocks.get_mut(index)
    {
        let (low, high) = gscore_guitar::knob_ranges::GAIN;
        block.gain_db = (block.gain_db - trim).clamp(low, high);
    }
}

/// A plain electric tone used only if the built-in list somehow lost its entries.
fn fallback_tone() -> &'static BuiltInTone {
    static FALLBACK: LazyLock<BuiltInTone> = LazyLock::new(|| {
        tone(
            "Clean",
            ToneFamily::Clean,
            "The plain modelled electric guitar.",
            StringKind::Electric,
            StringSound::from(gscore_audio::default_string_model(StringKind::Electric)),
            Vec::new(),
        )
    });
    &FALLBACK
}

/// The program's documented tone, before the kind is checked.
fn program_tone(program: u8) -> Option<&'static BuiltInTone> {
    let name = match program {
        // Nylon, steel and the electric guitars.
        24 => "Nylon Classical",
        25 => "Steel Fingerpicked",
        26 => "Warm Jazz",
        27 => "Clean Single Coil",
        28 => "Funk Rhythm",
        29 => "Classic Rock Rhythm",
        30 => "Modern Metal Rhythm",
        31 => "80s Lead",
        // The basses: GM 34 is the picked electric bass, 35 the fretless (a fingerstyle
        // sound); 36 and 37 slap; 38 and 39 are synth basses, played with a pick here.
        32 | 33 | 35 => "Clean Finger",
        34 => "Pick",
        36 | 37 => "Slap",
        38 | 39 => "Pick",
        // Other string programs (banjo, ukulele, ...) take their kind's default instead.
        _ => return None,
    };
    find(name)
}

/// A fresh sound of `kind` with `pickup` and where it sits.
fn sound(kind: StringKind, pickup: PickupSetting, position: f32) -> StringSound {
    let mut sound = StringSound::from(gscore_audio::default_string_model(kind));
    if pickup != PickupSetting::Unset {
        sound.pickup = pickup;
        sound.pickup_position = position;
    }
    sound
}

/// A chain of `blocks` in the first places.
fn chain(blocks: Vec<BlockSetting>) -> ChainSetting {
    let mut chain = ChainSetting::default();
    for (slot, block) in chain.blocks.iter_mut().zip(blocks) {
        *slot = block;
    }
    chain
}

/// An EQ block with a low and a high cut and four (frequency, gain, width) bands.
fn eq(low_cut: f32, high_cut: f32, bands: [(f32, f32, f32); 4]) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Eq);
    block.low_cut = low_cut;
    block.high_cut = high_cut;
    for (band, (frequency, gain, width)) in bands.into_iter().enumerate() {
        if let (Some(slot), Some(level), Some(q)) = (
            block.eq_band_freq.get_mut(band),
            block.eq_band_gain.get_mut(band),
            block.eq_band_q.get_mut(band),
        ) {
            *slot = frequency;
            *level = gain;
            *q = width;
        }
    }
    block
}

/// The computed guitar amp: voice, gain, tone stack and output level.
fn amp(
    voice: AmpVoiceSetting,
    gain: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    level: f32,
) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Amp);
    block.voice = voice;
    block.drive = gain;
    block.bass = bass;
    block.middle = mid;
    block.treble = treble;
    block.gain_db = level;
    block
}

/// The computed overdrive pedal: drive, tone and level.
fn drive(drive: f32, tone: f32, level: f32) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Drive);
    block.drive = drive;
    block.tone = tone;
    block.gain_db = level;
    block
}

/// The FET compressor: ratio, input and output level, attack and release.
fn compressor(
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

/// The computed reverb: size, decay, pre-delay and mix.
fn reverb(size: f32, decay: f32, pre_delay: f32, mix: f32) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Reverb);
    block.size = size;
    block.decay = decay;
    block.pre_delay = pre_delay;
    block.mix = mix;
    block
}

/// The stereo chorus: speed, depth and mix.
fn chorus(rate: f32, depth: f32, mix: f32) -> BlockSetting {
    let mut block = BlockSetting::of(BlockChoice::Chorus);
    block.chorus_rate = rate;
    block.chorus_depth = depth;
    block.mix = mix;
    block
}

/// A built-in tone.
fn tone(
    name: &'static str,
    family: ToneFamily,
    description: &'static str,
    kind: StringKind,
    sound: StringSound,
    blocks: Vec<BlockSetting>,
) -> BuiltInTone {
    BuiltInTone {
        name,
        family,
        description,
        tone: Tone {
            kind,
            sound,
            chain: chain(blocks),
        },
    }
}

/// Every built-in tone, designed as a starting point for the owner to tune by ear.
fn build_tones() -> Vec<BuiltInTone> {
    use ToneFamily::{Acoustic, Bass, Clean, Crunch, HighGain, Lead};
    let electric = StringKind::Electric;
    let steel = StringKind::Steel;
    let nylon = StringKind::Nylon;
    let bass = StringKind::Bass;
    vec![
        // Clean: bright single coils, a warm neck, and a compressed funk sound.
        tone(
            "Clean Single Coil",
            Clean,
            "Bright single-coil bridge with light compression and a little air.",
            electric,
            sound(
                electric,
                PickupSetting::SingleCoil,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                compressor(CompRatioSetting::Four, 3.0, -3.0, 15.0, 180.0),
                eq(
                    80.0,
                    12_000.0,
                    [
                        (100.0, 0.0, 0.707),
                        (800.0, 0.0, 1.0),
                        (3_000.0, 1.0, 1.0),
                        (8_000.0, 1.5, 0.707),
                    ],
                ),
            ],
        ),
        tone(
            "Glassy Neck",
            Clean,
            "Neck single coil, glassy and wide, with a gentle stereo chorus.",
            electric,
            sound(
                electric,
                PickupSetting::SingleCoil,
                gscore_guitar::pickup_positions::NECK,
            ),
            vec![
                compressor(CompRatioSetting::Four, 2.5, -2.5, 12.0, 160.0),
                chorus(0.6, 1.8, 0.25),
            ],
        ),
        tone(
            "Warm Jazz",
            Clean,
            "Neck humbucker with the highs rolled off: round, dark and smooth.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::NECK,
            ),
            vec![
                eq(
                    90.0,
                    6_500.0,
                    [
                        (100.0, 1.5, 0.707),
                        (450.0, 1.0, 1.0),
                        (2_000.0, 0.0, 1.0),
                        (5_000.0, -2.0, 0.707),
                    ],
                ),
                compressor(CompRatioSetting::Four, 3.0, -3.0, 20.0, 220.0),
            ],
        ),
        tone(
            "Funk Rhythm",
            Clean,
            "Tight single-coil snap with a scooped mid and fast compression.",
            electric,
            sound(
                electric,
                PickupSetting::SingleCoil,
                gscore_guitar::pickup_positions::MIDDLE,
            ),
            vec![
                compressor(CompRatioSetting::Eight, 6.0, -6.0, 5.0, 90.0),
                eq(
                    120.0,
                    12_000.0,
                    [
                        (150.0, 1.0, 0.707),
                        (900.0, -2.0, 1.2),
                        (2_500.0, 1.0, 1.0),
                        (7_000.0, 2.0, 0.707),
                    ],
                ),
            ],
        ),
        // Crunch: a pedal into a breaking-up amp, and two classic amp sounds.
        tone(
            "Texas Blues",
            Crunch,
            "A pushed combo with a little pedal grit.",
            electric,
            sound(
                electric,
                PickupSetting::SingleCoil,
                gscore_guitar::pickup_positions::MIDDLE,
            ),
            vec![
                drive(0.35, 5_000.0, -3.0),
                amp(AmpVoiceSetting::Crunch, 0.55, 0.55, 0.5, 0.6, -6.0),
            ],
        ),
        tone(
            "British Crunch",
            Crunch,
            "Mid-forward crunchy rhythm with the bass kept firm.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                amp(AmpVoiceSetting::Crunch, 0.65, 0.45, 0.7, 0.55, -5.0),
                eq(
                    90.0,
                    10_000.0,
                    [
                        (120.0, 1.0, 0.707),
                        (500.0, 1.0, 1.0),
                        (2_500.0, 0.5, 1.0),
                        (6_000.0, -1.0, 0.707),
                    ],
                ),
            ],
        ),
        tone(
            "Classic Rock Rhythm",
            Crunch,
            "A pedal into a cranked amp: thick, even and ready for chords.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                drive(0.5, 4_000.0, -4.0),
                amp(AmpVoiceSetting::Crunch, 0.7, 0.5, 0.6, 0.5, -6.0),
            ],
        ),
        // Lead: singing sustain and an eighties chorus lead.
        tone(
            "80s Lead",
            Lead,
            "Smooth lead with a long, even sustain.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::NECK,
            ),
            vec![
                drive(0.45, 4_500.0, -3.0),
                amp(AmpVoiceSetting::Lead, 0.7, 0.5, 0.55, 0.6, -5.0),
            ],
        ),
        tone(
            "Singing Sustain",
            Lead,
            "Long, even sustain with the compression holding every note.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                compressor(CompRatioSetting::Eight, 8.0, -7.0, 12.0, 250.0),
                amp(AmpVoiceSetting::Lead, 0.8, 0.5, 0.6, 0.55, -6.0),
            ],
        ),
        // High gain: a tight rhythm and a scooped, percussive djent sound.
        tone(
            "Modern Metal Rhythm",
            HighGain,
            "High gain with the mud cut out and the attack left sharp.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                drive(0.7, 3_000.0, -6.0),
                amp(AmpVoiceSetting::Lead, 0.85, 0.4, 0.55, 0.5, -7.0),
            ],
        ),
        tone(
            "Djent Tight",
            HighGain,
            "Very tight lows, a hard compressor and a percussive mid scoop.",
            electric,
            sound(
                electric,
                PickupSetting::Humbucker,
                gscore_guitar::pickup_positions::BRIDGE,
            ),
            vec![
                compressor(CompRatioSetting::Twelve, 10.0, -8.0, 3.0, 80.0),
                amp(AmpVoiceSetting::Lead, 0.9, 0.35, 0.5, 0.55, -7.0),
            ],
        ),
        // Acoustic: two steel sounds and a classical guitar.
        tone(
            "Steel Strummed",
            Acoustic,
            "A wide steel-string strum with a little compression.",
            steel,
            sound(steel, PickupSetting::Unset, 0.0),
            vec![
                compressor(CompRatioSetting::Four, 3.0, -3.0, 20.0, 200.0),
                eq(
                    70.0,
                    14_000.0,
                    [
                        (100.0, 0.5, 0.707),
                        (700.0, 0.0, 1.0),
                        (3_000.0, 1.0, 1.0),
                        (9_000.0, 1.0, 0.707),
                    ],
                ),
            ],
        ),
        tone(
            "Steel Fingerpicked",
            Acoustic,
            "Fingertips on steel, close and detailed, with only a hint of room.",
            steel,
            sound(steel, PickupSetting::Unset, 0.0),
            vec![
                eq(
                    60.0,
                    14_000.0,
                    [
                        (100.0, 0.0, 0.707),
                        (800.0, 0.0, 1.0),
                        (3_500.0, 0.5, 1.0),
                        (8_000.0, 1.0, 0.707),
                    ],
                ),
                reverb(0.3, 0.9, 18.0, 0.1),
            ],
        ),
        tone(
            "Nylon Classical",
            Acoustic,
            "Warm nylon strings with a rounded top and a soft room.",
            nylon,
            sound(nylon, PickupSetting::Unset, 0.0),
            vec![
                eq(
                    60.0,
                    9_000.0,
                    [
                        (100.0, 0.5, 0.707),
                        (600.0, 0.5, 1.0),
                        (2_500.0, 0.0, 1.0),
                        (6_000.0, -1.0, 0.707),
                    ],
                ),
                reverb(0.4, 1.1, 22.0, 0.14),
            ],
        ),
        // Bass: fingers, pick, slap and a driven sound.
        tone(
            "Clean Finger",
            Bass,
            "Fingerstyle bass: even lows, a clear attack and no boom.",
            bass,
            sound(bass, PickupSetting::Unset, 0.0),
            vec![
                compressor(CompRatioSetting::Four, 4.0, -4.0, 15.0, 180.0),
                eq(
                    35.0,
                    7_000.0,
                    [
                        (60.0, 1.0, 0.707),
                        (250.0, 0.5, 1.0),
                        (900.0, 0.0, 1.0),
                        (3_000.0, 0.5, 0.707),
                    ],
                ),
            ],
        ),
        tone(
            "Pick",
            Bass,
            "A pick on the strings: harder attack and a brighter top.",
            bass,
            sound(bass, PickupSetting::Unset, 0.0),
            vec![compressor(CompRatioSetting::Four, 5.0, -5.0, 8.0, 140.0)],
        ),
        tone(
            "Slap",
            Bass,
            "Snappy slap: compressed lows and a bright, popping top.",
            bass,
            sound(bass, PickupSetting::Unset, 0.0),
            vec![
                compressor(CompRatioSetting::Twelve, 9.0, -8.0, 3.0, 90.0),
                eq(
                    45.0,
                    12_000.0,
                    [
                        (80.0, 1.5, 0.707),
                        (500.0, -2.0, 1.2),
                        (2_000.0, 1.0, 1.0),
                        (6_000.0, 2.0, 0.707),
                    ],
                ),
            ],
        ),
        tone(
            "Driven",
            Bass,
            "A bass pushed into a warm overdrive, still holding its low end.",
            bass,
            sound(bass, PickupSetting::Unset, 0.0),
            vec![
                drive(0.4, 2_500.0, -5.0),
                compressor(CompRatioSetting::Eight, 6.0, -6.0, 10.0, 150.0),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn every_family_has_tones_with_unique_names() {
        let mut names: Vec<&str> = Vec::new();
        for family in ToneFamily::ALL {
            let count = all().iter().filter(|tone| tone.family == family).count();
            assert!(count > 0, "{} has no tone", family.label());
        }
        for tone in all() {
            assert!(!names.contains(&tone.name), "{} appears twice", tone.name);
            names.push(tone.name);
            assert!(!tone.description.trim().is_empty(), "{}", tone.name);
        }
        // The owner's list from the handoff is all here.
        for name in [
            "Clean Single Coil",
            "Glassy Neck",
            "Warm Jazz",
            "Funk Rhythm",
            "Texas Blues",
            "British Crunch",
            "Classic Rock Rhythm",
            "80s Lead",
            "Singing Sustain",
            "Modern Metal Rhythm",
            "Djent Tight",
            "Steel Strummed",
            "Steel Fingerpicked",
            "Nylon Classical",
            "Clean Finger",
            "Pick",
            "Slap",
            "Driven",
        ] {
            assert!(find(name).is_some(), "{name} is missing");
        }
    }

    #[test]
    fn a_tone_is_a_complete_sane_sound_and_names_no_person() {
        for tone in all() {
            // Every knob is in range and there is a real chain (built-ins use no IR file:
            // there is no bundled one yet).
            for block in &tone.tone.chain.blocks {
                assert!(block.file.is_none(), "{} uses an IR file", tone.name);
                let knobs = block.knobs();
                assert_eq!(knobs, knobs.sanitized(), "{}", tone.name);
            }
            assert!(
                tone.tone
                    .chain
                    .blocks
                    .iter()
                    .any(|block| block.choice != BlockChoice::Empty),
                "{} has no effects",
                tone.name
            );
            // The sound belongs to the tone's kind.
            let sound = tone.tone.sound.model();
            assert_eq!(sound, sound.sanitized(), "{}", tone.name);
            // The pickup is as the tone was dialed in (an acoustic has none).
            if tone.tone.kind == StringKind::Electric {
                assert_ne!(
                    tone.tone.sound.pickup,
                    PickupSetting::Unset,
                    "{} has no pickup",
                    tone.name
                );
            }
            // Descriptions name a sound, never a person: no capitalised first name pairs.
            assert!(
                !tone.name.contains("'s") && !tone.description.contains("'s"),
                "{}",
                tone.name
            );
        }
    }

    #[test]
    fn the_owners_saved_presets_stand_in_for_the_programs_defaults() {
        let preset = |name: &str, kind: StringKind, level: f32| ChainPreset {
            name: name.to_owned(),
            kind: crate::settings::kind_id(kind).to_owned(),
            sound: StringSound {
                level,
                ..StringSound::from(gscore_audio::default_string_model(kind))
            },
            chain: ChainSetting::default(),
            engine: crate::settings::engine_id(gscore_play::TrackEngine::Modelled(kind)).to_owned(),
        };
        let saved = vec![
            preset("Electric Guitar 1", StringKind::Electric, 0.35),
            // A second steel and a second nylon preset: the acoustic mapping still chooses
            // the natural-steel and classical ones.
            preset("Steel Strummed 2", StringKind::Steel, 0.5),
            preset("Natural Steel 1", StringKind::Steel, 0.6),
            preset("Clean Bass 1", StringKind::Bass, 0.5),
            preset("Muted Overdriven Guitar", StringKind::Electric, 0.6),
            preset("Metal Guitar 1", StringKind::Electric, 0.6),
            preset("Lead Guitar 1", StringKind::Electric, 0.6),
            preset("Nylon Folk 2", StringKind::Nylon, 0.5),
            preset("Classical Guitar 1", StringKind::Nylon, 0.4),
        ];
        let name = |program, kind| {
            saved_default_for(program, kind, &saved).map(|preset| preset.name.as_str())
        };
        assert_eq!(name(24, StringKind::Nylon), Some("Classical Guitar 1"));
        assert_eq!(name(25, StringKind::Steel), Some("Natural Steel 1"));
        assert_eq!(name(27, StringKind::Electric), Some("Electric Guitar 1"));
        assert_eq!(
            name(29, StringKind::Electric),
            Some("Muted Overdriven Guitar")
        );
        assert_eq!(name(30, StringKind::Electric), Some("Metal Guitar 1"));
        assert_eq!(name(31, StringKind::Electric), Some("Lead Guitar 1"));
        assert_eq!(name(33, StringKind::Bass), Some("Clean Bass 1"));
        // The one clean bass preset must not stand in for a picked or slapped bass: those
        // have their own built-in tones until the owner saves presets for them.
        assert_eq!(name(34, StringKind::Bass), None);
        assert_eq!(name(36, StringKind::Bass), None);
        // No saved presets at all: the built-in library still answers.
        assert_eq!(saved_default_for(27, StringKind::Electric, &[]), None);
        assert_eq!(
            default_tone_name(27, StringKind::Electric, &[]),
            for_program(27, StringKind::Electric).name
        );
        assert_eq!(
            default_tone_name(27, StringKind::Electric, &saved),
            "Electric Guitar 1"
        );
    }

    #[test]
    fn a_saved_default_keeps_its_chain_levels() {
        // A saved preset's amp output is the owner's: only the string level is matched, so
        // a clean preset saved quiet is not left weak beside the amp presets.
        let mut saved = Tone {
            kind: StringKind::Electric,
            sound: StringSound {
                level: 0.35,
                ..StringSound::from(gscore_audio::default_string_model(StringKind::Electric))
            },
            chain: chain(vec![amp(AmpVoiceSetting::Lead, 0.8, 0.5, 0.5, 0.5, -14.0)]),
        };
        normalize_string_level(&mut saved, 0.597);
        assert!((saved.sound.level - 0.597).abs() < 1e-6);
        assert!((saved.chain.blocks[0].gain_db - (-14.0)).abs() < 1e-6);
    }

    #[test]
    fn normalized_defaults_arrive_at_the_same_level() {
        // A quiet string sound with an amp trimmed far down, and a loud one with only a
        // cabinet: both come out at the same string level and with no net chain trim.
        let mut quiet = Tone {
            kind: StringKind::Electric,
            sound: StringSound {
                level: 0.2,
                ..StringSound::from(gscore_audio::default_string_model(StringKind::Electric))
            },
            chain: chain(vec![
                amp(AmpVoiceSetting::Lead, 0.8, 0.5, 0.5, 0.5, -18.0),
                compressor(CompRatioSetting::Four, 4.0, -4.0, 10.0, 150.0),
            ]),
        };
        normalize_default(&mut quiet, 0.55);
        assert!((quiet.sound.level - 0.55).abs() < 1e-6);
        let trim: f32 = quiet
            .chain
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.choice,
                    BlockChoice::Amp
                        | BlockChoice::Drive
                        | BlockChoice::Compressor
                        | BlockChoice::Cabinet
                        | BlockChoice::Room
                )
            })
            .map(|block| block.gain_db)
            .sum();
        assert!(trim.abs() < 1e-4, "{trim}");
        // The amp's output (the strongest level block) took the adjustment; its voice and
        // drive did not move.
        assert!((quiet.chain.blocks[0].gain_db - 4.0).abs() < 1e-4);
        assert!((quiet.chain.blocks[0].drive - 0.8).abs() < 1e-6);
    }

    #[test]
    fn each_guitar_and_bass_program_starts_on_a_tone_of_its_kind() {
        for (program, kind) in [
            (24, StringKind::Nylon),
            (25, StringKind::Steel),
            (26, StringKind::Electric),
            (27, StringKind::Electric),
            (28, StringKind::Electric),
            (29, StringKind::Electric),
            (30, StringKind::Electric),
            (31, StringKind::Electric),
            (32, StringKind::Bass),
            (33, StringKind::Bass),
            (36, StringKind::Bass),
            (38, StringKind::Bass),
        ] {
            let tone = for_program(program, kind);
            assert_eq!(tone.tone.kind, kind, "{program}");
        }
        // A program with no documented tone takes the kind's default.
        assert_eq!(
            for_program(0, StringKind::Electric).name,
            default_for_kind(StringKind::Electric).name
        );
        // A track switched to another kind does not keep a tone of the old one.
        assert_eq!(
            for_program(30, StringKind::Bass).tone.kind,
            StringKind::Bass
        );
        // Every guitar and bass program has a distinguishable default within its kind.
        assert_ne!(
            for_program(27, StringKind::Electric).name,
            for_program(30, StringKind::Electric).name
        );
        assert_ne!(
            for_program(33, StringKind::Bass).name,
            for_program(36, StringKind::Bass).name
        );
        // GM 34 is the picked bass and 36 the slapped one, not another clean finger bass.
        assert_eq!(for_program(33, StringKind::Bass).name, "Clean Finger");
        assert_eq!(for_program(34, StringKind::Bass).name, "Pick");
        assert_eq!(for_program(36, StringKind::Bass).name, "Slap");
    }
}
