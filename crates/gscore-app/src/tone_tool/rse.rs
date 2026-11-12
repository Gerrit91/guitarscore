//! `tones rse`: Guitar Pro's RSE sounds as placeholders for the built-in tones. The sounds
//! found in Guitar Pro files are listed by name, and a name can be mapped to a tone of the
//! library, so a track made with that sound starts on it.

use super::*;

/// What `rse` was asked to do.
#[derive(Debug, Default)]
pub(super) struct RseArgs {
    pub(super) options: Options,
    /// Guitar Pro files and folders to list the RSE sounds of.
    pub(super) paths: Vec<PathBuf>,
    /// RSE sounds to start on `tone`.
    pub(super) map: Vec<String>,
    /// The built-in tone `map` names.
    pub(super) tone: Option<String>,
    /// RSE sounds to forget the tone of.
    pub(super) unmap: Vec<String>,
    pub(super) dry_run: bool,
}

pub(super) fn parse_rse(args: &[String]) -> Result<RseArgs, String> {
    let mut rse = RseArgs::default();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--settings" => {
                rse.options.settings = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--file" => {
                rse.options.file = Some(PathBuf::from(next_value(&mut rest, argument)?));
            }
            "--map" => rse.map.push(next_value(&mut rest, argument)?),
            "--unmap" => rse.unmap.push(next_value(&mut rest, argument)?),
            "--tone" => rse.tone = Some(next_value(&mut rest, argument)?),
            "--dry-run" => rse.dry_run = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown option `{other}`\n\n{USAGE}"));
            }
            path => rse.paths.push(PathBuf::from(path)),
        }
    }
    Ok(rse)
}

pub(super) fn run_rse(args: RseArgs) -> Result<(), String> {
    let path = tones_path(args.options.file.as_deref());
    let mut file = load_library(&path)?;
    if !args.map.is_empty() || !args.unmap.is_empty() {
        change(&mut file, &args)?;
        validate(&file)?;
        if args.dry_run {
            println!("dry run: {} not written", path.display());
        } else {
            write_library(&path, &file)?;
            println!("wrote {}", path.display());
        }
        return Ok(());
    }
    if args.tone.is_some() {
        return Err(format!(
            "`--tone` names the tone for `--map SOUND`\n\n{USAGE}"
        ));
    }
    if args.paths.is_empty() {
        println!("{} RSE sound(s) mapped to a tone:", file.rse.len());
        if file.rse.is_empty() {
            println!("  (none)");
        }
        let width = file.rse.keys().map(String::len).max().unwrap_or(0);
        for (sound, tone) in &file.rse {
            println!("  {sound:<width$}  {tone}");
        }
        println!("Pass Guitar Pro files or folders to list the sounds they use.");
        return Ok(());
    }
    let found = scan(&args.paths)?;
    for line in found_lines(&file, &found) {
        println!("{line}");
    }
    Ok(())
}

/// Applies `--map SOUND --tone NAME` and `--unmap SOUND` to the library.
fn change(file: &mut ToneLibraryFile, args: &RseArgs) -> Result<(), String> {
    if !args.map.is_empty() {
        let tone = args
            .tone
            .as_deref()
            .ok_or_else(|| format!("`--map SOUND` needs `--tone NAME`\n\n{USAGE}"))?;
        let entry = file
            .tones
            .iter()
            .find(|entry| entry.name == tone)
            .ok_or_else(|| {
                let names: Vec<&str> = file.tones.iter().map(|tone| tone.name.as_str()).collect();
                format!(
                    "no built-in tone is named \"{tone}\"; the library has: {}",
                    names.join(", ")
                )
            })?;
        for sound in &args.map {
            if sound.trim().is_empty() {
                return Err("an RSE sound needs a name".to_owned());
            }
            println!("  {sound} -> \"{}\" ({} strings)", entry.name, entry.kind);
            file.rse.insert(sound.clone(), entry.name.clone());
        }
    }
    for sound in &args.unmap {
        match file.rse.remove(sound) {
            Some(tone) => println!("  {sound} no longer starts on \"{tone}\""),
            None => return Err(format!("the RSE sound \"{sound}\" is not mapped")),
        }
    }
    Ok(())
}

