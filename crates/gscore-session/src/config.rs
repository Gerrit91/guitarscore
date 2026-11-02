//! User settings files: where they live and how they are read. Only the keymap for now
//! (AGENTS.md section 5, decision 7); a broken file never stops the app, it falls back to
//! the built-in settings and says why.

use std::path::{Path, PathBuf};

use gscore_edit::Keymap;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Application directory name inside the platform's config directory.
const APP_DIR: &str = "guitarscore";
/// The keymap's file name in the config directory.
pub const KEYMAP_FILE: &str = "keymap.txt";
/// The application settings' file name.
pub const SETTINGS_FILE: &str = "settings.json";
/// Keymap files larger than this are refused (a keymap is a few hundred bytes).
const MAX_KEYMAP_BYTES: u64 = 256 * 1024;
/// Settings files larger than this are refused.
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

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

/// The settings file in the user's config directory, if there is one.
pub fn default_settings_path() -> Option<PathBuf> {
    default_config_dir().map(|dir| dir.join(SETTINGS_FILE))
}

/// A JSON settings file that was read, with the default value if it could not be used.
#[derive(Debug, Clone)]
pub struct LoadedJson<T> {
    /// The file's value, or `T::default()`.
    pub value: T,
    /// The file that was read, if it exists.
    pub path: Option<PathBuf>,
    /// Why the file was not used, for the user.
    pub problem: Option<String>,
}

/// Reads a JSON settings file. A missing file means the defaults without a problem; an
/// unreadable, oversized or invalid one also means the defaults, with the problem described.
/// A broken settings file never stops the app.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> LoadedJson<T> {
    let fallback = |problem: String| LoadedJson {
        value: T::default(),
        path: Some(path.to_path_buf()),
        problem: Some(problem),
    };
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedJson {
                value: T::default(),
                path: None,
                problem: None,
            };
        }
        Err(error) => return fallback(format!("cannot read {}: {error}", path.display())),
    };
    if size > MAX_SETTINGS_BYTES {
        return fallback(format!(
            "{} is too large for a settings file ({size} bytes)",
            path.display()
        ));
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return fallback(format!("cannot read {}: {error}", path.display())),
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => LoadedJson {
            value,
            path: Some(path.to_path_buf()),
            problem: None,
        },
        Err(error) => fallback(format!(
            "{} is not a valid settings file, using the defaults: {error}",
            path.display()
        )),
    }
}

/// Writes a JSON settings file through a temporary file and a rename, creating the folder.
/// The file is either the old one or the new one, never half written.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_vec_pretty(value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let temporary = path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temporary)?;
        use std::io::Write as _;
        file.write_all(&text)?;
        file.sync_all()?;
        drop(file);
        // Windows cannot rename over an existing file; removing first is fine for settings.
        if std::fs::rename(&temporary, path).is_err() {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&temporary, path)?;
        }
        Ok(())
    };
    write().map_err(|e| format!("cannot write {}: {e}", path.display()))
}
