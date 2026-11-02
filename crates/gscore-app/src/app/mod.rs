//! The application: menus, toolbar, the page with the edit cursor, and the status bar. It
//! holds state and delegates: opening and saving to [`Tasks`], editing to the document's
//! [`gscore_edit::TabEditor`], layout to [`Document`], painting to the canvas.
//!
//! Split by job: `bars` (menus, toolbar, status), `page` (the page view, cursor and
//! clicks), `editing` (keys and actions), `files` (open, save, unsaved changes).

mod bars;
mod edit_effects;
mod edit_palette;
mod editing;
mod files;
mod grid;
mod icons;
mod instrument;
mod page;
mod recovery;
mod settings_window;
mod style;
mod tiles;
mod title_bar;
mod toolbar;
mod track_panel;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use gscore_edit::{Cursor, Keymap};
use gscore_layout::{Fonts, LayoutOptions};
use gscore_session::FoundSession;

use crate::document::Document;
use crate::keys::KeyReader;
use crate::playback::{Output, Playback};
use crate::settings::{Settings, WindowState};
use crate::state::{ThemeChoice, ViewState};
use crate::tasks::Tasks;

pub(crate) use files::Then;
use grid::RenameTrack;

/// The GuitarScore application.
#[derive(Debug)]
pub struct GuitarScoreApp {
    view: ViewState,
    fonts: Result<Fonts, String>,
    options: LayoutOptions,
    document: Option<Document>,
    tasks: Tasks,
    /// A message for the user (an error); shown until dismissed.
    notice: Option<String>,
    /// A short confirmation (saved, ...) in the status bar.
    status: Option<String>,
    /// The theme applied to egui's own widgets, to apply a change once.
    applied_theme: Option<ThemeChoice>,
    /// The window title last sent, to send a change once.
    applied_title: String,
    playback: Playback,
    keymap: Keymap,
    keys: KeyReader,
    /// A question for the user (unsaved changes, a lossy save), shown as a window.
    dialog: Option<files::Dialog>,
    /// What to do once the save that is running has succeeded.
    after_save: Option<Then>,
    /// Unsaved changes were dealt with: the window may close.
    may_close: bool,
    /// The track being renamed, while the rename window is open.
    renaming: Option<RenameTrack>,
    /// The right panel's state: the shown track, and the name being typed.
    track_panel: track_panel::TrackPanelState,
    /// The left panel's state: the chosen note length and header fields.
    edit_palette: edit_palette::EditPaletteState,
    /// Where a drag on the page started, while selecting beats.
    drag_anchor: Option<Cursor>,
    /// The cursor and revision the view last scrolled to, to scroll only on a change.
    shown_cursor: Option<(Cursor, u64)>,
    /// Where documents' recovery journals go; `None` keeps no journal.
    recovery_dir: Option<PathBuf>,
    /// Whether the recovery directory was searched for crashed documents yet.
    recovery_searched: bool,
    /// Crashed documents still to ask about.
    recoverable: Vec<FoundSession>,
    /// The settings window, while it is open.
    settings_window: Option<settings_window::SettingsWindow>,
    /// What the app remembers between runs.
    settings: Settings,
    /// Where the settings are stored (`None` keeps them only in memory).
    settings_path: Option<PathBuf>,
    /// Panel sizes, as last drawn, for the next run.
    panel_sizes: PanelSizes,
    /// The window's last position and size.
    last_window: Option<WindowState>,
}

/// Panel sizes the app remembers: the song grid's height and the track panel's width.
#[derive(Debug, Clone, Copy, Default)]
struct PanelSizes {
    grid: Option<f32>,
    track_panel: Option<f32>,
    edit_panel: Option<f32>,
}

impl GuitarScoreApp {
    /// A new app with the built-in keymap; opens `file` in the background if given.
    pub fn new(ctx: &egui::Context, file: Option<PathBuf>) -> Self {
        GuitarScoreApp::with_keymap(ctx, file, Keymap::default())
    }

    /// A new app using `keymap`; opens `file` in the background if given.
    pub fn with_keymap(ctx: &egui::Context, file: Option<PathBuf>, keymap: Keymap) -> Self {
        GuitarScoreApp::with_options(ctx, file, keymap, None)
    }

    /// A new app using `keymap`, journaling edits in `recovery_dir` (and offering, at the
    /// start, to recover what a crash left there); opens `file` in the background if
    /// given.
    pub fn with_options(
        ctx: &egui::Context,
        file: Option<PathBuf>,
        keymap: Keymap,
        recovery_dir: Option<PathBuf>,
    ) -> Self {
        GuitarScoreApp::with_settings(ctx, file, keymap, recovery_dir, Settings::default(), None)
    }

