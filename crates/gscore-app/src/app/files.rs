//! Opening, saving and closing songs, and the questions around them: unsaved changes before
//! closing a tab or the window, and a save that would lose details.
//!
//! Every song opens in its own tab; opening another file never closes or disturbs the
//! current one. Saving always writes Guitar Pro 7 (`.gp`) with the safe-save procedure of
//! `gscore-session`. A file opened in another format is never overwritten: Save asks for
//! a new `.gp` file instead (AGENTS.md "Saving").

use std::path::{Path, PathBuf};

use gscore_model::SongTextField;
use gscore_session::FoundSession;

use super::GuitarScoreApp;
use super::alert::{AlertIcon, Answer, Reply, alert};
use crate::document::Document;
use crate::tasks::{Event, SaveFailure};

/// The path and format of a new, unsaved score: Save asks for a real `.gp` file.
const UNTITLED: &str = "Untitled.gp";

/// How many lost details a dialog lists before "and N more".
const SHOWN_LOSSES: usize = 8;

/// What to do once the unsaved changes of a tab are dealt with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Then {
    /// Close this tab (after its changes were saved or discarded).
    CloseTab(usize),
}

/// A question for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dialog {
    /// The tab has unsaved changes; save them before going on?
    Unsaved(Then),
    /// Saving to `path` would lose `details`; save anyway?
    Losses { path: PathBuf, details: Vec<String> },
    /// A document was not closed normally; restore it?
    Recover(Box<FoundSession>),
}

impl GuitarScoreApp {
    /// Opens the new-song dialog (its fields start as "New Song", 120 BPM).
    pub fn new_song(&mut self, ctx: &egui::Context) {
        self.new_song_dialog = Some(super::new_song::NewSongDialog::default());
        ctx.request_repaint();
    }

