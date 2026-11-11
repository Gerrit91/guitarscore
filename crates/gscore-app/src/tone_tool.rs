//! `cargo xtask tones`: importing a saved tone from the settings into the built-in tone
//! library (`data/tones.json`), levelling it, and checking it. The library is data,
//! embedded with `include_str!`, so this tool edits it without touching Rust code; nothing
//! here runs in the application.
//!
//! ```text
//! cargo xtask tones list
//! cargo xtask tones import --preset "Metal Guitar 1" --program 30 --name "Modern Metal"
//! cargo xtask tones normalize
//! cargo xtask tones check
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gscore_play::StringKind;

use crate::chain_settings::{BlockChoice, ChainSetting};
use crate::settings::{ChainPreset, Settings};
use crate::tone_library::{self, ToneEntry, ToneFamily, ToneLibraryFile};

mod import;
mod measure;
mod normalize;
#[cfg(test)]
mod tests;

use import::{parse_import, run_import};
use normalize::{parse_normalize, run_normalize};

const USAGE: &str = "\
Usage: cargo xtask tones <command> [options]

Commands:
  list [--settings FILE]
      The saved tones in the settings file, with the programs each one currently stands
      in for (the app matches them to the built-in tones by name).
  import --preset NAME --program N [options]
      Copy the saved tone NAME into the built-in library exactly as dialed in (its string
      sound and its chain) and make program N (24-39) start on it. Nothing is levelled;
      run `normalize` afterwards for that.
      --name NAME          the shipped name (default: the preset's)
      --family FAMILY      clean, crunch, lead, high_gain, acoustic or bass
                           (default: the program's family)
      --description TEXT   the line shown on hover
      --ir drop|keep       `drop` replaces a preset's impulse responses (cabinet, room,
                           body) with the computed blocks; `keep` (the default) stores the
                           paths and warns, since the files are not bundled yet
      --dry-run            print what would change, write nothing
  normalize [--tone NAME] [--target DB] [--static] [--dry-run]
      Level every built-in tone with the others. By default each tone renders a fixed
      reference note through its own strings and chain and its output level is set so that
      note reaches `--target` (default -24 dBFS RMS); the level is the chain's output knob,
      so no amp or drive setting is touched. Prints the measured level and the output
      deviation of every tone. --static cancels the chains' level trims at the output knob
      instead (no rendering), --tone limits it to one tone, --dry-run writes nothing.
  check [--file FILE]
      Validate the embedded library (unique names, known kinds, every program naming a
      tone) and list the impulse responses it still points at.

Options common to every command:
  --settings FILE   the settings.json to read (default: the app's config file)
  --file FILE       the tone library to edit (default: crates/gscore-app/data/tones.json)";

/// The largest settings or library file the tool reads (far above a real one).
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// What every command may be given.
#[derive(Debug, Default)]
struct Options {
    /// The settings file to read; `None`: the app's own.
    settings: Option<PathBuf>,
    /// The tone library to edit; `None`: the embedded one.
    file: Option<PathBuf>,
}

/// What `import` was asked to do.
#[derive(Debug, Default)]
struct ImportArgs {
    options: Options,
    preset: Option<String>,
    program: Option<u8>,
    name: Option<String>,
    family: Option<ToneFamily>,
    description: Option<String>,
    /// Replace a preset's impulse responses with the computed blocks.
    drop_irs: bool,
    dry_run: bool,
}

/// The level one reference note should come out at, dBFS RMS.
const DEFAULT_TARGET_DB: f32 = -24.0;

/// What `normalize` was asked to do.
#[derive(Debug)]
struct NormalizeArgs {
    options: Options,
    /// Only this tone, by name.
    tone: Option<String>,
    /// The level the measured reference note should reach, dBFS RMS.
    target_db: f32,
    /// Level with the cheap estimate instead of rendering the reference note.
    static_levels: bool,
    dry_run: bool,
}

impl Default for NormalizeArgs {
    fn default() -> Self {
        NormalizeArgs {
            options: Options::default(),
            tone: None,
            target_db: DEFAULT_TARGET_DB,
            static_levels: false,
            dry_run: false,
        }
    }
}

/// Runs the tool with the arguments after `tones`.
pub fn run(args: &[String]) -> Result<(), String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => return Err(format!("no command given\n\n{USAGE}")),
    };
    match command {
        "list" => run_list(parse_options(rest)?),
        "import" => run_import(parse_import(rest)?),
        "normalize" => run_normalize(parse_normalize(rest)?),
        "check" => run_check(parse_options(rest)?),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            // `--list` and the like: the commands are plain words.
            let word = other.trim_start_matches('-');
            let hint = if ["list", "import", "normalize", "check"].contains(&word) {
                format!(" (did you mean `{word}`?)")
            } else {
                String::new()
            };
            Err(format!("unknown command `{other}`{hint}\n\n{USAGE}"))
        }
    }
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--settings" => {
                options.settings = Some(PathBuf::from(next_value(&mut rest, argument)?))
            }
            "--file" => options.file = Some(PathBuf::from(next_value(&mut rest, argument)?)),
            other => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
        }
    }
    Ok(options)
}