    /// A new app with the user's `settings` (theme, zoom, panels, recent files, autosave),
    /// saved back to `settings_path` on exit; opens `file` in the background if given.
    pub fn with_settings(
        ctx: &egui::Context,
        file: Option<PathBuf>,
        keymap: Keymap,
        recovery_dir: Option<PathBuf>,
        mut settings: Settings,
        settings_path: Option<PathBuf>,
    ) -> Self {
        crate::fonts::install(ctx);
        style::install(ctx);
        settings.sanitize();
        let mut view = ViewState::default();
        settings.apply_to_view(&mut view);
        let last_window = settings.window;
        let panel_sizes = PanelSizes {
            grid: settings.grid_height,
            track_panel: settings.track_panel_width,
            edit_panel: settings.edit_panel_width,
        };
        let mut app = GuitarScoreApp {
            view,
            fonts: Fonts::bundled().map_err(|e| e.to_string()),
            options: LayoutOptions::default(),
            document: None,
            tasks: Tasks::default(),
            notice: None,
            status: None,
            applied_theme: None,
            applied_title: String::new(),
            playback: Playback::default(),
            keymap,
            keys: KeyReader::default(),
            dialog: None,
            after_save: None,
            may_close: false,
            renaming: None,
            track_panel: track_panel::TrackPanelState::default(),
            edit_palette: edit_palette::EditPaletteState::default(),
            drag_anchor: None,
            shown_cursor: None,
            recovery_dir,
            recovery_searched: false,
            recoverable: Vec::new(),
            settings_window: None,
            settings,
            settings_path,
            panel_sizes,
            last_window,
        };
        if let Some(path) = file {
            app.open(path, ctx);
        }
        app
    }

    /// The settings as they are now (including the view and window state).
    pub fn settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        settings.update_from_view(&self.view);
        settings.grid_height = self.panel_sizes.grid;
        settings.track_panel_width = self.panel_sizes.track_panel;
        settings.edit_panel_width = self.panel_sizes.edit_panel;
        settings.window = self.last_window;
        settings.soundfont_path = self.playback.soundfont().map(std::path::Path::to_path_buf);
        settings.prefer_synth = matches!(self.playback.output(), Output::Synth(_));
        settings.midi_port = self.playback.port().map(str::to_owned);
        settings
    }

    /// Writes the settings to their file, if there is one. Errors go to the notice bar.
    pub fn save_settings(&mut self) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        let settings = self.settings();
        if let Err(error) = gscore_session::save_json(&path, &settings) {
            self.notice = Some(error);
        } else {
            self.settings = settings;
        }
    }

    /// Shows `notice` to the user (for example a problem with the keymap file).
    pub fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    /// Whether a background job (opening, saving, a file dialog) is running.
    pub fn is_busy(&self) -> bool {
        self.tasks.busy()
    }

    /// The open document.
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// The open document, to change it (its editor and cursor).
    pub fn document_mut(&mut self) -> Option<&mut Document> {
        self.document.as_mut()
    }

    /// The current message for the user, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The status bar's last confirmation, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Whether a question is waiting for the user's answer.
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// Playback (built-in synthesizer or MIDI output).
    pub fn playback(&self) -> &Playback {
        &self.playback
    }

    /// Playback, to change it.
    pub fn playback_mut(&mut self) -> &mut Playback {
        &mut self.playback
    }

    /// The view state (theme, zoom, track).
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// The view state, to change it.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        // The chrome stays dark; the score's theme only changes how the page is drawn,
        // which reads `view.theme` every frame. A change needs one more frame.
        if self.applied_theme != Some(self.view.theme) {
            ctx.request_repaint();
            self.applied_theme = Some(self.view.theme);
        }
    }

    fn apply_title(&mut self, ctx: &egui::Context) {
        let title = self.document.as_ref().map_or_else(
            || "GuitarScore".to_owned(),
            |d| {
                let changed = if d.is_dirty() { "• " } else { "" };
                format!("{changed}{} — GuitarScore", d.display_title())
            },
        );
        if title != self.applied_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.applied_title = title;
        }
    }
}

