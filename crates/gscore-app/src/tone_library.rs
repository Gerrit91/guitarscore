//! GuitarScore's built-in tones for physically modelled strings: a small library of named
//! sounds, grouped by family, that a PM track can play instead of just the General MIDI
//! program. A tone is the whole [`Tone`] - the kind of string, its sound and the chain
//! after it - plus a name, a family and a short description; nothing here is Guitar Pro's
//! RSE material, and the names describe a style or a sound, never a person.
//!
//! Every General MIDI program has a default tone from this library, so a track starts on a
//! fitting sound instead of a generic one ([`Self::for_program`]). The tones and the program
//! map live in `data/tones.json`, embedded at build time, so `cargo xtask tones` can add a
//! saved tone from the owner's settings without touching Rust code. The built-in tones use
//! computed blocks; for now a tone may also point at an impulse response file, which is not
//! bundled (the owner is still collecting them), and the chain skips a block whose file is
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
use crate::settings::{ChainPreset, PickupSetting, StringSound, kind_from_id};

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
}

/// The built-in tones as the program ships them, embedded at build time.
pub(crate) const TONES_JSON: &str = include_str!("../data/tones.json");

/// The embedded library, parsed once. A malformed file leaves the library empty and every
/// program on the plain fallback tone; a test keeps that from happening.
static LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let file: ToneLibraryFile = match serde_json::from_str(TONES_JSON) {
        Ok(file) => file,
        Err(_) => return Library::default(),
    };
    Library {
        tones: file.tones.iter().filter_map(ToneEntry::built_in).collect(),
        programs: file.programs,
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
    // Every bass program plays the owner's saved bass preset (there is one bass sound in
    // the presets): the finger, pick and slap programs all start on it.
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

/// The sum of the blocks' output levels (an amp, a drive, a compressor, a cabinet, a
/// room): what makes one tone louder than another with the same strings. Drive and tone
/// knobs are never part of it.
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
pub(crate) fn normalize_static(tone: &mut Tone, target_level: f32) {
    normalize_string_level(tone, target_level);
    let trim = chain_level_trim(&tone.chain);
    if trim.is_finite() {
        let (low, high) = gscore_guitar::knob_ranges::OUTPUT;
        tone.chain.output_db = (-trim).clamp(low, high);
    }
}

/// A plain electric tone used only if the embedded library cannot be read.
fn fallback_tone() -> &'static BuiltInTone {
    static FALLBACK: LazyLock<BuiltInTone> = LazyLock::new(|| BuiltInTone {
        name: "Clean".to_owned(),
        family: ToneFamily::Clean,
        description: "The plain modelled electric guitar.".to_owned(),
        tone: Tone {
            kind: StringKind::Electric,
            sound: sound(StringKind::Electric, PickupSetting::Unset, 0.0),
            chain: ChainSetting::default(),
        },
    });
    &FALLBACK
}

/// The program's documented tone, before the kind is checked.
fn program_tone(program: u8) -> Option<&'static BuiltInTone> {
    LIBRARY.programs.get(&program).and_then(|name| find(name))
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
    fn every_family_has_tones_with_unique_names() {
        let mut names: Vec<String> = Vec::new();
        for family in ToneFamily::ALL {
            let count = all().iter().filter(|tone| tone.family == family).count();
            assert!(count > 0, "{} has no tone", family.label());
        }
        for tone in all() {
            assert!(!names.contains(&tone.name), "{} appears twice", tone.name);
            names.push(tone.name.clone());
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
            // Every knob is in range and there is a real chain. An impulse response path is
            // allowed for now (the owner is still collecting the files; the chain skips a
            // block whose file is missing).
            for block in &tone.tone.chain.blocks {
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
        // Every bass program plays the one saved bass preset.
        for program in 32..=39 {
            assert_eq!(
                name(program, StringKind::Bass),
                Some("Clean Bass 1"),
                "bass program {program}"
            );
        }
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
    fn the_embedded_file_parses_and_every_program_names_a_tone() {
        let file: ToneLibraryFile = serde_json::from_str(TONES_JSON).unwrap();
        assert!(!all().is_empty());
        assert_eq!(file.tones.len(), all().len(), "a tone was dropped");
        assert!(!LIBRARY.programs.is_empty());
        for (program, name) in &LIBRARY.programs {
            let tone = find(name).unwrap_or_else(|| panic!("program {program} names {name}"));
            assert_eq!(&tone.name, name, "program {program}");
        }
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
