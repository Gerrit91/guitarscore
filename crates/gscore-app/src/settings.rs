//! The app's settings: view state, panel sizes, window, recent files, keymap and autosave.
//! Plain serializable data, stored as JSON in the user's config directory (`gscore-session`),
//! so it can be inspected and restored without a running app.

use std::path::PathBuf;
use std::time::Duration;

use gscore_session::{RecoveryOptions, SNAPSHOT_EVERY};
use serde::{Deserialize, Serialize};

use crate::state::{ViewState, ZOOM_RANGE};

/// How many recent files the File menu remembers.
pub const MAX_RECENT_FILES: usize = 10;
/// Default interval between recovery snapshots, in seconds.
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 60;

/// The window's position and size, so it opens where it was closed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Left edge in screen points.
    pub x: f32,
    /// Top edge in screen points.
    pub y: f32,
    /// Inner width.
    pub width: f32,
    /// Inner height.
    pub height: f32,
    /// Whether it was maximized.
    pub maximized: bool,
}

impl WindowState {
    /// Whether the values make sense (finite, positive size, finite position).
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    /// The size as egui takes it.
    pub fn size(self) -> [f32; 2] {
        [self.width, self.height]
    }

    /// The position as egui takes it.
    pub fn position(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

/// Everything the app remembers between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Light or dark.
    pub theme: crate::state::ThemeChoice,
    /// Zoom factor; 1 is normal.
    pub zoom: f32,
    /// Whether the song grid (bottom panel) is shown.
    pub show_track_grid: bool,
    /// Whether the track panel (right panel) is shown.
    pub show_track_panel: bool,
    /// Whether the edit palette (left panel) is shown.
    pub show_edit_panel: bool,
    /// Height the song grid was dragged to; `None`: it follows the number of tracks. (The
    /// key is new: the old `grid_height_chosen` held whatever the panel happened to be.)
    pub grid_height_chosen: Option<f32>,
    /// Width of the track panel, as last dragged.
    pub track_panel_width: Option<f32>,
    /// Width of the edit palette, as last dragged.
    pub edit_panel_width: Option<f32>,
    /// The window's last position and size.
    pub window: Option<WindowState>,
    /// Recently opened files, newest first.
    pub recent_files: Vec<PathBuf>,
    /// Reopen the last open file at startup (on by default).
    pub reopen_last_file: bool,
    /// The file to reopen at startup.
    pub last_file: Option<PathBuf>,
    /// The keymap file, if the user chose one.
    pub keymap_path: Option<PathBuf>,
    /// Seconds between recovery snapshots; 0 means only by edit count.
    pub autosave_seconds: u64,
    /// The SoundFont chosen for the built-in synthesizer, if any.
    pub soundfont_path: Option<PathBuf>,
    /// Whether playback prefers the built-in synthesizer over MIDI output.
    pub prefer_synth: bool,
    /// The MIDI port chosen, if any.
    pub midi_port: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: crate::state::ThemeChoice::Light,
            zoom: 1.0,
            show_track_grid: true,
            show_track_panel: true,
            show_edit_panel: true,
            grid_height_chosen: None,
            track_panel_width: None,
            edit_panel_width: None,
            window: None,
            recent_files: Vec::new(),
            reopen_last_file: true,
            last_file: None,
            keymap_path: None,
            autosave_seconds: DEFAULT_AUTOSAVE_SECONDS,
            soundfont_path: None,
            prefer_synth: true,
            midi_port: None,
        }
    }
}

impl Settings {
    /// Settings that show `view`.
    pub fn from_view(view: &ViewState) -> Self {
        let mut settings = Settings::default();
        settings.update_from_view(view);
        settings
    }

    /// Takes the view state's theme, zoom and panel visibility.
    pub fn update_from_view(&mut self, view: &ViewState) {
        self.theme = view.theme;
        self.zoom = view.zoom();
        self.show_track_grid = view.show_track_grid;
        self.show_track_panel = view.show_track_panel;
        self.show_edit_panel = view.show_edit_panel;
    }

