//! Opening and saving files, and the questions around them: unsaved changes before closing
//! the window or opening another file, and a save that would lose details.
//!
//! Saving always writes Guitar Pro 7 (`.gp`) with the safe-save procedure of
//! `gscore-session`. A file opened in another format is never overwritten: Save asks for
//! a new `.gp` file instead (AGENTS.md "Saving").

use std::path::PathBuf;

use gscore_session::FoundSession;

use super::GuitarScoreApp;
use super::alert::{AlertIcon, Answer, Reply, alert};
use crate::document::Document;
use crate::tasks::{Event, SaveFailure};

/// The path and format of a new, unsaved score: Save asks for a real `.gp` file.
const UNTITLED: &str = "Untitled.gp";

/// How many lost details a dialog lists before "and N more".
const SHOWN_LOSSES: usize = 8;

/// What to do once unsaved changes are dealt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Then {
    /// Close the window.
    Close,
    /// Open this file.
    Open(PathBuf),
    /// Start a new score.
    NewSong,
    /// Restore this crashed document.
    Restore(Box<FoundSession>),
}

/// A question for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dialog {
    /// The song has unsaved changes; save them before going on?
    Unsaved(Then),
    /// Saving to `path` would lose `details`; save anyway?
    Losses { path: PathBuf, details: Vec<String> },
    /// A document was not closed normally; restore it?
    Recover(Box<FoundSession>),
}

impl GuitarScoreApp {
    /// Starts a new score, asking first if the open song has unsaved changes.
    pub fn new_song(&mut self, ctx: &egui::Context) {
        if self.document.as_ref().is_some_and(Document::is_dirty) {
            self.dialog = Some(Dialog::Unsaved(Then::NewSong));
            return;
        }
        self.create_new_song();
        ctx.request_repaint();
    }

    /// Makes a new, unsaved one-track score the open document.
    fn create_new_song(&mut self) {
        match gscore_model::Song::new_score() {
            Ok(song) => {
                self.replace_document(Document::new(
                    PathBuf::from(UNTITLED),
                    song,
                    gscore_session::FileFormat::Gpx,
                ));
                self.status = Some("New song".to_owned());
                self.notice = None;
            }
            Err(error) => self.notice = Some(format!("Could not create a song: {error}")),
        }
    }

