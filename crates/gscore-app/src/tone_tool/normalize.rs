//! `tones normalize`: leveling every built-in tone with the others.
//!
//! By default a tone is measured: its own strings and chain render a fixed reference note
//! (`measure`), and its output level is set so that note comes out at the target. `--static`
//! keeps the old cheap estimate (the string level and the chain's level trims cancelled at
//! the output knob) without rendering.

use std::collections::HashMap;

use crate::chain_settings::Tone;
use crate::tone_library;

use super::*;

pub(super) fn parse_normalize(args: &[String]) -> Result<NormalizeArgs, String> {
    let mut normalize = NormalizeArgs::default();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--settings" => {
                normalize.options.settings = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--file" => {
                normalize.options.file = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--tone" => normalize.tone = Some(next_value(&mut rest, argument)?),
            "--target" => normalize.target_db = parse_db(&next_value(&mut rest, argument)?)?,
            "--static" => normalize.static_levels = true,
            "--dry-run" => normalize.dry_run = true,
            other => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
        }
    }
    Ok(normalize)
}

fn parse_db(text: &str) -> Result<f32, String> {
    let db: f32 = text
        .parse()
        .map_err(|_| format!("`{text}` is not a level in dB"))?;
    if !db.is_finite() || !(-60.0..=0.0).contains(&db) {
        return Err(format!("`{text}` is not a target level from -60 to 0 dBFS"));
    }
    Ok(db)
}

pub(super) fn run_normalize(args: NormalizeArgs) -> Result<(), String> {
    let settings_file = settings_path(args.options.settings.as_deref())?;
    // A fresh machine has no settings file yet: the defaults still give every kind a level.
    let (settings, source) = if args.options.settings.is_none() && !settings_file.exists() {
        (
            Settings::default(),
            "the default levels (no settings file yet)".to_owned(),
        )
    } else {
        (
            load_settings(&settings_file)?,
            settings_file.display().to_string(),
        )
    };
    let path = tones_path(args.options.file.as_deref());
    let mut file = load_library(&path)?;
    validate(&file)?;
    let targets = &settings.string_sounds;
    let target_db = if args.static_levels {
        None
    } else {
        Some(args.target_db)
    };
    println!(
        "target levels from {source}: nylon {:.3}, steel {:.3}, electric {:.3}, bass {:.3}",
        targets.get(StringKind::Nylon).level,
        targets.get(StringKind::Steel).level,
        targets.get(StringKind::Electric).level,
        targets.get(StringKind::Bass).level
    );
    if let Some(target_db) = target_db {
        println!("reference note target: {target_db:.1} dBFS RMS (measured)");
    } else {
        println!("static leveling: no reference note, the trims are cancelled (--static)");
    }
    let mut impulses = HashMap::new();
    let mut changed = 0_usize;
    let mut matched = 0_usize;
    for entry in &mut file.tones {
        if let Some(only) = &args.tone
            && entry.name != *only
        {
            continue;
        }
        matched = matched.saturating_add(1);
        let Some(mut tone) = entry.built_in().map(|built| built.tone) else {
            continue;
        };
        let target_level = targets.get(tone.kind).level;
        let level_before = tone.sound.level;
        let output_before = tone.chain.output_db;
        let trimmed = tone_library::chain_level_trim(&tone.chain);
        match target_db {
            Some(target_db) => {
                tone_library::normalize_string_level(&mut tone, target_level);
                // Measure the chain without the output knob, so the result does not depend
                // on an earlier run.
                tone.chain.output_db = 0.0;
                let measurement = measure::measure(&tone, &mut impulses)?;
                let (low, high) = gscore_guitar::knob_ranges::OUTPUT;
                let silent = measurement.rms_db < measure::SILENT_DB;
                // A tone that makes no sound has no level to match: its output stays.
                tone.chain.output_db = if silent {
                    output_before
                } else {
                    (target_db - measurement.rms_db).clamp(low, high)
                };
                let mut note = if silent {
                    "  warning: the reference note was silent; the output level is left as it is"
                        .to_owned()
                } else if measurement.short {
                    "  note: a short tone, measured over its first 0.2 s".to_owned()
                } else {
                    String::new()
                };
                for skipped in &measurement.skipped {
                    note.push_str(&format!("  warning: {skipped}"));
                }
                print_row(
                    entry,
                    level_before,
                    &tone,
                    output_before,
                    trimmed,
                    Some((target_db, measurement.rms_db)),
                    note,
                );
            }
            None => {
                tone_library::normalize_static(&mut tone, target_level);
                print_row(
                    entry,
                    level_before,
                    &tone,
                    output_before,
                    trimmed,
                    None,
                    String::new(),
                );
            }
        }
        let is_changed = (tone.sound.level - level_before).abs() > 1e-4
            || (tone.chain.output_db - output_before).abs() > 1e-4;
        if is_changed {
            changed = changed.saturating_add(1);
        }
        entry.sound = tone.sound;
        entry.chain = tone.chain;
    }
    if matched == 0 {
        return Err(match &args.tone {
            Some(name) => format!("there is no tone named \"{name}\""),
            None => "the library has no tones".to_owned(),
        });
    }
    if args.dry_run {
        println!(
            "dry run: {changed} of {matched} tone(s) would change, {} not written",
            path.display()
        );
        return Ok(());
    }
    write_library(&path, &file)?;
    println!(
        "wrote {} ({changed} of {matched} tone(s) changed)",
        path.display()
    );
    Ok(())
}

/// One tone's row: its level, what was measured, and the output level it ends at.
fn print_row(
    entry: &ToneEntry,
    level_before: f32,
    tone: &Tone,
    output_before: f32,
    trimmed: f32,
    measured: Option<(f32, f32)>,
    note: String,
) {
    let change = (tone.chain.output_db - output_before).abs() > 1e-4
        || (tone.sound.level - level_before).abs() > 1e-4;
    let measured_text = match measured {
        Some((target, rms_db)) => format!("  measured {rms_db:>7.1} dBFS (target {target:.1})"),
        None => format!("  level trims {trimmed:+.1} dB"),
    };
    println!(
        "  {:<30} {:<8} level {level_before:.3} -> {:.3}  output {output_before:+.1} -> {:+.1} dB{measured_text}{}{}",
        entry.name,
        entry.kind,
        tone.sound.level,
        tone.chain.output_db,
        if change { "  changed" } else { "  unchanged" },
        note
    );
}
