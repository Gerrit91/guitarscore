//! The GuitarScore desktop application (egui/eframe): a thin shell that holds state and
//! delegates. Files are opened and saved in the background through `gscore-session`,
//! edited at a cursor through `gscore-edit` (keys go through the keymap), laid out by
//! `gscore-layout`, and the draw operations are painted with egui.

mod app;
mod canvas;
mod document;
mod fonts;
mod keys;
mod playback;
mod state;
mod tasks;

pub use app::GuitarScoreApp;
pub use document::Document;
pub use gscore_edit::Keymap;
pub use playback::Playback;
pub use state::{ThemeChoice, ViewState, ZOOM_RANGE};

// The snapshot test and example render the app offscreen and save PNGs; this marks the
// test-only dependencies for the library test target's unused-dependency check.
#[cfg(test)]
use {egui_kittest as _, image as _};
