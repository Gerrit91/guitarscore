//! Browser storage for a web build: settings and the keymap come from `localStorage`
//! instead of the config directory, and a missing or broken entry falls back to the
//! defaults exactly like a broken file does on the desktop.
//!
//! `localStorage` is synchronous and small (a few megabytes), which suits the settings
//! file. Documents themselves are opened and downloaded through the browser; the recovery
//! journal is not written in the browser yet (see `docs/web.md`).

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::{LoadedJson, LoadedKeymap};
use gscore_edit::Keymap;

/// Settings larger than this are refused (a broken or hostile value must not fill the
/// origin's storage quota).
const MAX_WEB_SETTINGS_BYTES: usize = 1024 * 1024;

/// A named `localStorage` entry.
#[derive(Debug, Clone, Copy)]
pub struct WebKey(&'static str);

impl WebKey {
    /// Settings JSON.
    pub const SETTINGS: WebKey = WebKey("guitarscore.settings");
}

fn storage() -> Result<web_sys::Storage, String> {
    let window = web_sys::window().ok_or_else(|| "there is no browser window".to_owned())?;
    window
        .local_storage()
        .map_err(|_| "browser storage is not available".to_owned())?
        .ok_or_else(|| "browser storage is disabled for this page".to_owned())
}

/// Reads a JSON value from browser storage. A missing entry means the defaults without a
/// problem; an unreadable, oversized or invalid one also means the defaults, with the
/// problem described (the same contract as [`crate::load_json`]).
pub fn load_web_json<T: DeserializeOwned + Default>(key: WebKey) -> LoadedJson<T> {
    let fallback = |problem: String| LoadedJson {
        value: T::default(),
        path: None,
        problem: Some(problem),
    };
    let storage = match storage() {
        Ok(storage) => storage,
        Err(problem) => return fallback(problem),
    };
    let text = match storage.get_item(key.0) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return LoadedJson {
                value: T::default(),
                path: None,
                problem: None,
            };
        }
        Err(_) => return fallback("cannot read browser storage".to_owned()),
    };
    if text.len() > MAX_WEB_SETTINGS_BYTES {
        return fallback(format!(
            "the stored {} entry is too large ({} bytes)",
            key.0,
            text.len()
        ));
    }
    match serde_json::from_str(&text) {
        Ok(value) => LoadedJson {
            value,
            path: None,
            problem: None,
        },
        Err(error) => fallback(format!(
            "the stored {} entry is not valid JSON, using the defaults: {error}",
            key.0
        )),
    }
}

/// Writes a JSON value to browser storage, replacing the entry. A full storage area is an
/// error the settings window can show, never a panic.
pub fn save_web_json<T: Serialize>(key: WebKey, value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value)
        .map_err(|error| format!("cannot serialize the settings: {error}"))?;
    if text.len() > MAX_WEB_SETTINGS_BYTES {
        return Err(format!("the settings are too large ({} bytes)", text.len()));
    }
    storage()?
        .set_item(key.0, &text)
        .map_err(|_| "cannot write browser storage (it may be full)".to_owned())
}

/// The keymap in a browser: the built-in one. There is no keymap file to read; shortcuts
/// changed in the settings still apply on top of it.
pub fn load_web_keymap() -> LoadedKeymap {
    LoadedKeymap {
        keymap: Keymap::default(),
        path: None,
        problem: None,
    }
}
