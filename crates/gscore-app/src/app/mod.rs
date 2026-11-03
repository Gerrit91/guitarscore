//! The application: menus, toolbar, the page with the edit cursor, and the status bar. It
//! holds state and delegates: opening and saving to [`Tasks`], editing to the document's
//! [`gscore_edit::TabEditor`], layout to [`Document`], painting to the canvas.
//!
//! Split by job: `bars` (menus, toolbar, status), `page` (the page view, cursor and
//! clicks), `editing` (keys and actions), `files` (open, save, unsaved changes).

mod alert;
mod bars;
mod circle_of_fifths;
mod edit_dynamics;
mod edit_effects;
mod edit_palette;
mod edit_staff;
mod edit_staff_dialogs;
mod editing;
mod files;
mod grid;
mod harmonic_diagram;
mod harmonic_editor;
mod history;
mod icons;
mod instrument;
mod metronome;
mod new_song;
mod new_track;
mod page;
mod page_menu;
mod recovery;
mod settings_page;
mod settings_window;
mod shape_editor;
mod shortcuts;
mod staff_pickers;
mod staff_widgets;
mod style;
mod tab;
mod tab_bar;
mod tile_faces;
mod tiles;
mod title_bar;
mod toolbar;
mod track_panel;

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use gscore_edit::Keymap;
use gscore_layout::{Fonts, LayoutOptions};
use gscore_session::FoundSession;

use crate::document::Document;
use crate::keys::KeyReader;
use crate::playback::{Output, Playback};
use crate::settings::{Settings, WindowState};
use crate::state::{ThemeChoice, ViewState};
use crate::tasks::Tasks;
use tab::Tab;

pub(crate) use files::Then;

