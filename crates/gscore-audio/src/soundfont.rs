//! Finding a SoundFont for the internal synth: the `GSCORE_SOUNDFONT` variable, the user's
//! own SoundFont folders, the SoundFont shipped next to the executable (a release archive
//! puts `FluidR3_GM.sf2` beside the binary), and the places where Linux distributions and
//! macOS install General MIDI SoundFonts.

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
    let exe = std::env::current_exe().ok();
    find_soundfont_in(env, home.as_deref(), Path::new("/"), exe.as_deref())
}

/// [`find_soundfont`] with the variable, the home folder, the system root and the
/// executable given (for tests). Order: the variable (even if its file is missing, so the
/// user hears about it), the user's SoundFont folders, the SoundFont bundled next to the
/// executable, the well-known system files, any `.sf2` in the system folders.
pub fn find_soundfont_in(
    env: Option<PathBuf>,
    home: Option<&Path>,
    root: &Path,
    exe: Option<&Path>,
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
        .or_else(|| exe.and_then(first_bundled))
        .or_else(|| {
            SYSTEM_FILES
                .iter()
                .map(|p| under_root(p))
                .find(|f| f.is_file())
        })
        .or_else(|| SYSTEM_DIRS.iter().find_map(|d| first_sf2(&under_root(d))))
}

/// The SoundFont shipped next to `exe`: `FluidR3_GM.sf2`, then any `.sf2` in a
/// `soundfonts/` folder beside it. Empty when nothing is bundled (a source build).
fn bundled_soundfonts(exe: &Path) -> Vec<PathBuf> {
    let dir = exe.parent().unwrap_or(Path::new("."));
    let mut found: Vec<PathBuf> = Vec::new();
    let default = dir.join("FluidR3_GM.sf2");
    if default.is_file() {
        found.push(default);
    }
    found.extend(all_sf2(&dir.join("soundfonts")));
    found.sort();
    found.dedup();
    found
}

/// The SoundFont to play with, given the one saved in the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSoundFont {
    /// The SoundFont to use, if there is one.
    pub path: Option<PathBuf>,
    /// Why the saved one is not used, for the user (it is gone and nothing replaces it).
    pub problem: Option<String>,
}

/// Where the SoundFont saved in the settings is now: see [`resolve_saved_soundfont_in`].
pub fn resolve_saved_soundfont(saved: &Path) -> SavedSoundFont {
    let exe = std::env::current_exe().ok();
    resolve_saved_soundfont_in(saved, exe.as_deref(), find_soundfont)
}

/// [`resolve_saved_soundfont`] with the executable and the search given (for tests).
///
/// The settings keep a path, and a bundled SoundFont's path changes when the app folder is
/// moved or replaced by an update (and at every start of an AppImage, which is mounted in
/// a new place each time). So a saved SoundFont that is missing, or that is the bundled
/// one of another copy of the app, quietly becomes the one bundled with this executable
/// under the same name: that is what the user chose, not a problem. Only a missing
/// SoundFont that nothing bundled replaces is reported; `find` then picks another one.
pub fn resolve_saved_soundfont_in(
    saved: &Path,
    exe: Option<&Path>,
    find: impl FnOnce() -> Option<PathBuf>,
) -> SavedSoundFont {
    let exists = saved.is_file();
    let replacement = exe
        .filter(|exe| !exists || is_bundled_with_an_app(saved, exe))
        .and_then(|exe| same_bundled(saved, exe));
    if let Some(path) = replacement {
        return SavedSoundFont {
            path: Some(path),
            problem: None,
        };
    }
    if exists {
        return SavedSoundFont {
            path: Some(saved.to_path_buf()),
            problem: None,
        };
    }
    let path = find();
    let problem = match &path {
        Some(found) => format!(
            "The SoundFont {} was not found; playing with {} instead.",
            saved.display(),
            found.display()
        ),
        None => format!(
            "The SoundFont {} was not found; choose one in Settings › Audio I/O.",
            saved.display()
        ),
    };
    SavedSoundFont {
        path,
        problem: Some(problem),
    }
}

/// Whether `soundfont` lies where a copy of the app bundles its SoundFont: beside an
/// executable named like `exe`, or in a `soundfonts/` folder beside one.
fn is_bundled_with_an_app(soundfont: &Path, exe: &Path) -> bool {
    let Some(exe_name) = exe.file_name() else {
        return false;
    };
    let Some(dir) = soundfont.parent() else {
        return false;
    };
    let app_dir = if dir.file_name().is_some_and(|name| name == "soundfonts") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    app_dir.join(exe_name).is_file()
}

/// The SoundFont bundled with `exe` that matches `saved`: the same file name, in the same
/// place relative to the executable (beside it or in its `soundfonts/` folder).
fn same_bundled(saved: &Path, exe: &Path) -> Option<PathBuf> {
    let in_folder = |path: &Path| {
        path.parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "soundfonts")
    };
    bundled_soundfonts(exe).into_iter().find(|bundled| {
        bundled.file_name() == saved.file_name() && in_folder(bundled) == in_folder(saved)
    })
}

