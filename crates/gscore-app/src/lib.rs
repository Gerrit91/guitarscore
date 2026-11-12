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
pub(crate) use std::time::Instant;
/// A clock that also works in a browser: `std::time::Instant` panics on
/// `wasm32-unknown-unknown`, `web_time::Instant` uses `performance.now()` there.
#[cfg(target_arch = "wasm32")]
pub(crate) use web_time::Instant;
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
