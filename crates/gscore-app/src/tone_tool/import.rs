//! `tones import`: copying one saved tone into the built-in library.

use crate::chain_settings::{BlockChoice, BlockSetting, Tone};
use crate::settings::kind_from_id;
use crate::tone_library::{ToneEntry, ToneSource};

use super::*;

pub(super) fn parse_import(args: &[String]) -> Result<ImportArgs, String> {
    let mut import = ImportArgs::default();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--settings" => {
                import.options.settings = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--file" => {
                import.options.file = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--preset" => import.preset = Some(next_value(&mut rest, argument)?),
            "--program" => import.program = Some(parse_program(&next_value(&mut rest, argument)?)?),
            "--name" => import.name = Some(next_value(&mut rest, argument)?),
            "--family" => {
                let text = next_value(&mut rest, argument)?;
                import.family = Some(
                    ToneFamily::from_id(&text)
                        .ok_or_else(|| format!("`{text}` is not a family; try one of clean, crunch, lead, high_gain, acoustic, bass"))?,
                );
            }
            "--rse" => import.rse.push(next_value(&mut rest, argument)?),
            "--description" => import.description = Some(next_value(&mut rest, argument)?),
            "--ir" => {
                let text = next_value(&mut rest, argument)?;
                match text.as_str() {
                    "drop" => import.drop_irs = true,
                    "keep" => import.drop_irs = false,
                    other => {
                        return Err(format!(
                            "`--ir {other}` is not understood; use `keep` or `drop`"
                        ));
                    }
                }
            }
            "--dry-run" => import.dry_run = true,
            other => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
        }
    }
    Ok(import)
}

pub(super) fn run_import(args: ImportArgs) -> Result<(), String> {
    let preset_name = args
        .preset
        .as_deref()
        .ok_or_else(|| format!("`import` needs `--preset NAME`\n\n{USAGE}"))?;
    let program = args
        .program
        .ok_or_else(|| format!("`import` needs `--program N` (24-39)\n\n{USAGE}"))?;
    if !(24..=39).contains(&program) {
        return Err(format!(
            "program {program} is not a guitar or bass program (24-39)"
        ));
    }
    let settings_file = settings_path(args.options.settings.as_deref())?;
    let settings = load_settings(&settings_file)?;
    let preset = find_preset(&settings, preset_name)?;
    let kind = kind_from_id(&preset.kind).ok_or_else(|| {
        format!(
            "\"{}\" has an unknown kind \"{}\"",
            preset.name, preset.kind
        )
    })?;
    let mut tone = Tone {
        kind,
        sound: preset.sound.for_kind(kind),
        chain: preset.chain.clone(),
    };
    let copied_level = tone.sound.level;
    let trim = chain_trim(&tone.chain);

    // Impulse responses: a built-in tone may keep a path for now (the owner is still
    // collecting them), but a machine without the file skips the block, so say so.
    let files: Vec<String> = tone
        .chain
        .blocks
        .iter()
        .filter_map(|block| {
            block
                .file
                .as_ref()
                .map(|file| format!("{} ({})", file.display(), block.choice.label()))
        })
        .collect();
    let dropped = if args.drop_irs {
        drop_impulse_responses(&mut tone)
    } else {
        Vec::new()
    };
    let kept = if args.drop_irs { Vec::new() } else { files };

    let name = args.name.unwrap_or_else(|| preset.name.clone());
    let family = args.family.unwrap_or_else(|| default_family(program, kind));
    let description = args
        .description
        .unwrap_or_else(|| format!("The {name} tone."));
    let entry = ToneEntry {
        name: name.clone(),
        family,
        description,
        kind: crate::settings::kind_id(kind).to_owned(),
        sound: tone.sound,
        chain: tone.chain,
        source: Some(ToneSource {
            preset: preset.name.clone(),
            digest: preset_digest(preset),
            drop_irs: args.drop_irs,
        }),
    };

    let path = tones_path(args.options.file.as_deref());
    let mut file = load_library(&path)?;
    if let Some(slot) = file.tones.iter_mut().find(|tone| tone.name == entry.name) {
        *slot = entry.clone();
    } else {
        file.tones.push(entry.clone());
    }
    file.programs.insert(program, entry.name.clone());
    for sound in &args.rse {
        file.rse.insert(sound.clone(), entry.name.clone());
    }
    validate(&file)?;

    println!(
        "Imported \"{}\" as \"{}\" for program {program} ({})",
        preset.name,
        entry.name,
        gscore_model::program_name(program)
    );
    println!(
        "  strings: {} at level {copied_level:.3}, chain trims {trim:.1} dB",
        entry.kind
    );
    for file in &kept {
        println!(
            "  warning: keeps {file}; the file is not bundled, so a machine without it \
             skips that block"
        );
    }
    for file in &dropped {
        println!("  dropped: {file}");
    }
    for sound in &args.rse {
        println!("  tracks made with the RSE sound \"{sound}\" start on it too");
    }
    println!("  run `cargo xtask tones normalize` to level it with the others");
    if let Some(expected) = StringKind::for_program(program)
        && expected != kind
    {
        println!(
            "  warning: program {program} is for {} strings, not {}; the app falls back to \
             that kind's default instead",
            kind_name(expected),
            entry.kind
        );
    }

    if args.dry_run {
        println!("dry run: {} not written", path.display());
        return Ok(());
    }
    write_library(&path, &file)?;
    println!("wrote {}", path.display());
    Ok(())
}

/// A fingerprint of a saved tone: its kind, strings and chain. An imported tone keeps the
/// one it was copied at, so a later change to the saved tone shows (FNV-1a over the JSON
/// text, which is the same on every machine).
pub(super) fn preset_digest(preset: &ChainPreset) -> String {
    let text =
        serde_json::to_string(&(&preset.kind, &preset.sound, &preset.chain)).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// How an imported tone's saved tone stands in the settings now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceState {
    /// The saved tone is as it was when it was copied.
    Current,
    /// The saved tone has changed since: `update` copies it again.
    Changed,
    /// The settings have no saved tone of that name any more.
    Missing,
}

pub(super) fn source_state(source: &ToneSource, settings: &Settings) -> SourceState {
    match settings
        .string_presets
        .iter()
        .find(|preset| preset.name == source.preset)
    {
        None => SourceState::Missing,
        Some(preset) if preset_digest(preset) == source.digest => SourceState::Current,
        Some(_) => SourceState::Changed,
    }
}

pub(super) fn drop_impulse_responses(tone: &mut Tone) -> Vec<String> {
    let mut dropped = Vec::new();
    for index in 0..tone.chain.blocks.len() {
        let (choice, file) = match tone.chain.blocks.get(index) {
            Some(block) => (
                block.choice,
                block.file.as_ref().map(|file| file.display().to_string()),
            ),
            None => continue,
        };
        if !matches!(choice, BlockChoice::Cabinet | BlockChoice::Room) {
            continue;
        }
        let Some(file) = file else {
            continue;
        };
        if let Some(slot) = tone.chain.blocks.get_mut(index) {
            if choice == BlockChoice::Cabinet {
                // The Cab block stays and plays its built-in cabinet.
                slot.file = None;
            } else {
                *slot = BlockSetting::of(BlockChoice::Empty);
            }
        }
        dropped.push(format!("{} {file}", choice.label()));
    }
    dropped
}
