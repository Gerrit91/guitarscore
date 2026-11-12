//! `tones update`: copying the saved tones of the imported built-in tones again, where
//! they have changed since the import.

use crate::chain_settings::Tone;
use crate::settings::{kind_from_id, kind_id};

use super::import::{drop_impulse_responses, preset_digest};
use super::*;

pub(super) fn parse_update(args: &[String]) -> Result<UpdateArgs, String> {
    let mut update = UpdateArgs::default();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--settings" => {
                update.options.settings = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--file" => {
                update.options.file = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--dry-run" => update.dry_run = true,
            other => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
        }
    }
    Ok(update)
}

pub(super) fn run_update(args: UpdateArgs) -> Result<(), String> {
    let settings_file = settings_path(args.options.settings.as_deref())?;
    let settings = load_settings(&settings_file)?;
    let path = tones_path(args.options.file.as_deref());
    let mut file = load_library(&path)?;

    let mut imported = 0_usize;
    let mut updated = 0_usize;
    for entry in &mut file.tones {
        let Some(source) = entry.source.as_mut() else {
            continue;
        };
        imported += 1;
        let preset = match source_state(source, &settings) {
            SourceState::Current => {
                println!("  {:<30} up to date with \"{}\"", entry.name, source.preset);
                continue;
            }
            SourceState::Missing => {
                println!(
                    "  {:<30} left as it is: \"{}\" is no longer in the settings",
                    entry.name, source.preset
                );
                continue;
            }
            SourceState::Changed => find_preset(&settings, &source.preset)?,
        };
        let Some(kind) = kind_from_id(&preset.kind) else {
            println!(
                "  {:<30} left as it is: \"{}\" has an unknown kind \"{}\"",
                entry.name, preset.name, preset.kind
            );
            continue;
        };
        let mut tone = Tone {
            kind,
            sound: preset.sound.for_kind(kind),
            chain: preset.chain.clone(),
        };
        if source.drop_irs {
            drop_impulse_responses(&mut tone);
        }
        println!("  {:<30} updated from \"{}\"", entry.name, preset.name);
        if entry.kind != kind_id(kind) {
            println!(
                "    warning: it is for {} strings now, not {}",
                kind_id(kind),
                entry.kind
            );
        }
        entry.kind = kind_id(kind).to_owned();
        entry.sound = tone.sound;
        entry.chain = tone.chain;
        source.digest = preset_digest(preset);
        updated += 1;
    }
    if imported == 0 {
        println!("no built-in tone remembers a saved tone yet; `import` it (again) to link them");
        return Ok(());
    }
    if updated == 0 {
        println!("nothing to update");
        return Ok(());
    }
    validate(&file)?;
    println!("  run `cargo xtask tones normalize` to level them with the others");
    if args.dry_run {
        println!("dry run: {} not written", path.display());
        return Ok(());
    }
    write_library(&path, &file)?;
    println!("wrote {} ({updated} tone(s) updated)", path.display());
    Ok(())
}
