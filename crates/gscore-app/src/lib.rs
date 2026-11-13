//! The GuitarScore desktop application (egui/eframe): a thin shell that holds state and
//! delegates. Files are opened and saved in the background through `gscore-session`,
//! edited at a cursor through `gscore-edit` (keys go through the keymap), laid out by
//! `gscore-layout`, and the draw operations are painted with egui.

mod app;
mod canvas;
pub mod chain_settings;
mod document;
mod export;
mod fonts;
mod keys;
mod playback;
mod rse_sound;
mod settings;
mod state;
mod tasks;
mod tone_library;
#[cfg(not(target_arch = "wasm32"))]
pub mod tone_tool;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::{Instant, SystemTime, UNIX_EPOCH};
/// Clocks that also work in a browser: `std::time::Instant` and `SystemTime` panic on
/// `wasm32-unknown-unknown` ("time not implemented on this platform"); `web_time`'s use
/// `performance.now()` and `Date.now()` there. Never use the `std` ones in this crate.
#[cfg(target_arch = "wasm32")]
pub(crate) use web_time::{Instant, SystemTime, UNIX_EPOCH};
// Native uses `std::time::Instant`; the dependency is still declared for the wasm build.
#[cfg(not(target_arch = "wasm32"))]
use web_time as _;

pub use app::GuitarScoreApp;
pub use document::{Document, GridData};
pub use gscore_edit::Keymap;
#[cfg(target_arch = "wasm32")]
pub use playback::BROWSER_SOUNDFONT_NAME;
pub use playback::{Heard, Output, Playback, SPEED_CHOICES, SPEED_RANGE, SpeedTrainer};
pub use settings::{
    DEFAULT_AUTOSAVE_SECONDS, MAX_OPEN_FILES, MAX_RECENT_FILES, MAX_SHAPE_PRESETS, PresetTone,
    ReverbBus, Settings, ShapePreset, ShortcutSetting, WindowState,
};
pub use state::{MeasureSelection, ScoreView, ThemeChoice, ViewState, ZOOM_RANGE};

/// The app icon as a 256 px PNG (`packaging/guitarscore.svg` rendered): the window icon
/// and the picture in Help › About.
pub const APP_ICON: &[u8] = include_bytes!("../../../packaging/guitarscore.png");

// The snapshot test and example render the app offscreen and save PNGs; this marks the
// test-only dependencies for the library test target's unused-dependency check.
#[cfg(test)]
use {egui_kittest as _, image as _};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::Path;

    /// The `.rs` files below `dir`.
    fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    #[test]
    fn the_app_reads_the_time_only_through_the_browser_safe_clocks() {
        // `std::time::Instant::now()` and `SystemTime::now()` panic in the browser build;
        // the crate's own `Instant` and `SystemTime` (see above) work on both. The tone
        // tool runs on the desktop only.
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        assert!(files.len() > 20, "the sources were found");
        for file in files {
            if file.ends_with("lib.rs")
                || file
                    .components()
                    .any(|part| part.as_os_str() == "tone_tool")
            {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            for (number, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or_default();
                let std_clock = code.contains("std::time::")
                    && (code.contains("Instant") || code.contains("SystemTime"));
                assert!(
                    !std_clock,
                    "{}:{}: use crate::Instant or crate::SystemTime, which work in the browser",
                    file.display(),
                    number + 1
                );
            }
        }
    }
}