fn next_value(rest: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    rest.next()
        .cloned()
        .ok_or_else(|| format!("`{flag}` needs a value"))
}

fn parse_program(text: &str) -> Result<u8, String> {
    let program: u8 = text
        .parse()
        .map_err(|_| format!("`{text}` is not a General MIDI program (0-127)"))?;
    if program > 127 {
        return Err(format!("`{text}` is not a General MIDI program (0-127)"));
    }
    Ok(program)
}

fn settings_path(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    gscore_session::default_settings_path()
        .ok_or_else(|| "cannot find the settings directory; pass `--settings FILE`".to_owned())
}

fn tones_path(explicit: Option<&Path>) -> PathBuf {
    explicit.map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("data/tones.json"),
        Path::to_path_buf,
    )
}

fn read_text(path: &Path, max: u64) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if metadata.len() > max {
        return Err(format!(
            "{} is larger than {max} bytes; is it the right file?",
            path.display()
        ));
    }
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn load_settings(path: &Path) -> Result<Settings, String> {
    let text = read_text(path, MAX_FILE_BYTES)?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} is not a valid settings file: {error}", path.display()))
}

fn load_library(path: &Path) -> Result<ToneLibraryFile, String> {
    let text = read_text(path, MAX_FILE_BYTES)?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{} is not a valid tone library: {error}", path.display()))
}

fn run_list(options: Options) -> Result<(), String> {
    let path = settings_path(options.settings.as_deref())?;
    let settings = load_settings(&path)?;
    println!(
        "{} saved tone(s) in {}:",
        settings.string_presets.len(),
        path.display()
    );
    if settings.string_presets.is_empty() {
        println!("  (none)");
        return Ok(());
    }
    // Which programs each saved tone currently stands in for: the app matches them to the
    // built-in tones by name words (`tone_library::saved_default_for`).
    let mut local: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for program in 24..=39_u8 {
        let Some(kind) = StringKind::for_program(program) else {
            continue;
        };
        if let Some(preset) =
            tone_library::saved_default_for(program, kind, &settings.string_presets)
        {
            local.entry(preset.name.clone()).or_default().push(program);
        }
    }
    for preset in &settings.string_presets {
        let blocks: Vec<&str> = preset
            .chain
            .blocks
            .iter()
            .filter(|block| block.choice != BlockChoice::Empty)
            .map(|block| block.choice.label())
            .collect();
        let irs = preset
            .chain
            .blocks
            .iter()
            .filter(|block| block.file.is_some())
            .count();
        let mut line = format!(
            "  {:<30} {:<8} level {:>6.3}  {}",
            preset.name,
            preset.kind,
            preset.sound.level,
            if blocks.is_empty() {
                "no effects".to_owned()
            } else {
                blocks.join(", ")
            }
        );
        if irs > 0 {
            line.push_str(&format!("  [{irs} impulse response(s)]"));
        }
        println!("{line}");
        if let Some(programs) = local.get(&preset.name) {
            let names: Vec<String> = programs
                .iter()
                .map(|&program| format!("{program} ({})", gscore_model::program_name(program)))
                .collect();
            println!("      stands in for {}", names.join(", "));
        }
    }
    Ok(())
}