/// The GuitarScore application.
#[derive(Debug)]
pub struct GuitarScoreApp {
    view: ViewState,
    fonts: Result<Fonts, String>,
    options: LayoutOptions,
    /// The open documents, one per tab, and the active tab's index.
    tabs: Vec<Tab>,
    active: usize,
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
    /// Which tab the save that is running belongs to (to mark it saved).
    saving_tab: Option<usize>,
    /// A window close was deferred: each dirty tab is asked about in turn, then the window
    /// may close.
    closing_window: bool,
    /// Unsaved changes were dealt with: the window may close.
    may_close: bool,
    /// Where documents' recovery journals go; `None` keeps no journal.
    recovery_dir: Option<PathBuf>,
    /// Whether the recovery directory was searched for crashed documents yet.
    recovery_searched: bool,
    /// Crashed documents still to ask about.
    recoverable: Vec<FoundSession>,
    /// The shortcut editor, while it is open.
    shortcuts: Option<shortcuts::ShortcutPanel>,
    /// The edit history, while it is open.
    history: Option<history::HistoryPanel>,
    /// The new-song dialog, while it is open.
    new_song_dialog: Option<new_song::NewSongDialog>,
    /// Files still to open when the tabs from last time are being restored.
    pending_opens: VecDeque<PathBuf>,
    /// The file that was active when the tabs were saved, to select after restoring them.
    startup_active: Option<PathBuf>,
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
    mixer: Option<f32>,
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
        // What to open at startup: a file from the command line, or the tabs that were open
        // last time (each skipped if it is no longer there).
        let (startup_files, startup_active): (Vec<PathBuf>, Option<PathBuf>) =
            if let Some(file) = file {
                (vec![file], None)
            } else if settings.reopen_last_file {
                let mut files = settings.open_files.clone();
                // Settings written by an older version only know the last file.
                if files.is_empty() {
                    files.extend(settings.last_file.clone());
                }
                files.retain(|path| path.is_file());
                (files, settings.active_file.clone())
            } else {
                (Vec::new(), None)
            };
        let mut view = ViewState::default();
        settings.apply_to_view(&mut view);
        let last_window = settings.window;
        let panel_sizes = PanelSizes {
            grid: settings.grid_height_chosen,
            mixer: settings.mixer_height_chosen,
            track_panel: settings.track_panel_width,
            edit_panel: settings.edit_panel_width,
        };
        let mut app = GuitarScoreApp {
            view,
            fonts: Fonts::bundled().map_err(|e| e.to_string()),
            options: LayoutOptions::default(),
            tabs: Vec::new(),
            active: 0,
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
            saving_tab: None,
            closing_window: false,
            may_close: false,
            recovery_dir,
            recovery_searched: false,
            recoverable: Vec::new(),
            shortcuts: None,
            history: None,
            new_song_dialog: None,
            pending_opens: VecDeque::new(),
            startup_active: None,
            settings_window: None,
            settings,
            settings_path,
            panel_sizes,
            last_window,
        };
        app.restore_tabs(startup_files, startup_active, ctx);
        app
    }

    /// The settings as they are now (including the view and window state).
    pub fn settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        settings.update_from_view(&self.view);
        settings.grid_height_chosen = self.panel_sizes.grid;
        settings.mixer_height_chosen = self.panel_sizes.mixer;
        settings.track_panel_width = self.panel_sizes.track_panel;
        settings.edit_panel_width = self.panel_sizes.edit_panel;
        settings.window = self.last_window;
        settings.soundfont_path = self.playback.soundfont().map(std::path::Path::to_path_buf);
        settings.prefer_synth = matches!(self.playback.output(), Output::Synth(_));
        settings.midi_port = self.playback.port().map(str::to_owned);
        // Remember every tab backed by a real file, and which one was active. Setting the
        // old single-file key too keeps older versions reopening something sensible.
        settings.open_files = self
            .tabs
            .iter()
            .filter_map(|tab| tab.document.reopen_path().map(std::path::Path::to_path_buf))
            .collect();
        settings.active_file = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.document.reopen_path())
            .map(std::path::Path::to_path_buf);
        settings.last_file = settings
            .active_file
            .clone()
            .or_else(|| settings.open_files.first().cloned());
        settings.store_shortcuts(&self.keymap);
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

    /// The active tab's index, if any.
    pub fn active_tab(&self) -> Option<usize> {
        self.tabs.get(self.active).map(|_| self.active)
    }

    /// The number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The open document (the active tab's).
    pub fn document(&self) -> Option<&Document> {
        self.tabs.get(self.active).map(|tab| &tab.document)
    }

    /// The document in `index` (for tests and inspection), if it exists.
    pub fn tab_document(&self, index: usize) -> Option<&Document> {
        self.tabs.get(index).map(|tab| &tab.document)
    }

    /// The open document, to change it (its editor and cursor).
    pub fn document_mut(&mut self) -> Option<&mut Document> {
        self.tabs.get_mut(self.active).map(|tab| &mut tab.document)
    }

    /// The shown track of the active tab (0-based).
    pub fn shown_track(&self) -> usize {
        self.tabs.get(self.active).map_or(0, |tab| tab.track)
    }

    /// Sets the active tab's shown track.
    pub fn set_shown_track(&mut self, track: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.track = track;
        }
    }

    /// The selected measure range of the active tab, if any.
    pub fn selection(&self) -> Option<crate::state::MeasureSelection> {
        self.tabs.get(self.active).and_then(|tab| tab.selection)
    }

    /// Sets the active tab's measure selection (the grid and the page share it).
    pub fn set_selection(&mut self, selection: Option<crate::state::MeasureSelection>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.selection = selection;
        }
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

    /// Opens the settings window (also used by the snapshot example).
    pub fn open_settings(&mut self) {
        self.open_settings_window();
    }

    /// The view state (theme, zoom, track).
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// The view state, to change it.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    /// The keymap in use.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
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
        let title = self.document().map_or_else(
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
        // Wayland never tells the position; the size and the mode are kept regardless.
        if let Some(window) =
            ctx.input(|input| WindowState::of_viewport(input.viewport(), self.last_window))
        {
            self.last_window = Some(window);
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
        // A count-in that has finished starts the song it was counting in.
        self.poll_count_in();
        // Keep a loop of the selected measures going.
        self.poll_loop();
        // A beat chosen while the SoundFont loaded plays once it is ready.
        match self.playback.poll_preview() {
            Ok(true) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Ok(false) => {}
            Err(error) => self.notice = Some(error),
        }
        // An entered note stops after a moment.
        if let Some(left) = self.playback.poll_audition() {
            ctx.request_repaint_after(left);
        }
        // The cursor follows the beat being played; the page scrolls with it.
        self.follow_playback();
        if self.playback.state() == gscore_play::PlayState::Playing {
            // A loop checks its end more often; the position moves every frame otherwise.
            let millis = if self.playback.loop_selection() {
                16
            } else {
                50
            };
            ctx.request_repaint_after(std::time::Duration::from_millis(millis));
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
        egui::Panel::bottom("status")
            .frame(style::chrome_frame(&ctx, 3))
            .show(ui, |ui| self.status_bar(ui));
        if self.view.show_track_grid && self.document().is_some() {
            // Until the user drags it, the grid is just tall enough for its tracks (up to
            // eight) and the master row, and follows when tracks come and go: a panel id per
            // fitting height makes egui take the new default. The mixer view has its own,
            // taller default and its own remembered height.
            let tracks = self.document().map_or(1, |d| d.song().track_count());
            let mixer = self.view.grid_mixer;
            let (fit, min, max, chosen, id) = if mixer {
                let chosen = self.panel_sizes.mixer;
                let id = match chosen {
                    Some(_) => "mixer_chosen".to_owned(),
                    None => "mixer_fit".to_owned(),
                };
                (
                    grid::MIXER_HEIGHT,
                    grid::MIXER_MIN_HEIGHT,
                    grid::MIXER_MAX_HEIGHT,
                    chosen,
                    id,
                )
            } else {
                let fit = grid::opening_height(tracks);
                let chosen = self.panel_sizes.grid;
                let id = match chosen {
                    Some(_) => "track_grid_chosen".to_owned(),
                    None => format!("track_grid_fit_{}", tracks.min(grid::OPEN_ROWS)),
                };
                (
                    fit,
                    grid::height_for(0),
                    grid::height_for(tracks),
                    chosen,
                    id,
                )
            };
            let panel = egui::Panel::bottom(id)
                .resizable(true)
                .min_size(min)
                .max_size(max)
                .default_size(chosen.unwrap_or(fit))
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| self.track_grid(ui));
                });
            let height = panel.response.rect.height();
            // A height other than the fitting one means the user dragged the edge.
            if chosen.is_some() || (height - fit).abs() > 1.0 {
                if mixer {
                    self.panel_sizes.mixer = Some(height);
                } else {
                    self.panel_sizes.grid = Some(height);
                }
            }
        }
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        if self.view.show_track_panel && self.document().is_some() {
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
        if self.view.show_edit_panel && self.document().is_some() {
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
        // The tab strip sits below the toolbar, between the side panels: because it is
        // created after them, it spans only the central column.
        if !self.tabs.is_empty() {
            egui::Panel::top("tabs")
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| self.tab_bar(ui));
                });
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
        self.show_new_song_dialog(&ctx);
        self.show_rename_window(&ctx);
        self.show_settings_window(&ctx);
        self.show_shortcuts(&ctx);
        self.show_history(&ctx);
        self.show_new_track_dialog(&ctx);
        self.show_shape_editors(&ctx);
        self.show_staff_dialogs(&ctx);
        // Every edit of this frame goes to the recovery journal (one per open document).
        let mut journal_error = None;
        for tab in &mut self.tabs {
            if let Some(error) = tab.document.record_changes() {
                journal_error.get_or_insert(error);
            }
        }
        if let Some(error) = journal_error {
            self.notice = Some(error);
        }
    }

    fn on_exit(&mut self) {
        self.save_settings();
        // A normal exit (unsaved changes were asked about): the recovery folders go.
        for tab in self.tabs.drain(..) {
            tab.document.close(true);
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
                x: Some(f32::NAN),
                y: Some(0.0),
                width: -10.0,
                height: 0.0,
                maximized: false,
                fullscreen: false,
            }),
            ..Settings::default()
        };
        let app =
            GuitarScoreApp::with_settings(&ctx, None, Keymap::default(), None, settings, None);
        assert!(app.last_window.is_none());
    }
}
