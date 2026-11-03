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
    all_sf2(dir).into_iter().next()
}

/// Every `.sf2` file in `dir`, sorted by name.
fn all_sf2(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
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
    found
}

/// SoundFonts the settings window can offer: the configured one, every `.sf2` in the user's
/// SoundFont folders and the usual system folders, and the well-known system files. Unique
/// and sorted, with the configured one first.
pub fn available_soundfonts() -> Vec<PathBuf> {
    let env = std::env::var_os(SOUNDFONT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    available_soundfonts_in(env, home.as_deref(), Path::new("/"))
}

/// [`available_soundfonts`] with the variable, home folder and system root given (for tests).
pub fn available_soundfonts_in(
    env: Option<PathBuf>,
    home: Option<&Path>,
    root: &Path,
) -> Vec<PathBuf> {
    /// Never offer an unbounded list from a huge folder.
    const MAX_SOUNDFONTS: usize = 128;
    let under_root = |path: &str| root.join(path.trim_start_matches('/'));
    let mut found: Vec<PathBuf> = Vec::new();
    if let Some(env) = env
        && env.is_file()
    {
        found.push(env);
    }
    if let Some(home) = home {
        for dir in HOME_DIRS {
            found.extend(all_sf2(&home.join(dir)));
        }
    }
    for file in SYSTEM_FILES {
        let path = under_root(file);
        if path.is_file() {
            found.push(path);
        }
    }
    for dir in SYSTEM_DIRS {
        found.extend(all_sf2(&under_root(dir)));
    }
    let first = found.first().cloned();
    found.sort();
    found.dedup();
    if let Some(first) = first {
        // The configured one stays first; if it is missing, the sorted list is unchanged.
        if let Some(index) = found.iter().position(|path| path == &first) {
            let configured = found.remove(index);
            found.insert(0, configured);
        }
    }
    found.truncate(MAX_SOUNDFONTS);
    found
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gscore-soundfonts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_soundfont_in_the_usual_places_is_offered_once() {
        let root = temp_dir("root");
        let home = temp_dir("home");
        let system = root.join("usr/share/soundfonts");
        let user = home.join(".local/share/soundfonts");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(system.join("B.sf2"), b"x").unwrap();
        std::fs::write(system.join("A.sf2"), b"x").unwrap();
        std::fs::write(system.join("not-a-font.txt"), b"x").unwrap();
        std::fs::write(user.join("User.sf2"), b"x").unwrap();
        let configured = system.join("B.sf2");

        let found = available_soundfonts_in(Some(configured.clone()), Some(&home), &root);
        assert_eq!(
            found,
            [
                configured.clone(),
                user.join("User.sf2"),
                system.join("A.sf2")
            ]
            .into_iter()
            .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(home);
    }
}