/// The first soundfont in [`bundled_soundfonts`], if any.
fn first_bundled(exe: &Path) -> Option<PathBuf> {
    bundled_soundfonts(exe).into_iter().next()
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

/// SoundFonts the settings window can offer: the one named by [`SOUNDFONT_ENV`], every
/// `.sf2` in the user's SoundFont folders, the one bundled next to the executable, and the
/// usual system folders and well-known system files. Unique and sorted, with the
/// [`SOUNDFONT_ENV`] one first. This reads directories: call it off the UI thread.
pub fn available_soundfonts() -> Vec<PathBuf> {
    let env = std::env::var_os(SOUNDFONT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let exe = std::env::current_exe().ok();
    available_soundfonts_in(env, home.as_deref(), Path::new("/"), exe.as_deref())
}

/// [`available_soundfonts`] with the variable, home folder, system root and executable given
/// (for tests).
pub fn available_soundfonts_in(
    env: Option<PathBuf>,
    home: Option<&Path>,
    root: &Path,
    exe: Option<&Path>,
) -> Vec<PathBuf> {
    /// Never offer an unbounded list from a huge folder.
    const MAX_SOUNDFONTS: usize = 128;
    let under_root = |path: &str| root.join(path.trim_start_matches('/'));
    let mut found: Vec<PathBuf> = Vec::new();
    let configured = env.filter(|env| env.is_file());
    if let Some(env) = &configured {
        found.push(env.clone());
    }
    if let Some(home) = home {
        for dir in HOME_DIRS {
            found.extend(all_sf2(&home.join(dir)));
        }
    }
    if let Some(exe) = exe {
        found.extend(bundled_soundfonts(exe));
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
    found.sort();
    found.dedup();
    // The configured one goes first; without one, the list stays sorted.
    if let Some(configured) = configured
        && let Some(index) = found.iter().position(|path| path == &configured)
    {
        let configured = found.remove(index);
        found.insert(0, configured);
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

        let found = available_soundfonts_in(Some(configured.clone()), Some(&home), &root, None);
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

        // Without a configured one, the list is simply sorted, even when a well-known
        // file (found before the folders are listed) sorts after another one.
        std::fs::write(system.join("default.sf2"), b"x").unwrap();
        let found = available_soundfonts_in(None, None, &root, None);
        assert_eq!(
            found,
            [
                system.join("A.sf2"),
                system.join("B.sf2"),
                system.join("default.sf2")
            ]
            .into_iter()
            .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_bundled_soundfont_is_offered() {
        let root = temp_dir("bundled-list-root");
        let bin = temp_dir("bundled-list-bin");
        std::fs::write(bin.join("FluidR3_GM.sf2"), b"x").unwrap();
        let found = available_soundfonts_in(None, None, &root, Some(&bin.join("guitarscore")));
        assert_eq!(found, vec![bin.join("FluidR3_GM.sf2")]);
        let _ = std::fs::remove_dir_all(bin);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A copy of the app in `dir`: the executable and its bundled FluidR3 (and, in its
    /// `soundfonts/` folder, `Extra.sf2`).
    fn install(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("soundfonts")).unwrap();
        std::fs::write(dir.join("guitarscore"), b"x").unwrap();
        std::fs::write(dir.join("FluidR3_GM.sf2"), b"x").unwrap();
        std::fs::write(dir.join("soundfonts/Extra.sf2"), b"x").unwrap();
        dir.join("guitarscore")
    }

    fn never_searched() -> Option<PathBuf> {
        panic!("the search must not run")
    }

    #[test]
    fn a_bundled_soundfont_left_behind_by_a_moved_or_updated_app_is_replaced_quietly() {
        let base = temp_dir("saved-moved");
        let exe = install(&base.join("new"));
        let used = |saved: &Path| resolve_saved_soundfont_in(saved, Some(&exe), never_searched);

        // The old folder is gone (moved, or deleted after an update; an AppImage's mount).
        for (saved, now) in [
            ("old/FluidR3_GM.sf2", "new/FluidR3_GM.sf2"),
            ("old/soundfonts/Extra.sf2", "new/soundfonts/Extra.sf2"),
        ] {
            let found = used(&base.join(saved));
            assert_eq!(found.path, Some(base.join(now)));
            assert_eq!(found.problem, None);
        }

        // The old copy of the app is still there: the running one's SoundFont is used.
        install(&base.join("old"));
        let found = used(&base.join("old/FluidR3_GM.sf2"));
        assert_eq!(found.path, Some(base.join("new/FluidR3_GM.sf2")));
        assert_eq!(found.problem, None);

        // The running app's own SoundFont stays as it is.
        let own = base.join("new/FluidR3_GM.sf2");
        assert_eq!(used(&own).path, Some(own));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_soundfont_the_user_chose_is_kept_and_reported_only_once_it_is_gone() {
        let base = temp_dir("saved-own");
        let exe = install(&base.join("app"));
        let own = base.join("fonts/FluidR3_GM.sf2");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, b"x").unwrap();

        // An existing one is used, even with the bundled one's name: it is not an app's.
        let found = resolve_saved_soundfont_in(&own, Some(&exe), never_searched);
        assert_eq!(found.path, Some(own.clone()));
        assert_eq!(found.problem, None);

        // A missing one that nothing bundled replaces: the search, and the user is told.
        let gone = base.join("fonts/Gone.sf2");
        let other = base.join("app/FluidR3_GM.sf2");
        let found = resolve_saved_soundfont_in(&gone, Some(&exe), || Some(other.clone()));
        assert_eq!(found.path, Some(other));
        assert!(found.problem.unwrap().contains("Gone.sf2"));
        let found = resolve_saved_soundfont_in(&gone, None, || None);
        assert_eq!(found.path, None);
        assert!(found.problem.unwrap().contains("Settings"));
        let _ = std::fs::remove_dir_all(base);
    }
}