    /// Makes a new, unsaved one-track score with `title` and `tempo`, in a new tab.
    pub(super) fn create_new_song(&mut self, title: String, tempo: u32) {
        match gscore_model::Song::new_score() {
            Ok(mut song) => {
                if !title.is_empty() {
                    let _ = song.set_text(SongTextField::Title, &title);
                }
                let _ = song.set_tempo(tempo);
                self.push_tab(Document::unsaved(
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

    /// Opens `path` in the background, in a new tab (or shows it if it is already open).
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        // Never open the same file twice: show the tab that already has it.
        if let Some(index) = self.find_open_tab(&path) {
            self.switch_tab(index);
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            self.status = Some(format!("{name} is already open"));
            // Choosing it (Open Recent, a drop) still makes it the most recent file.
            self.settings.add_recent(path);
            self.save_settings();
            return;
        }
        // The same file is being opened right now; it will become a tab shortly.
        if self
            .tasks
            .opening_path()
            .is_some_and(|opening| same_file(opening, &path))
        {
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

    /// The tab that already has `path` open, if any (compared as files, not spellings).
    fn find_open_tab(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.document
                .reopen_path()
                .is_some_and(|open| same_file(open, path))
        })
    }

    /// Reopens the tabs that were open last time, one after another, then selects the tab
    /// that was active (if it was restored).
    pub(super) fn restore_tabs(
        &mut self,
        files: Vec<PathBuf>,
        active: Option<PathBuf>,
        ctx: &egui::Context,
    ) {
        self.startup_active = active;
        self.pending_opens = files.into();
        self.open_next_pending(ctx);
    }

    /// Starts the next startup open, or finishes the restore when none is left.
    fn open_next_pending(&mut self, ctx: &egui::Context) {
        let options = self.settings.recovery_options();
        while let Some(path) = self.pending_opens.pop_front() {
            // Settings written before this could list the same file twice: only one tab.
            if self.find_open_tab(&path).is_some() {
                continue;
            }
            // A spawn failure only skips this file; the next one still opens.
            if let Err(error) = self
                .tasks
                .open(path, self.recovery_dir.clone(), options, ctx)
            {
                self.notice = Some(error);
                continue;
            }
            return;
        }
        self.finish_restore();
    }

    /// Selects the tab that was active when the tabs were saved, if it is open now.
    fn finish_restore(&mut self) {
        let Some(active) = self.startup_active.take() else {
            return;
        };
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.document.reopen_path() == Some(active.as_path()))
        {
            self.switch_tab(index);
        }
    }

    /// Shows the open dialog.
    pub(super) fn ask_open(&mut self, ctx: &egui::Context) {
        if let Err(error) = self.tasks.pick_file(ctx) {
            self.notice = Some(error);
        }
    }

    /// Saves the active tab: to its own file if that is a `.gp` file, otherwise to a new one.
    pub fn save_song(&mut self, ctx: &egui::Context) {
        if self.document().is_none() {
            return;
        }
        self.save_tab(self.active, ctx);
    }

    /// Saves `index`'s document, asking for a `.gp` file when it has none.
    fn save_tab(&mut self, index: usize, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        self.saving_tab = Some(index);
        if tab.document.saves_in_place() {
            let path = tab.document.path().to_path_buf();
            self.start_save(path, false, ctx);
        } else {
            self.save_as(ctx);
        }
    }

    /// Asks where to save the tab being saved as a `.gp` file.
    pub(super) fn save_as(&mut self, ctx: &egui::Context) {
        let Some(index) = self.saving_tab else {
            return;
        };
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let stem = tab.document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        if let Err(error) = self.tasks.pick_save_file(format!("{stem}.gp"), ctx) {
            self.notice = Some(error);
            self.after_save = None;
            self.saving_tab = None;
        }
    }

    /// Asks where to export the shown track as PDF.
    pub(super) fn ask_export_pdf(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let stem = tab.document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        if let Err(error) = self.tasks.pick_pdf_file(format!("{stem}.pdf"), ctx) {
            self.notice = Some(error);
        }
    }

    /// Exports the active tab's shown track to `path` as PDF in the background, on pages
    /// with the song's page setup (whatever the view shows).
    fn start_pdf_export(&mut self, path: PathBuf, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song().clone();
        let options = self
            .options
            .clone()
            .with_page_setup(&song.page_setup())
            .on_pages();
        if let Err(error) = self.tasks.export_pdf(song, tab.track, options, path, ctx) {
            self.notice = Some(error);
        }
    }

    /// Saves the tab being saved to `path` in the background.
    fn start_save(&mut self, path: PathBuf, allow_losses: bool, ctx: &egui::Context) {
        let Some(index) = self.saving_tab else {
            return;
        };
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let song = tab.document.song().clone();
        let revision = tab.document.editor().revision();
        if let Err(error) = self.tasks.save(song, path, revision, allow_losses, ctx) {
            self.notice = Some(error);
            self.after_save = None;
            self.saving_tab = None;
        }
    }

    /// Handles finished background jobs.
    pub(super) fn handle_events(&mut self, ctx: &egui::Context) {
        for event in self.tasks.poll() {
            let an_open_finished = matches!(event, Event::Opened { .. } | Event::Failed(_));
            match event {
                Event::Picked(Some(path)) => self.open(path, ctx),
                Event::Picked(None) => {}
                Event::Opened { path, result } => match result {
                    Ok(opened) => {
                        let recent = path.clone();
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
                        self.push_tab(document);
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
                Event::PickedSave(None) => {
                    self.after_save = None;
                    self.saving_tab = None;
                }
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
                        if let Some(index) = self.saving_tab.take()
                            && let Some(tab) = self.tabs.get_mut(index)
                        {
                            tab.document.mark_saved(path, revision);
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
                        self.saving_tab = None;
                    }
                },
                Event::Failed(error) => {
                    self.notice = Some(error);
                    self.after_save = None;
                    self.saving_tab = None;
                }
            }
            // Restoring the tabs from last time opens them one after another.
            if an_open_finished {
                self.open_next_pending(ctx);
            }
        }
    }

    /// Keeps the window open when it is closed with unsaved changes, and asks about each
    /// changed tab in turn before letting it close.
    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if !requested || self.may_close || self.closing_window {
            return;
        }
        if let Some(index) = self.first_dirty_tab() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing_window = true;
            self.dialog = Some(Dialog::Unsaved(Then::CloseTab(index)));
        } else {
            self.may_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The first tab with unsaved changes, if any.
    fn first_dirty_tab(&self) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.document.is_dirty())
    }

    /// Makes `document` the active tab.
    pub(super) fn push_tab(&mut self, document: Document) {
        self.playback.song_changed();
        // Start loading the synthesizer's SoundFont now, not on the first note: it takes
        // seconds, and it does so on its own thread.
        self.playback.preload_synth();
        self.tabs.push(super::tab::Tab::new(document));
        self.active = self.tabs.len() - 1;
    }

    /// Closes `index`, asking first if it has unsaved changes.
    pub(crate) fn close_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if tab.document.is_dirty() {
            self.dialog = Some(Dialog::Unsaved(Then::CloseTab(index)));
        } else {
            self.remove_tab(index);
        }
    }

    /// Removes `index` (its changes were saved or discarded). The active tab follows the
    /// one that closed; closing the last tab leaves an empty workspace.
    fn remove_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let was_active = index == self.active;
        let tab = self.tabs.remove(index);
        tab.document.close(false);
        if self.tabs.is_empty() {
            self.active = 0;
            self.playback.song_changed();
            return;
        }
        if index < self.active {
            self.active = self.active.saturating_sub(1);
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        if was_active {
            self.playback.song_changed();
            self.playback.preload_synth();
        }
    }

    /// Goes on with what waited for unsaved changes to be dealt with.
    pub(super) fn go_on(&mut self, then: Then, ctx: &egui::Context) {
        let Then::CloseTab(index) = then;
        self.remove_tab(index);
        // Closing the window asks about each dirty tab in turn, then lets it close.
        if self.closing_window {
            if let Some(next) = self.first_dirty_tab() {
                self.dialog = Some(Dialog::Unsaved(Then::CloseTab(next)));
            } else {
                self.closing_window = false;
                self.may_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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
        let Then::CloseTab(index) = then;
        let title = self
            .tabs
            .get(index)
            .map_or_else(String::new, |tab| tab.document.display_title());
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
                self.save_tab(index, ctx);
            }
            Reply::Answer(0) => {
                self.dialog = None;
                self.go_on(then, ctx);
            }
            Reply::Answer(_) | Reply::Closed => {
                self.dialog = None;
                // A cancelled or dismissed question about closing the window leaves it open;
                // a fresh close asks again.
                self.closing_window = false;
            }
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
                self.saving_tab = None;
            }
            Reply::Open => {}
        }
    }
}

/// Whether `a` and `b` name the same file: the same path, or two spellings (relative vs
/// absolute, a symlink) of one file.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    matches!(
        (std::fs::canonicalize(a), std::fs::canonicalize(b)),
        (Ok(a), Ok(b)) if a == b
    )
}
