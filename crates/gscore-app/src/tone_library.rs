//! GuitarScore's built-in tones for physically modelled strings: a small library of named
//! sounds, grouped by family, that a PM track can play instead of just the General MIDI
//! program. A tone is the whole [`Tone`] - the kind of string, its sound and the chain
//! after it - plus a name, a family and a short description; nothing here is Guitar Pro's
//! RSE material, and the names describe a style or a sound, never a person.
//!
//! A guitar or bass program may start on a tone from this library ([`for_program`]); a
//! program without one plays the plain strings of its kind, with no effects. Every tone
//! here was dialed in by the owner and imported with `cargo xtask tones import`, which
//! also remembers the saved tone it came from; none is made up. The tones and the program
//! map live in `data/tones.json`, embedded at build time, so the tool can add one without
//! touching Rust code. A tone may point at an impulse response file, which is not bundled
//! (the owner is still collecting them), and the chain skips a block whose file is
//! missing. A bundled IR would have to travel with the program and be licensed for it.
//!
//! The user's own saved tones live in the settings (`ChainPreset`); the pickers show them
//! after the built-in ones, in a group of their own.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use gscore_play::StringKind;
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::chain_settings::{AmpVoiceSetting, BlockSetting, CompRatioSetting};
use crate::chain_settings::{BlockChoice, ChainSetting, Tone};
use crate::settings::{StringSound, kind_from_id};

/// The group a tone appears under in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    /// Its id in the embedded file and on the command line (used by `cargo xtask tones`,
    /// which only the desktop build has).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn id(self) -> &'static str {
        match self {
            ToneFamily::Clean => "clean",
            ToneFamily::Crunch => "crunch",
            ToneFamily::Lead => "lead",
            ToneFamily::HighGain => "high_gain",
            ToneFamily::Acoustic => "acoustic",
            ToneFamily::Bass => "bass",
        }
    }

    /// The family `id` names, if it names one.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn from_id(id: &str) -> Option<ToneFamily> {
        ToneFamily::ALL.into_iter().find(|family| family.id() == id)
    }
}

/// A tone that ships with the program: a full sound with a name, a family and a short
/// description.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BuiltInTone {
    /// The name the pickers show.
    pub(crate) name: String,
    pub(crate) family: ToneFamily,
    /// A short line about the sound, shown on hover.
    pub(crate) description: String,
    /// The whole sound: strings, their knobs and the chain.
    pub(crate) tone: Tone,
}

/// One tone as the embedded file keeps it: the whole sound plus its name, family and
/// description. The kind is an id (`nylon`, `steel`, `electric`, `bass`), as the settings
/// keep it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToneEntry {
    /// The name the pickers show.
    pub(crate) name: String,
    pub(crate) family: ToneFamily,
    /// A short line about the sound, shown on hover.
    pub(crate) description: String,
    /// The kind of string the tone was dialed in on.
    pub(crate) kind: String,
    pub(crate) sound: StringSound,
    pub(crate) chain: ChainSetting,
    /// The saved tone it was imported from, if `cargo xtask tones import` made it. Only the
    /// tool reads this; the program plays the tone as it stands here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<ToneSource>,
}

/// Where an imported tone came from: the saved tone in the settings, and what it looked
/// like then, so `cargo xtask tones` can tell when it has changed since.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToneSource {
    /// The saved tone's name in the settings.
    pub(crate) preset: String,
    /// A fingerprint of the saved tone (kind, strings and chain) when it was copied.
    pub(crate) digest: String,
    /// The import replaced the saved tone's impulse responses with the computed blocks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) drop_irs: bool,
}

/// The embedded library file: every built-in tone and the General MIDI program it starts
/// on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToneLibraryFile {
    /// The guitar and bass programs (24-39) and the name of the tone each starts on.
    #[serde(default)]
    pub(crate) programs: BTreeMap<u8, String>,
    #[serde(default)]
    pub(crate) tones: Vec<ToneEntry>,
    /// Guitar Pro's RSE sounds (see [`crate::rse_sound::key`]) and the name of the tone a
    /// track with that sound starts on, ahead of its program's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) rse: BTreeMap<String, String>,
}

impl ToneEntry {
    /// The built-in tone it stands for, if its kind is one we know.
    pub(crate) fn built_in(&self) -> Option<BuiltInTone> {
        Some(BuiltInTone {
            name: self.name.clone(),
            family: self.family,
            description: self.description.clone(),
            tone: {
                let kind = kind_from_id(&self.kind)?;
                Tone {
                    kind,
                    // A body or pickup the file does not name is the kind's.
                    sound: self.sound.for_kind(kind),
                    chain: self.chain.clone(),
                }
            },
        })
    }
}