fn run_check(options: Options) -> Result<(), String> {
    let path = tones_path(options.file.as_deref());
    let file = load_library(&path)?;
    validate(&file)?;
    println!(
        "{}: {} tone(s), {} program(s)",
        path.display(),
        file.tones.len(),
        file.programs.len()
    );
    for warning in ir_warnings(&file) {
        println!("  warning: {warning}");
    }
    for entry in &file.tones {
        let Some(tone) = entry.built_in() else {
            continue;
        };
        let blocks: Vec<&str> = tone
            .tone
            .chain
            .blocks
            .iter()
            .filter(|block| block.choice != BlockChoice::Empty)
            .map(|block| block.choice.label())
            .collect();
        println!(
            "  {:<30} {:<8} {:<9} {}",
            entry.name,
            entry.kind,
            entry.family.id(),
            if blocks.is_empty() {
                "no effects".to_owned()
            } else {
                blocks.join(", ")
            }
        );
    }
    Ok(())
}

fn find_preset<'a>(settings: &'a Settings, name: &str) -> Result<&'a ChainPreset, String> {
    if let Some(preset) = settings
        .string_presets
        .iter()
        .find(|preset| preset.name == name)
    {
        return Ok(preset);
    }
    let names: Vec<&str> = settings
        .string_presets
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    Err(format!(
        "there is no saved tone named \"{name}\". Saved tones: {}",
        if names.is_empty() {
            "(none)".to_owned()
        } else {
            names.join(", ")
        }
    ))
}

/// Replaces every impulse response with the computed blocks: a cabinet after an amp turns
/// the amp's computed speaker back on; a room or body block simply goes. Returns what it
/// dropped, for the report.
fn chain_trim(chain: &ChainSetting) -> f32 {
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
    // Adding zero turns a negative zero into a positive one, so the report prints "0.0".
    if sum == 0.0 { 0.0 } else { sum }
}

fn tone_has_amp(chain: &ChainSetting) -> bool {
    chain
        .blocks
        .iter()
        .any(|block| block.choice == BlockChoice::Amp)
}

/// The family a program's tone belongs to, as the pickers group them.
fn default_family(program: u8, kind: StringKind) -> ToneFamily {
    match program {
        24 | 25 => ToneFamily::Acoustic,
        26..=28 => ToneFamily::Clean,
        29 => ToneFamily::Crunch,
        30 => ToneFamily::HighGain,
        31 => ToneFamily::Lead,
        32..=39 => ToneFamily::Bass,
        _ => match kind {
            StringKind::Nylon | StringKind::Steel => ToneFamily::Acoustic,
            StringKind::Bass => ToneFamily::Bass,
            StringKind::Electric => ToneFamily::Clean,
        },
    }
}

fn kind_name(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Nylon => "nylon",
        StringKind::Steel => "steel",
        StringKind::Electric => "electric",
        StringKind::Bass => "bass",
    }
}

/// Checks the library before it is written or embedded: the same invariants the tone
/// library's tests hold it to.
fn validate(file: &ToneLibraryFile) -> Result<(), String> {
    if file.tones.is_empty() {
        return Err("the library has no tones".to_owned());
    }
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for entry in &file.tones {
        if entry.name.trim().is_empty() {
            return Err("a tone has an empty name".to_owned());
        }
        if !names.insert(entry.name.as_str()) {
            return Err(format!("two tones are named \"{}\"", entry.name));
        }
        if entry.description.trim().is_empty() {
            return Err(format!("\"{}\" has no description", entry.name));
        }
        if entry.built_in().is_none() {
            return Err(format!(
                "\"{}\" has an unknown kind \"{}\"",
                entry.name, entry.kind
            ));
        }
        // An impulse response path is allowed for now (the owner is still collecting the
        // files); `ir_warnings` reports them and the chain skips a missing one.
    }
    for (program, name) in &file.programs {
        if !names.contains(name.as_str()) {
            return Err(format!(
                "program {program} starts on \"{name}\", which is not in the library"
            ));
        }
    }
    Ok(())
}

/// The impulse responses the library still points at. They are allowed for now (the owner
/// is still collecting the files); a machine without one skips that block.
fn ir_warnings(file: &ToneLibraryFile) -> Vec<String> {
    let mut warnings = Vec::new();
    for entry in &file.tones {
        for block in &entry.chain.blocks {
            if let Some(path) = &block.file {
                warnings.push(format!(
                    "\"{}\" loads {} ({})",
                    entry.name,
                    path.display(),
                    block.choice.label()
                ));
            }
        }
    }
    warnings
}

/// Writes the library atomically: a temporary file in the same directory, then a rename.
fn write_library(path: &Path, file: &ToneLibraryFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(file)
        .map_err(|error| format!("cannot encode the library: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, format!("{text}\n"))
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot replace {}: {error}", path.display()));
    }
    Ok(())
}
