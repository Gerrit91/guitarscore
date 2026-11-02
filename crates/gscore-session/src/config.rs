//! User settings files: where they live and how they are read. Only the keymap for now
//! (AGENTS.md section 5, decision 7); a broken file never stops the app, it falls back to
//! the built-in settings and says why.

use std::path::{Path, PathBuf};

use gscore_edit::Keymap;

/// Application directory name inside the platform's config directory.
const APP_DIR: &str = "guitarscore";
/// The keymap's file name in the config directory.
pub const KEYMAP_FILE: &str = "keymap.txt";
/// Keymap files larger than this are refused (a keymap is a few hundred bytes).
const MAX_KEYMAP_BYTES: u64 = 256 * 1024;

/// The per-user settings directory, or `None` if the environment does not say where the
/// home directory is.
///
/// Linux and other Unix: `$XDG_CONFIG_HOME/guitarscore`, defaulting to
/// `~/.config/guitarscore`. macOS: `~/Library/Application Support/guitarscore`.
/// Windows: `%APPDATA%\guitarscore`.
pub fn default_config_dir() -> Option<PathBuf> {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        env_dir("APPDATA").map(|d| d.join(APP_DIR))
    } else if cfg!(target_os = "macos") {
        env_dir("HOME").map(|d| d.join("Library").join("Application Support").join(APP_DIR))
    } else {
        env_dir("XDG_CONFIG_HOME")
            .or_else(|| env_dir("HOME").map(|h| h.join(".config")))
            .map(|d| d.join(APP_DIR))
    }
}

/// The keymap in use and, if the user's file could not be used, why.
#[derive(Debug, Clone)]
pub struct LoadedKeymap {
    /// The user's keymap, or the built-in one.
    pub keymap: Keymap,
    /// The file that was read, if there was one.
    pub path: Option<PathBuf>,
    /// Why the file was not used (it is then the built-in keymap), for the user.
    pub problem: Option<String>,
}

/// Reads the keymap file at `path`. A missing file means the built-in keymap; an unreadable
/// or invalid one also gives the built-in keymap, with the problem described.
pub fn load_keymap(path: &Path) -> LoadedKeymap {
    let fallback = |problem: String| LoadedKeymap {
        keymap: Keymap::default(),
        path: Some(path.to_path_buf()),
        problem: Some(problem),
    };
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedKeymap {
                keymap: Keymap::default(),
                path: None,
                problem: None,
            };
        }
        Err(error) => return fallback(format!("cannot read {}: {error}", path.display())),
    };
    if size > MAX_KEYMAP_BYTES {
        return fallback(format!(
            "{} is too large for a keymap ({size} bytes)",
            path.display()
        ));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => return fallback(format!("cannot read {}: {error}", path.display())),
    };
    match Keymap::parse(&text) {
        Ok(keymap) => LoadedKeymap {
            keymap,
            path: Some(path.to_path_buf()),
            problem: None,
        },
        Err(errors) => {
            let details: Vec<String> = errors.iter().map(ToString::to_string).collect();
            fallback(format!(
                "{} has errors, using the built-in keymap: {}",
                path.display(),
                details.join("; ")
            ))
        }
    }
}

/// The keymap from the user's config directory, or the built-in one.
pub fn load_user_keymap() -> LoadedKeymap {
    match default_config_dir() {
        Some(dir) => load_keymap(&dir.join(KEYMAP_FILE)),
        None => LoadedKeymap {
            keymap: Keymap::default(),
            path: None,
            problem: None,
        },
    }
}
