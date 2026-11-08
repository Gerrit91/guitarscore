//! `tones import`: copying one saved tone into the built-in library.

use crate::chain_settings::{BlockChoice, BlockSetting, Tone};
use crate::settings::kind_from_id;
use crate::tone_library::ToneEntry;

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
    };

    let path = tones_path(args.options.file.as_deref());
    let mut file = load_library(&path)?;
    if let Some(slot) = file.tones.iter_mut().find(|tone| tone.name == entry.name) {
        *slot = entry.clone();
    } else {
        file.tones.push(entry.clone());
    }
    file.programs.insert(program, entry.name.clone());
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
    if dropped
        .iter()
        .any(|dropped| dropped.starts_with(BlockChoice::Cabinet.label()))
        && !tone_has_amp(&entry.chain)
    {
        println!(
            "  warning: the dropped cabinet/body had no amp to take its place; the tone \
             has no body of its own now"
        );
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

pub(super) fn drop_impulse_responses(tone: &mut Tone) -> Vec<String> {
    let mut dropped = Vec::new();
    let mut last_amp: Option<usize> = None;
    for index in 0..tone.chain.blocks.len() {
        let (choice, file) = match tone.chain.blocks.get(index) {
            Some(block) => (
                block.choice,
                block.file.as_ref().map(|file| file.display().to_string()),
            ),
            None => continue,
        };
        if choice == BlockChoice::Amp {
            last_amp = Some(index);
            continue;
        }
        if !matches!(choice, BlockChoice::Cabinet | BlockChoice::Room) {
            continue;
        }
        let Some(file) = file else {
            continue;
        };
        if choice == BlockChoice::Cabinet
            && let Some(amp) = last_amp.and_then(|amp| tone.chain.blocks.get_mut(amp))
        {
            // The IR followed the amp: the computed speaker takes its place.
            amp.speaker = 1.0;
        }
        if let Some(slot) = tone.chain.blocks.get_mut(index) {
            *slot = BlockSetting::of(BlockChoice::Empty);
        }
        dropped.push(format!("{} {file}", choice.label()));
    }
    dropped
}
