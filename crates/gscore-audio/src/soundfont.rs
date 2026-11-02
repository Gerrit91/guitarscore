//! Finding a SoundFont for the internal synth: the `GSCORE_SOUNDFONT` variable, then the
//! places Linux distributions and macOS install General MIDI SoundFonts to.

use std::path::{Path, PathBuf};

/// Environment variable naming the SoundFont to use.
pub const SOUNDFONT_ENV: &str = "GSCORE_SOUNDFONT";

/// Well-known files, relative to the system root, tried in order.
const SYSTEM_FILES: &[&str] = &[
    // Arch (`soundfont-fluid`), Fedora (`fluid-soundfont-gm`).
    "/usr/share/soundfonts/default.sf2",
    "/usr/share/soundfonts/FluidR3_GM.sf2",
    // Debian and Ubuntu (`fluid-soundfont-gm`, `timgm6mb-soundfont`).
    "/usr/share/sounds/sf2/default-GM.sf2",
    "/usr/share/sounds/sf2/FluidR3_GM.sf2",
    "/usr/share/sounds/sf2/TimGM6mb.sf2",
];

/// Folders searched for any `.sf2` file when none of the well-known files exists.
const SYSTEM_DIRS: &[&str] = &["/usr/share/soundfonts", "/usr/share/sounds/sf2"];

/// Folders below the home folder searched the same way.
const HOME_DIRS: &[&str] = &[".local/share/soundfonts", "Library/Audio/Sounds/Banks"];

/// The SoundFont to use: `GSCORE_SOUNDFONT` if set, otherwise the first one found in the
/// usual places. `None` if there is none.
pub fn find_soundfont() -> Option<PathBuf> {
    let env = std::env::var_os(SOUNDFONT_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    find_soundfont_in(env, home.as_deref(), Path::new("/"))
}

/// [`find_soundfont`] with the variable, the home folder and the system root given (for
/// tests). Order: the variable (even if its file is missing, so the user hears about it),
/// the user's SoundFont folders, the well-known system files, any `.sf2` in the system
/// folders.
pub fn find_soundfont_in(
    env: Option<PathBuf>,
    home: Option<&Path>,
    root: &Path,
) -> Option<PathBuf> {
    if env.is_some() {
        return env;
    }
    let under_root = |p: &str| root.join(p.trim_start_matches('/'));
    let home_dirs = home
        .into_iter()
        .flat_map(|home| HOME_DIRS.iter().map(move |d| home.join(d)));
    let mut home_found = home_dirs.filter_map(|d| first_sf2(&d));
    home_found
        .next()
        .or_else(|| {
            SYSTEM_FILES
                .iter()
                .map(|p| under_root(p))
                .find(|f| f.is_file())
        })
        .or_else(|| SYSTEM_DIRS.iter().find_map(|d| first_sf2(&under_root(d))))
}

/// The first `.sf2` file in `dir` by name, if any.
fn first_sf2(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sf2"))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}
