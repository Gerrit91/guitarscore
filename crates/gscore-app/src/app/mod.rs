//! The application: menus, toolbar, the page with the edit cursor, and the status bar. It
//! holds state and delegates: opening and saving to [`Tasks`], editing to the document's
//! [`gscore_edit::TabEditor`], layout to [`Document`], painting to the canvas.
//!
//! Split by job: `bars` (menus, toolbar, status), `page` (the page view, cursor and
//! clicks), `editing` (keys and actions), `files` (open, save, unsaved changes).

mod bars;
mod editing;
mod files;
mod page;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use gscore_edit::{Cursor, Keymap};
use gscore_layout::{Fonts, LayoutOptions};

use crate::document::Document;
use crate::keys::KeyReader;
use crate::playback::Playback;
use crate::state::{ThemeChoice, ViewState};
use crate::tasks::Tasks;

pub(crate) use files::Then;

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
    /// The cursor and revision the view last scrolled to, to scroll only on a change.
    shown_cursor: Option<(Cursor, u64)>,
}

impl GuitarScoreApp {
    /// A new app with the built-in keymap; opens `file` in the background if given.
    pub fn new(ctx: &egui::Context, file: Option<PathBuf>) -> Self {
        GuitarScoreApp::with_keymap(ctx, file, Keymap::default())
    }

    /// A new app using `keymap`; opens `file` in the background if given.
    pub fn with_keymap(ctx: &egui::Context, file: Option<PathBuf>, keymap: Keymap) -> Self {
        crate::fonts::install(ctx);
        let mut app = GuitarScoreApp {
            view: ViewState::default(),
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
            shown_cursor: None,
        };
        if let Some(path) = file {
            app.open(path, ctx);
        }
        app
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
        if self.applied_theme != Some(self.view.theme) {
            // `set_theme` (not `set_visuals`) switches egui's own widgets too; the change
            // shows on the next frame, so ask for one.
            ctx.set_theme(match self.view.theme {
                ThemeChoice::Light => egui::Theme::Light,
                ThemeChoice::Dark => egui::Theme::Dark,
            });
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
        self.handle_events(&ctx);
        self.handle_close_request(&ctx);
        self.apply_theme(&ctx);
        // Keys first, before any widget sees them (a focused button would take Space).
        self.handle_keys(&ctx);
        self.apply_title(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            // A bug in drawing must not take the app down: contain it and tell the user.
            let drawn = catch_unwind(AssertUnwindSafe(|| self.page_view(ui)));
            if drawn.is_err() {
                self.notice = Some(
                    "Drawing the page failed unexpectedly; a crash report was written".to_owned(),
                );
            }
        });
        self.show_dialog(&ctx);
    }
}