/// The embedded library, parsed once.
#[derive(Debug, Default)]
struct Library {
    tones: Vec<BuiltInTone>,
    /// Guitar and bass programs and the name of the tone each starts on.
    programs: BTreeMap<u8, String>,
    /// Guitar Pro's RSE sounds and the name of the tone each starts on.
    rse: BTreeMap<String, String>,
}

/// The built-in tones as the program ships them, embedded at build time.
pub(crate) const TONES_JSON: &str = include_str!("../data/tones.json");

/// The embedded library, parsed once. A malformed file leaves the library empty and every
/// program on its plain strings; a test keeps that from happening.
static LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let file: ToneLibraryFile = match serde_json::from_str(TONES_JSON) {
        Ok(file) => file,
        Err(_) => return Library::default(),
    };
    Library {
        tones: file.tones.iter().filter_map(ToneEntry::built_in).collect(),
        programs: file.programs,
        rse: file.rse,
    }
});

/// Every built-in tone, in family order and within a family in the order designed.
pub(crate) fn all() -> &'static [BuiltInTone] {
    &LIBRARY.tones
}

/// The built-in tone named `name`, if there is one.
pub(crate) fn find(name: &str) -> Option<&'static BuiltInTone> {
    LIBRARY.tones.iter().find(|tone| tone.name == name)
}

/// The built-in tone `program` (0-127) starts on when strings of `kind` play it, if it has
/// one. A tone of another kind does not count: a track switched to a bass never plays an
/// electric guitar's tone by accident.
pub(crate) fn for_program(program: u8, kind: StringKind) -> Option<&'static BuiltInTone> {
    LIBRARY.for_program(program, kind)
}

/// The built-in tone a track with Guitar Pro's RSE sound `key` (see
/// [`crate::rse_sound::key`]) starts on, if the library maps that sound to one.
pub(crate) fn for_rse(key: &str) -> Option<&'static BuiltInTone> {
    LIBRARY.for_rse(key)
}

impl Library {
    fn for_rse(&self, key: &str) -> Option<&BuiltInTone> {
        let name = self.rse.get(key)?;
        self.tones.iter().find(|tone| &tone.name == name)
    }

    fn for_program(&self, program: u8, kind: StringKind) -> Option<&BuiltInTone> {
        let name = self.programs.get(&program)?;
        self.tones
            .iter()
            .find(|tone| &tone.name == name)
            .filter(|tone| tone.tone.kind == kind)
    }
}

/// Puts a default's string sound at `target_level` (the machine's level for its kind),
/// leaving its chain exactly as it was dialed in: the chain levels are the owner's, but a
/// clean tone saved lower than the kind's level would sound weak beside the amp tones.
pub(crate) fn normalize_string_level(tone: &mut Tone, target_level: f32) {
    tone.sound.level = if target_level.is_finite() {
        target_level.clamp(0.0, 1.0)
    } else {
        tone.sound.level.clamp(0.0, 1.0)
    };
}

/// The sum of the blocks' output levels (an amp, a drive, a compressor, a cabinet, a
/// room): what makes one tone louder than another with the same strings. Drive and tone
/// knobs are never part of it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn chain_level_trim(chain: &ChainSetting) -> f32 {
    let sum: f32 = chain
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
        .filter(|block| block.gain_db.is_finite())
        .map(|block| block.gain_db)
        .sum();
    // Adding zero turns a negative zero into a positive one, so reports print "0.0".
    if sum == 0.0 { 0.0 } else { sum }
}

/// The static leveling, for `cargo xtask tones normalize --static`: the string sound at
/// `target_level` and the chain's level trims cancelled by the *output* knob, leaving the
/// blocks' gain staging exactly as dialed in. (Playback no longer levels built-in tones
/// this way: their output level is measured once and stored with the tone.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn normalize_static(tone: &mut Tone, target_level: f32) {
    normalize_string_level(tone, target_level);
    let trim = chain_level_trim(&tone.chain);
    if trim.is_finite() {
        let (low, high) = gscore_guitar::knob_ranges::OUTPUT;
        tone.chain.output_db = (-trim).clamp(low, high);
    }
}

/// A chain of `blocks` in the first places.
#[cfg(test)]
fn chain(blocks: Vec<BlockSetting>) -> ChainSetting {
    let mut chain = ChainSetting::default();
    for (slot, block) in chain.blocks.iter_mut().zip(blocks) {
        *slot = block;
    }
    chain
}

/// The computed guitar amp: voice, gain, tone stack and output level.
#[cfg(test)]
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