/// The RSE sounds of the string tracks of some files: how many tracks use each, by the
/// sound and the kind of string its track's program plays.
#[derive(Debug, Default)]
pub(super) struct Found {
    pub(super) files: usize,
    pub(super) unreadable: usize,
    pub(super) sounds: BTreeMap<(String, Option<&'static str>), usize>,
}

/// The most files one scan reads, and how deep it follows folders.
const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 12;

fn scan(paths: &[PathBuf]) -> Result<Found, String> {
    let mut files = Vec::new();
    for path in paths {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        collect(path, 0, &mut files);
    }
    files.sort();
    files.dedup();
    let mut found = Found::default();
    for path in &files {
        match gscore_session::open(path) {
            Ok(imported) => {
                found.files += 1;
                found.add(&imported.song);
            }
            Err(_) => found.unreadable += 1,
        }
    }
    Ok(found)
}

/// The Guitar Pro files at or below `path` (a file named on the command line counts
/// whatever its extension).
fn collect(path: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if files.len() >= MAX_FILES {
        return;
    }
    if path.is_dir() {
        if depth >= MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() || is_guitar_pro(&child) {
                collect(&child, depth.saturating_add(1), files);
            }
        }
    } else if depth == 0 || is_guitar_pro(path) {
        files.push(path.to_path_buf());
    }
}

/// Whether `path` has the extension of a Guitar Pro 7 file (older formats keep no RSE
/// sound by name).
fn is_guitar_pro(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gp"))
}

impl Found {
    pub(super) fn add(&mut self, song: &gscore_model::Song) {
        for track in 0..song.track_count() {
            let Some(sound) = crate::rse_sound::of_track(song, track) else {
                continue;
            };
            let kind = song
                .track_sound(gscore_model::TrackIndex::new(track))
                .and_then(|sound| StringKind::for_program(sound.program))
                .map(kind_name);
            let count = self.sounds.entry((sound, kind)).or_default();
            *count = count.saturating_add(1);
        }
    }
}

/// The scan as `tones rse` prints it: most used first, each sound with the tone it starts
/// on, if any.
pub(super) fn found_lines(file: &ToneLibraryFile, found: &Found) -> Vec<String> {
    let mut lines = vec![format!(
        "{} RSE sound(s) on the string tracks of {} file(s){}:",
        found.sounds.len(),
        found.files,
        if found.unreadable == 0 {
            String::new()
        } else {
            format!(" ({} could not be read)", found.unreadable)
        }
    )];
    let mut rows: Vec<(&(String, Option<&str>), &usize)> = found.sounds.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let width = rows
        .iter()
        .map(|((sound, _), _)| sound.len())
        .max()
        .unwrap_or(0);
    let mut unmapped = 0_usize;
    for ((sound, kind), count) in rows {
        let kind_text = kind.unwrap_or("-");
        let tone = match file.rse.get(sound) {
            None => {
                unmapped = unmapped.saturating_add(1);
                "(none)".to_owned()
            }
            Some(name) => {
                let tone_kind = file
                    .tones
                    .iter()
                    .find(|entry| &entry.name == name)
                    .map(|entry| entry.kind.as_str());
                match (tone_kind, kind) {
                    (Some(tone_kind), Some(kind)) if tone_kind != *kind => {
                        format!("{name}  [a {tone_kind} tone: not played on {kind} strings]")
                    }
                    (_, None) => format!("{name}  [not a guitar or bass program]"),
                    _ => name.clone(),
                }
            }
        };
        lines.push(format!(
            "  {count:>5}  {kind_text:<8}  {sound:<width$}  {tone}"
        ));
    }
    if unmapped > 0 {
        lines.push(format!(
            "{unmapped} without a tone: `cargo xtask tones rse --map \"SOUND\" --tone \"NAME\"`"
        ));
    }
    lines
}
