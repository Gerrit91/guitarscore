//! `cargo xtask tones`: importing a saved tone from the settings into the built-in tone
//! library (`data/tones.json`), levelling it, and checking it. The library is data,
//! embedded with `include_str!`, so this tool edits it without touching Rust code; nothing
//! here runs in the application.
//!
//! ```text
//! cargo xtask tones list
//! cargo xtask tones programs
//! cargo xtask tones import --preset "Metal Guitar 1" --program 30
//! cargo xtask tones update
//! cargo xtask tones rse ~/tabs
//! cargo xtask tones rse --map "Distortion Guitar/Hard Rock" --tone "Modern Metal Rhythm"
//! cargo xtask tones normalize
//! cargo xtask tones check
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gscore_play::StringKind;

use crate::chain_settings::{BlockChoice, ChainSetting};
use crate::settings::{ChainPreset, Settings};
use crate::tone_library::{ToneEntry, ToneLibraryFile};

mod import;
mod measure;
mod normalize;
mod rse;
#[cfg(test)]
mod tests;
mod update;

use import::{SourceState, parse_import, run_import, source_state};
use normalize::{parse_normalize, run_normalize};
use rse::{parse_rse, run_rse};
use update::{parse_update, run_update};

const USAGE: &str = "\
Usage: cargo xtask tones <command> [options]

Commands:
  list [--settings FILE] [--file FILE]
      The saved tones in the settings file, with the built-in tones imported from each
      one and the programs that start on them.
  programs [--settings FILE] [--file FILE]
      The guitar and bass programs (24-39) with their General MIDI names, the kind of
      string each plays and the built-in tone it starts on; `(none)` plays the plain
      strings. `[saved: NAME]` is the saved tone the built-in one was imported from; it
      says so when that saved tone has changed since (run `update`) or is gone.
  import --preset NAME --program N [options]
      Copy the saved tone NAME exactly as dialed in (its string sound and its chain) into
      program N's (24-39) built-in tone. The tone keeps its fixed name, family and
      description (`programs` lists them; they are changed in code, never by an import)
      and remembers the saved tone it came from. Every program's tone stands on its own:
      importing one saved tone for two programs makes two tones. The saved tone must be
      of the kind of string the program plays. Nothing is levelled; run `normalize`
      afterwards for that.
      --rse SOUND          also start tracks made with Guitar Pro's RSE sound SOUND on
                           it (see `rse`; may be given more than once)
      --ir drop|keep       `drop` takes a preset's impulse responses out: a Cab block
                           plays its built-in cabinet, a room block goes; `keep` (the default) stores the
                           paths and warns, since the files are not bundled yet
      --dry-run            print what would change, write nothing
  update [--dry-run]
      Copy every imported tone's saved tone again where it has changed since the import
      (`programs` shows which). The built-in name, family and description stay; a tone
      whose saved tone is gone is left as it is. Run `normalize` afterwards.
  rse [PATH...]
      Guitar Pro's RSE sounds as placeholders for the built-in tones. Without a PATH: the
      sounds mapped to a tone. With Guitar Pro files or folders: every RSE sound their
      guitar and bass tracks use (`Distortion Guitar/Hard Rock`: the instrument and the
      sound's name), how many tracks use it, the kind of string and the tone it starts
      on. A track made with a mapped sound starts on that tone instead of its program's;
      a tone the song carries still wins.
  rse --map SOUND --tone NAME [--dry-run]
      Start tracks made with the RSE sound SOUND on the built-in tone NAME (`--map` may
      be given more than once). The tone must be of the kind of string the track plays.
  rse --unmap SOUND [--dry-run]
      Forget the tone of the RSE sound SOUND.
  normalize [--tone NAME] [--target DB] [--static] [--dry-run]
      Level every built-in tone with the others. By default each tone renders a fixed
      reference note through its own strings and chain and its output level is set so that
      note reaches `--target` (default -24 dBFS RMS); the level is the chain's output knob,
      so no amp or drive setting is touched. Prints the measured level and the output
      deviation of every tone. --static cancels the chains' level trims at the output knob
      instead (no rendering), --tone limits it to one tone, --dry-run writes nothing.
  check [--file FILE]
      Validate the embedded library (every tone under its program's fixed name, family
      and description, known kinds) and list the impulse responses it still points at.

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
    /// Replace a preset's impulse responses with the computed blocks.
    drop_irs: bool,
    /// Guitar Pro's RSE sounds that start on the imported tone too.
    rse: Vec<String>,
    dry_run: bool,
}