impl eframe::App for GuitarScoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(rect) = ctx.input(|input| input.viewport().outer_rect) {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            self.last_window = Some(WindowState {
                x: rect.min.x,
                y: rect.min.y,
                width: rect.width(),
                height: rect.height(),
                maximized,
            });
        }
        self.handle_events(&ctx);
        // A file dropped on the window opens like File > Open (asking about unsaved work).
        if let Some(path) = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        }) {
            self.open(path, &ctx);
        }
        self.search_recoverable(&ctx);
        self.handle_close_request(&ctx);
        self.apply_theme(&ctx);
        // Keys first, before any widget sees them (a focused button would take Space).
        self.handle_keys(&ctx);
        // The cursor follows the beat being played; the page scrolls with it.
        self.follow_playback();
        if self.playback.state() == gscore_play::PlayState::Playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.apply_title(&ctx);

        egui::Panel::top("title_bar")
            .frame(style::chrome_frame(&ctx, 0))
            .exact_size(title_bar::TITLE_BAR_HEIGHT)
            .resizable(false)
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::top("toolbar")
            .frame(style::chrome_frame(&ctx, 5))
            .show(ui, |ui| self.toolbar(ui));
        if let Some(progress) = self.tasks.open_progress().cloned() {
            egui::Panel::top("opening").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Opening…");
                    if let Some(fraction) = progress.fraction() {
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(220.0)
                                .show_percentage(),
                        );
                    } else {
                        ui.spinner();
                    }
                    if progress.importing() {
                        ui.label("Importing…");
                    }
                    if ui.button("Cancel").clicked() {
                        self.tasks.cancel_open();
                    }
                });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::Panel::bottom("status")
            .frame(style::chrome_frame(&ctx, 3))
            .show(ui, |ui| self.status_bar(ui));
        if self.view.show_track_grid && self.document.is_some() {
            let panel = egui::Panel::bottom("track_grid")
                .resizable(true)
                .min_size(80.0)
                .default_size(self.panel_sizes.grid.unwrap_or(grid::DEFAULT_HEIGHT))
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| self.track_grid(ui));
                });
            self.panel_sizes.grid = Some(panel.response.rect.height());
        }
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        if self.view.show_track_panel && self.document.is_some() {
            let panel = egui::Panel::right("track_panel")
                .resizable(true)
                .min_size(200.0)
                .default_size(self.panel_sizes.track_panel.unwrap_or(260.0))
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    self.track_panel(ui);
                });
            self.panel_sizes.track_panel = Some(panel.response.rect.width());
        }
        if self.view.show_edit_panel && self.document.is_some() {
            let panel = egui::Panel::left("edit_palette")
                .resizable(true)
                .min_size(200.0)
                .default_size(self.panel_sizes.edit_panel.unwrap_or(250.0))
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    self.edit_palette(ui);
                });
            self.panel_sizes.edit_panel = Some(panel.response.rect.width());
        }
        egui::CentralPanel::default()
            .frame(style::canvas_frame(&ctx))
            .show(ui, |ui| {
                // A bug in drawing must not take the app down: contain it and tell the user.
                let drawn = catch_unwind(AssertUnwindSafe(|| self.page_view(ui)));
                if drawn.is_err() {
                    self.notice = Some(
                        "Drawing the page failed unexpectedly; a crash report was written"
                            .to_owned(),
                    );
                }
            });
        title_bar::resize_edges(&ctx);
        self.show_dialog(&ctx);
        self.show_rename_window(&ctx);
        self.show_settings_window(&ctx);
        // Every edit of this frame goes to the recovery journal.
        if let Some(document) = &mut self.document
            && let Some(error) = document.record_changes()
        {
            self.notice = Some(error);
        }
    }

    fn on_exit(&mut self) {
        self.save_settings();
        // A normal exit (unsaved changes were asked about): the recovery folder goes.
        if let Some(document) = self.document.take() {
            document.close(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn loaded_settings_restore_the_view() {
        let ctx = egui::Context::default();
        let settings = Settings {
            theme: ThemeChoice::Dark,
            zoom: 1.5,
            show_track_panel: false,
            ..Settings::default()
        };
        let app =
            GuitarScoreApp::with_settings(&ctx, None, Keymap::default(), None, settings, None);
        assert_eq!(app.view().theme, ThemeChoice::Dark);
        assert!((app.view().zoom() - 1.5).abs() < 1e-4);
        assert!(!app.view().show_track_panel);
        assert!(app.document().is_none());
    }

    #[test]
    fn the_window_state_is_only_saved_when_it_makes_sense() {
        let ctx = egui::Context::default();
        let settings = Settings {
            window: Some(WindowState {
                x: f32::NAN,
                y: 0.0,
                width: -10.0,
                height: 0.0,
                maximized: false,
            }),
            ..Settings::default()
        };
        let app =
            GuitarScoreApp::with_settings(&ctx, None, Keymap::default(), None, settings, None);
        assert!(app.last_window.is_none());
    }
}