    /// Opens `path` in the background, asking first if the open song has unsaved changes.
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.document.as_ref().is_some_and(Document::is_dirty) {
            self.dialog = Some(Dialog::Unsaved(Then::Open(path)));
            return;
        }
        let options = self.settings.recovery_options();
        if let Err(error) = self
            .tasks
            .open(path, self.recovery_dir.clone(), options, ctx)
        {
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

    /// Asks where to export the shown track as PDF.
    pub(super) fn ask_export_pdf(&mut self, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let stem = document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        if let Err(error) = self.tasks.pick_pdf_file(format!("{stem}.pdf"), ctx) {
            self.notice = Some(error);
        }
    }

    /// Exports the shown track to `path` as PDF in the background, on pages with the
    /// song's page setup (whatever the view shows).
    fn start_pdf_export(&mut self, path: PathBuf, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let song = document.song().clone();
        let options = self
            .options
            .clone()
            .with_page_setup(&song.page_setup())
            .on_pages();
        if let Err(error) = self
            .tasks
            .export_pdf(song, self.view.track, options, path, ctx)
        {
            self.notice = Some(error);
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
                    Ok(opened) => {
                        let recent = path.clone();
                        self.settings.last_file = Some(recent.clone());
                        let (document, problem) = match opened.journal {
                            Ok(Some(journal)) => (
                                Document::with_journal(path, *opened.song, opened.format, journal),
                                None,
                            ),
                            Ok(None) => (Document::new(path, *opened.song, opened.format), None),
                            Err(error) => (
                                Document::new(path, *opened.song, opened.format),
                                Some(format!(
                                    "{error}. Edits are not protected against a crash; save often."
                                )),
                            ),
                        };
                        self.replace_document(document);
                        self.notice = problem;
                        self.status = None;
                        self.settings.add_recent(recent);
                        self.save_settings();
                    }
                    Err(error) => self.notice = Some(error),
                },
                Event::FoundRecoverable(sessions) => self.found_recoverable(sessions),
                Event::Recovered { session, result } => self.recovered(&session, result),
                Event::PickedSave(Some(path)) => {
                    let path = if path.extension().is_some_and(|e| e == "gp") {
                        path
                    } else {
                        path.with_extension("gp")
                    };
                    self.start_save(path, false, ctx);
                }
                Event::PickedSoundFont(Some(path)) => self.playback.use_soundfont(path),
                Event::PickedSoundFont(None) => {}
                // Cancelled: whatever was waiting for the save does not happen.
                Event::PickedSave(None) => self.after_save = None,
                Event::PickedPdf(Some(path)) => {
                    let path = if path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                    {
                        path
                    } else {
                        path.with_extension("pdf")
                    };
                    self.start_pdf_export(path, ctx);
                }
                Event::PickedPdf(None) => {}
                Event::Exported(Ok(message)) => self.status = Some(message),
                Event::Exported(Err(error)) => {
                    self.notice = Some(format!("Not exported: {error}"));
                }
                Event::Saved {
                    path,
                    revision,
                    result,
                } => match result {
                    Ok(message) => {
                        self.settings.last_file = Some(path.clone());
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

    /// Makes `document` the open one; the old one is closed (its recovery folder goes:
    /// its changes were saved or discarded before getting here).
    pub(super) fn replace_document(&mut self, document: Document) {
        self.playback.song_changed();
        // Start loading the synthesizer's SoundFont now, not on the first note: it takes
        // seconds, and it does so on its own thread.
        self.playback.preload_synth();
        self.edit_palette.forget_fields();
        if let Some(old) = self.document.replace(document) {
            old.close(false);
        }
        self.view.track = 0;
        self.view.selection = None;
        self.shown_cursor = None;
        self.history_stamps.clear();
        self.sync_history_stamps();
    }

    /// Goes on with what waited for unsaved changes to be dealt with.
    pub(super) fn go_on(&mut self, then: Then, ctx: &egui::Context) {
        match then {
            Then::Close => {
                self.may_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Then::Open(path) => {
                let options = self.settings.recovery_options();
                if let Err(error) = self
                    .tasks
                    .open(path, self.recovery_dir.clone(), options, ctx)
                {
                    self.notice = Some(error);
                }
            }
            Then::NewSong => self.create_new_song(),
            Then::Restore(session) => self.start_recovery(*session, ctx),
        }
    }

    /// Shows the pending question, if any, as a modal window.
    pub(super) fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        match dialog {
            Dialog::Unsaved(then) => self.unsaved_dialog(ctx, then),
            Dialog::Losses { path, details } => self.losses_dialog(ctx, path, &details),
            Dialog::Recover(session) => self.recover_dialog(ctx, *session),
        }
    }

    fn unsaved_dialog(&mut self, ctx: &egui::Context, then: Then) {
        let title = self
            .document
            .as_ref()
            .map_or_else(String::new, Document::display_title);
        let reply = alert(
            ctx,
            "Unsaved changes",
            AlertIcon::Warning,
            &format!("Save the changes to “{title}”?"),
            |ui| {
                ui.label("Your changes are lost if you go on without saving them.");
            },
            &[
                Answer {
                    text: "Discard changes",
                    main: false,
                },
                Answer {
                    text: "Cancel",
                    main: false,
                },
                Answer {
                    text: "Save",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Answer(2) => {
                self.dialog = None;
                self.after_save = Some(then);
                self.save_song(ctx);
            }
            Reply::Answer(0) => {
                self.dialog = None;
                self.go_on(then, ctx);
            }
            Reply::Answer(_) | Reply::Closed => self.dialog = None,
            Reply::Open => {}
        }
    }

    fn losses_dialog(&mut self, ctx: &egui::Context, path: PathBuf, details: &[String]) {
        let reply = alert(
            ctx,
            "Save as Guitar Pro 7",
            AlertIcon::Warning,
            "Some of the song cannot be kept",
            |ui| {
                ui.label("Guitar Pro 7 (.gp) has no place for:");
                for detail in details.iter().take(SHOWN_LOSSES) {
                    ui.label(format!("• {detail}"));
                }
                if details.len() > SHOWN_LOSSES {
                    ui.label(
                        egui::RichText::new(format!(
                            "and {} more",
                            details.len().saturating_sub(SHOWN_LOSSES)
                        ))
                        .weak(),
                    );
                }
            },
            &[
                Answer {
                    text: "Cancel",
                    main: false,
                },
                Answer {
                    text: "Save anyway",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Answer(1) => {
                self.dialog = None;
                self.start_save(path, true, ctx);
            }
            Reply::Answer(_) | Reply::Closed => {
                self.dialog = None;
                self.after_save = None;
            }
            Reply::Open => {}
        }
    }
}