    /// Applies the stored theme, zoom and panel visibility to `view`.
    pub fn apply_to_view(&self, view: &mut ViewState) {
        view.theme = self.theme;
        view.set_zoom(self.zoom);
        view.show_track_grid = self.show_track_grid;
        view.show_track_panel = self.show_track_panel;
        view.show_edit_panel = self.show_edit_panel;
    }

    /// Puts `path` first in the recent files, without duplicates.
    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|recent| recent != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    /// How often the recovery journal writes a full snapshot.
    pub fn recovery_options(&self) -> RecoveryOptions {
        RecoveryOptions {
            snapshot_every: SNAPSHOT_EVERY,
            snapshot_interval: if self.autosave_seconds == 0 {
                Duration::ZERO
            } else {
                Duration::from_secs(self.autosave_seconds.min(24 * 60 * 60))
            },
        }
    }

    /// Makes the loaded values safe: finite zoom and sizes, a sane autosave interval, and
    /// no nonsense window rectangles.
    pub fn sanitize(&mut self) {
        if !self.zoom.is_finite() {
            self.zoom = 1.0;
        }
        self.zoom = self.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        if let Some(height) = self.grid_height_chosen
            && !(height.is_finite() && height > 0.0)
        {
            self.grid_height_chosen = None;
        }
        if let Some(width) = self.track_panel_width
            && !(width.is_finite() && width > 0.0)
        {
            self.track_panel_width = None;
        }
        if let Some(width) = self.edit_panel_width
            && !(width.is_finite() && width > 0.0)
        {
            self.edit_panel_width = None;
        }
        if self.window.is_some_and(|window| !window.is_valid()) {
            self.window = None;
        }
        if self.autosave_seconds > 24 * 60 * 60 {
            self.autosave_seconds = DEFAULT_AUTOSAVE_SECONDS;
        }
        self.recent_files.truncate(MAX_RECENT_FILES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ThemeChoice;

    #[test]
    fn recent_files_are_unique_and_bounded() {
        let mut settings = Settings::default();
        for index in 0..MAX_RECENT_FILES.saturating_add(5) {
            settings.add_recent(PathBuf::from(format!("song{index}.gp")));
        }
        assert_eq!(settings.recent_files.len(), MAX_RECENT_FILES);
        assert_eq!(settings.recent_files[0], PathBuf::from("song14.gp"));
        let oldest = settings.recent_files[0].clone();
        settings.add_recent(oldest.clone());
        assert_eq!(settings.recent_files[0], oldest);
        assert_eq!(
            settings
                .recent_files
                .iter()
                .filter(|path| **path == oldest)
                .count(),
            1
        );
    }

    #[test]
    fn view_state_round_trips_through_the_settings() {
        let mut view = ViewState::default();
        view.theme = ThemeChoice::Dark;
        view.set_zoom(1.5);
        view.show_track_grid = false;
        let mut settings = Settings::from_view(&view);
        let mut restored = ViewState::default();
        settings.apply_to_view(&mut restored);
        assert_eq!(restored, view);
        settings.zoom = f32::NAN;
        settings.grid_height_chosen = Some(f32::INFINITY);
        settings.autosave_seconds = u64::MAX;
        settings.sanitize();
        assert_eq!(settings.zoom, 1.0);
        assert_eq!(settings.grid_height_chosen, None);
        assert_eq!(settings.autosave_seconds, DEFAULT_AUTOSAVE_SECONDS);
    }

    #[test]
    fn the_autosave_interval_sets_the_snapshot_interval() {
        let mut settings = Settings::default();
        assert_eq!(
            settings.recovery_options().snapshot_interval,
            Duration::from_secs(DEFAULT_AUTOSAVE_SECONDS)
        );
        settings.autosave_seconds = 0;
        assert!(settings.recovery_options().snapshot_interval.is_zero());
    }

    #[test]
    fn a_json_round_trip_keeps_every_field() {
        let mut settings = Settings::default();
        settings.add_recent(PathBuf::from("/tabs/song.gp"));
        settings.keymap_path = Some(PathBuf::from("/keys/keymap.txt"));
        settings.grid_height_chosen = Some(180.0);
        settings.window = Some(WindowState {
            x: 10.0,
            y: 20.0,
            width: 900.0,
            height: 700.0,
            maximized: false,
        });
        let text = serde_json::to_string(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, settings);
    }
}