/// What `update` was asked to do.
#[derive(Debug, Default)]
struct UpdateArgs {
    options: Options,
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
        "programs" => run_programs(parse_options(rest)?),
        "import" => run_import(parse_import(rest)?),
        "update" => run_update(parse_update(rest)?),
        "rse" => run_rse(parse_rse(rest)?),
        "normalize" => run_normalize(parse_normalize(rest)?),
        "check" => run_check(parse_options(rest)?),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            // `--list` and the like: the commands are plain words.
            let word = other.trim_start_matches('-');
            let hint = if [
                "list",
                "programs",
                "import",
                "update",
                "rse",
                "normalize",
                "check",
            ]
            .contains(&word)
            {
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
    // The built-in tones imported from each saved tone, with the programs starting on
    // them (the library is optional here: without it the saved tones still list).
    let library_path = tones_path(options.file.as_deref());
    let library = if library_path.exists() {
        load_library(&library_path)?
    } else {
        ToneLibraryFile::default()
    };
    let mut imported: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for tone in &library.tones {
        let Some(source) = &tone.source else {
            continue;
        };
        let programs: Vec<String> = library
            .programs
            .iter()
            .filter(|(_, name)| **name == tone.name)
            .map(|(&program, _)| format!("{program} ({})", gscore_model::program_name(program)))
            .collect();
        imported
            .entry(source.preset.as_str())
            .or_default()
            .push(format!(
                "\"{}\"{}{}",
                tone.name,
                if programs.is_empty() {
                    ", on no program".to_owned()
                } else {
                    format!(" for {}", programs.join(", "))
                },
                match source_state(source, &settings) {
                    SourceState::Changed => "; changed since: run `tones update`",
                    SourceState::Current | SourceState::Missing => "",
                }
            ));
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
        for tone in imported.get(preset.name.as_str()).into_iter().flatten() {
            println!("      imported as {tone}");
        }
    }
    Ok(())
}

fn run_programs(options: Options) -> Result<(), String> {
    let library_path = tones_path(options.file.as_deref());
    let library = load_library(&library_path)?;
    // The settings are optional here: without them the built-in tones still list.
    let settings_file = settings_path(options.settings.as_deref())?;
    let settings = if settings_file.exists() {
        Some(load_settings(&settings_file)?)
    } else {
        None
    };
    println!(
        "Guitar and bass programs, built-in tones from {}{}:",
        library_path.display(),
        match &settings {
            Some(_) => format!(", this machine's from {}", settings_file.display()),
            None => String::new(),
        }
    );
    for line in program_lines(&library, settings.as_ref()) {
        println!("{line}");
    }
    Ok(())
}

/// One line per guitar and bass program (24-39): its number and General MIDI name, the kind
/// of string it plays and the built-in tone it starts on; then the saved tone that one was
/// imported from (with `settings`: whether it has changed since or is gone).
fn program_lines(library: &ToneLibraryFile, settings: Option<&Settings>) -> Vec<String> {
    (24..=39_u8)
        .map(|program| {
            let kind = StringKind::for_program(program).map_or("-", kind_name);
            let built_in = library
                .programs
                .get(&program)
                .map_or("(none)", String::as_str);
            let mut line = format!(
                "  {program:>3}  {:<20} {kind:<8} {built_in:<20}",
                gscore_model::program_name(program)
            );
            let source = library
                .tones
                .iter()
                .find(|tone| tone.name == built_in)
                .and_then(|tone| tone.source.as_ref());
            if let Some(source) = source {
                let state = settings.map(|settings| source_state(source, settings));
                line.push_str(&format!(
                    " [saved: {}{}]",
                    source.preset,
                    match state {
                        Some(SourceState::Changed) => ", changed: run `tones update`",
                        Some(SourceState::Missing) => ", no longer in the settings",
                        Some(SourceState::Current) | None => "",
                    }
                ));
            }
            line.trim_end().to_owned()
        })
        .collect()
}

fn run_check(options: Options) -> Result<(), String> {
    let path = tones_path(options.file.as_deref());
    let file = load_library(&path)?;
    validate(&file)?;
    println!(
        "{}: {} tone(s), {} program(s), {} RSE sound(s)",
        path.display(),
        file.tones.len(),
        file.programs.len(),
        file.rse.len()
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

/// The level the chain's blocks add in all, dB.
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
        // A program starts on its own tone, under its fixed name.
        let fixed = crate::tone_library::program_tone(*program)
            .ok_or_else(|| format!("program {program} is no guitar or bass program"))?;
        if name != fixed.name {
            return Err(format!(
                "program {program} must start on \"{}\", not \"{name}\"",
                fixed.name
            ));
        }
    }
    // Every tone is a program's, with that program's fixed family and description.
    for entry in &file.tones {
        let Some(fixed) = crate::tone_library::PROGRAM_TONES
            .iter()
            .find(|fixed| fixed.name == entry.name)
        else {
            return Err(format!(
                "\"{}\" is no built-in tone name; the names are fixed per program \
                 (`programs` lists them)",
                entry.name
            ));
        };
        if file.programs.get(&fixed.program) != Some(&entry.name) {
            return Err(format!(
                "\"{}\" is program {}'s tone, but that program does not start on it",
                entry.name, fixed.program
            ));
        }
        // A tone of another kind of string than its program's would never be heard (the
        // app plays a program's tone only on the program's strings).
        if let Some(expected) = StringKind::for_program(fixed.program)
            && crate::settings::kind_from_id(&entry.kind) != Some(expected)
        {
            return Err(format!(
                "\"{}\" is for {} strings, but program {} plays {} strings",
                entry.name,
                entry.kind,
                fixed.program,
                kind_name(expected)
            ));
        }
        if entry.family != fixed.family || entry.description != fixed.description {
            return Err(format!(
                "\"{}\" must keep its fixed family and description",
                entry.name
            ));
        }
    }
    for (sound, name) in &file.rse {
        if !names.contains(name.as_str()) {
            return Err(format!(
                "the RSE sound \"{sound}\" starts on \"{name}\", which is not in the library"
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
