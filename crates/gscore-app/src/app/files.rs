//! Opening and saving files, and the questions around them: unsaved changes before closing
//! the window or opening another file, and a save that would lose details.
//!
//! Saving always writes Guitar Pro 7 (`.gp`) with the safe-save procedure of
//! `gscore-session`. A file opened in another format is never overwritten: Save asks for
//! a new `.gp` file instead (AGENTS.md "Saving").

use std::path::PathBuf;

use super::GuitarScoreApp;
use crate::document::Document;
use crate::tasks::{Event, SaveFailure};

/// How many lost details a dialog lists before "and N more".
const SHOWN_LOSSES: usize = 8;

/// What to do once unsaved changes are dealt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Then {
    /// Close the window.
    Close,
    /// Open this file.
    Open(PathBuf),
}

/// A question for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dialog {
    /// The song has unsaved changes; save them before going on?
    Unsaved(Then),
    /// Saving to `path` would lose `details`; save anyway?
    Losses { path: PathBuf, details: Vec<String> },
}

impl GuitarScoreApp {
    /// Opens `path` in the background, asking first if the open song has unsaved changes.
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.document.as_ref().is_some_and(Document::is_dirty) {
            self.dialog = Some(Dialog::Unsaved(Then::Open(path)));
            return;
        }
        if let Err(error) = self.tasks.open(path, ctx) {
            self.notice = Some(error);
        }
    }

    /// Shows the open dialog.
    pub(super) fn ask_open(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.tasks.pick_file(ctx) {
            self.notice = Some(error);
        }
    }

    /// Saves the song: to its own file if that is a `.gp` file, otherwise to a new one.
    pub fn save_song(&mut self, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        if document.saves_in_place() {
            let path = document.path().to_path_buf();
            self.start_save(path, false, ctx);
        } else {
            self.save_as(ctx);
        }
    }

    /// Asks where to save the song as a `.gp` file.
    pub(super) fn save_as(&mut self, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let stem = document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        if let Err(error) = self.tasks.pick_save_file(format!("{stem}.gp"), ctx) {
            self.notice = Some(error);
            self.after_save = None;
        }
    }

    /// Saves the current song to `path` in the background.
    fn start_save(&mut self, path: PathBuf, allow_losses: bool, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let song = document.song().clone();
        let revision = document.editor().revision();
        if let Err(error) = self.tasks.save(song, path, revision, allow_losses, ctx) {
            self.notice = Some(error);
            self.after_save = None;
        }
    }

    /// Handles finished background jobs.
    pub(super) fn handle_events(&mut self, ctx: &egui::Context) {
        for event in self.tasks.poll() {
            match event {
                Event::Picked(Some(path)) => self.open(path, ctx),
                Event::Picked(None) => {}
                Event::Opened { path, result } => match result {
                    Ok((song, format)) => {
                        self.playback.song_changed();
                        self.document = Some(Document::new(path, *song, format));
                        self.view.track = 0;
                        self.notice = None;
                        self.status = None;
                        self.shown_cursor = None;
                    }
                    Err(error) => self.notice = Some(error),
                },
                Event::PickedSave(Some(path)) => {
                    let path = if path.extension().is_some_and(|e| e == "gp") {
                        path
                    } else {
                        path.with_extension("gp")
                    };
                    self.start_save(path, false, ctx);
                }
                // Cancelled: whatever was waiting for the save does not happen.
                Event::PickedSave(None) => self.after_save = None,
                Event::Saved {
                    path,
                    revision,
                    result,
                } => match result {
                    Ok(message) => {
                        if let Some(document) = &mut self.document {
                            document.mark_saved(path, revision);
                        }
                        self.status = Some(message);
                        if let Some(then) = self.after_save.take() {
                            self.go_on(then, ctx);
                        }
                    }
                    Err(SaveFailure::Losses(details)) => {
                        self.dialog = Some(Dialog::Losses { path, details });
                    }
                    Err(SaveFailure::Other(error)) => {
                        self.notice = Some(format!("Not saved: {error}"));
                        self.after_save = None;
                    }
                },
                Event::Failed(error) => {
                    self.notice = Some(error);
                    self.after_save = None;
                }
            }
        }
    }

    /// Keeps the window open when it is closed with unsaved changes, and asks.
    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if requested && !self.may_close && self.document.as_ref().is_some_and(Document::is_dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Unsaved(Then::Close));
        }
    }

    /// Goes on with what waited for unsaved changes to be dealt with.
    fn go_on(&mut self, then: Then, ctx: &egui::Context) {
        match then {
            Then::Close => {
                self.may_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Then::Open(path) => {
                if let Err(error) = self.tasks.open(path, ctx) {
                    self.notice = Some(error);
                }
            }
        }
    }

    /// Shows the pending question, if any, as a modal window.
    pub(super) fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("gscore-dialog"));
        modal.show(ctx, |ui| {
            ui.set_max_width(420.0);
            match dialog {
                Dialog::Unsaved(then) => self.unsaved_dialog(ui, then),
                Dialog::Losses { path, details } => self.losses_dialog(ui, path, &details),
            }
        });
    }

    fn unsaved_dialog(&mut self, ui: &mut egui::Ui, then: Then) {
        let title = self
            .document
            .as_ref()
            .map_or_else(String::new, Document::display_title);
        ui.heading("Unsaved changes");
        ui.label(format!("“{title}” has changes that are not saved."));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.dialog = None;
                self.after_save = Some(then.clone());
                self.save_song(ui.ctx());
            }
            if ui.button("Discard changes").clicked() {
                self.dialog = None;
                self.go_on(then.clone(), ui.ctx());
            }
            if ui.button("Cancel").clicked() {
                self.dialog = None;
            }
        });
    }

    fn losses_dialog(&mut self, ui: &mut egui::Ui, path: PathBuf, details: &[String]) {
        ui.heading("Save as Guitar Pro 7?");
        ui.label("Guitar Pro 7 (.gp) cannot keep everything in this song:");
        for detail in details.iter().take(SHOWN_LOSSES) {
            ui.label(format!("• {detail}"));
        }
        if details.len() > SHOWN_LOSSES {
            ui.label(format!(
                "and {} more",
                details.len().saturating_sub(SHOWN_LOSSES)
            ));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save anyway").clicked() {
                self.dialog = None;
                self.start_save(path.clone(), true, ui.ctx());
            }
            if ui.button("Cancel").clicked() {
                self.dialog = None;
                self.after_save = None;
            }
        });
    }
}