/// The FET compressor: ratio, input and output level, attack and release.
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_tone_is_a_complete_sane_sound_with_a_unique_name() {
        let mut names: Vec<&str> = Vec::new();
        for tone in all() {
            assert!(!names.contains(&tone.name.as_str()), "{} twice", tone.name);
            names.push(&tone.name);
            assert!(!tone.description.trim().is_empty(), "{}", tone.name);
            // Every knob is in range. An impulse response path is allowed for now (the
            // owner is still collecting the files; the chain skips a block whose file is
            // missing), and so is a tone that is only its strings.
            for block in &tone.tone.chain.blocks {
                let knobs = block.knobs();
                assert_eq!(knobs, knobs.sanitized(), "{}", tone.name);
            }
            let sound = tone.tone.sound.model();
            assert_eq!(sound, sound.sanitized(), "{}", tone.name);
        }
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
    fn static_leveling_lands_on_the_output_knob_and_leaves_the_blocks_alone() {
        // A quiet string sound with an amp trimmed far down and a compressor: the static
        // leveling sets the string level and cancels the trims at the output knob, without
        // moving the amp's gain staging (its drive, or the level into the cabinet).
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
        normalize_static(&mut quiet, 0.55);
        assert!((quiet.sound.level - 0.55).abs() < 1e-6);
        // -18 + -4 dB of trims are cancelled at the output.
        assert!(
            (quiet.chain.output_db - 22.0).abs() < 1e-4,
            "{}",
            quiet.chain.output_db
        );
        assert!((quiet.chain.blocks[0].gain_db - (-18.0)).abs() < 1e-6);
        assert!((quiet.chain.blocks[0].drive - 0.8).abs() < 1e-6);
        assert!((quiet.chain.blocks[1].gain_db - (-4.0)).abs() < 1e-6);
        assert!((chain_level_trim(&quiet.chain) - (-22.0)).abs() < 1e-4);
    }

    #[test]
    fn the_embedded_file_parses_and_every_program_names_a_tone_of_its_kind() {
        let file: ToneLibraryFile = serde_json::from_str(TONES_JSON).unwrap();
        assert_eq!(file.tones.len(), all().len(), "a tone was dropped");
        assert_eq!(file.programs, LIBRARY.programs);
        for (&program, name) in &LIBRARY.programs {
            let tone = find(name).unwrap_or_else(|| panic!("program {program} names {name}"));
            let kind = StringKind::for_program(program)
                .unwrap_or_else(|| panic!("{program} is no guitar or bass program"));
            assert_eq!(tone.tone.kind, kind, "program {program} starts on {name}");
            assert_eq!(
                for_program(program, kind).map(|tone| &tone.name),
                Some(name)
            );
        }
    }

    #[test]
    fn a_program_starts_on_its_tone_only_on_that_tones_kind_of_string() {
        let tone = |name: &str, kind| BuiltInTone {
            name: name.to_owned(),
            family: ToneFamily::Clean,
            description: "A tone.".to_owned(),
            tone: Tone {
                kind,
                sound: StringSound::from(gscore_audio::default_string_model(kind)),
                chain: ChainSetting::default(),
            },
        };
        let library = Library {
            tones: vec![
                tone("Lead", StringKind::Electric),
                tone("Finger", StringKind::Bass),
            ],
            programs: BTreeMap::from([
                (30, "Lead".to_owned()),
                (33, "Finger".to_owned()),
                (34, "Gone".to_owned()),
            ]),
            rse: BTreeMap::from([
                ("Distortion Guitar/Lead".to_owned(), "Lead".to_owned()),
                ("Clean Bass/Rock".to_owned(), "Gone".to_owned()),
            ]),
        };
        let name = |program, kind| {
            library
                .for_program(program, kind)
                .map(|tone| tone.name.as_str())
        };
        assert_eq!(name(30, StringKind::Electric), Some("Lead"));
        assert_eq!(name(33, StringKind::Bass), Some("Finger"));
        // A track switched to another kind does not keep a tone of the old one.
        assert_eq!(name(30, StringKind::Bass), None);
        // A program without a tone, or naming one that is gone, has none: it plays its
        // plain strings.
        assert_eq!(name(27, StringKind::Electric), None);
        assert_eq!(name(34, StringKind::Bass), None);
        // A Guitar Pro sound the library maps names its tone; an unmapped one, or one
        // mapped to a tone that is gone, has none.
        let rse = |key| library.for_rse(key).map(|tone| tone.name.as_str());
        assert_eq!(rse("Distortion Guitar/Lead"), Some("Lead"));
        assert_eq!(rse("Distortion Guitar/Fuzz"), None);
        assert_eq!(rse("Clean Bass/Rock"), None);
    }
}
