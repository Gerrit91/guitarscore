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
use super::tab::TabId;
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
    CloseTab(TabId),
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
        // The same file is being opened right now, or waits its turn: it becomes a tab
        // shortly.
        if self
            .tasks
            .opening_path()
            .into_iter()
            .chain(self.pending_opens.iter().map(PathBuf::as_path))
            .any(|opening| same_file(opening, &path))
        {
            return;
        }
        // One open at a time: this one starts when the running one is done.
        if self.tasks.open_progress().is_some() {
            self.pending_opens.push_back(path);
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

    /// Starts the next waiting open (the tabs from last time, or files chosen while another
    /// was opening), or finishes the restore when none is left.
    fn open_next_pending(&mut self, ctx: &egui::Context) {
        if self.tasks.open_progress().is_some() {
            return;
        }
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
        let Some(id) = self.tabs.get(self.active).map(|tab| tab.id) else {
            return;
        };
        self.save_tab(id, None, ctx);
    }

    /// The index of the tab `id`, if it is still open.
    fn tab_index(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Saves tab `id`'s document, asking for a `.gp` file when it has none, and does `then`
    /// once it is saved. While another save is running, nothing starts: its result could
    /// not tell the two apart.
    fn save_tab(&mut self, id: TabId, then: Option<Then>, ctx: &egui::Context) {
        let Some(tab) = self.tab_index(id).and_then(|index| self.tabs.get(index)) else {
            return;
        };
        if self.saving_tab.is_some() {
            self.status =
                Some("Another save is still running; try again when it is done".to_owned());
            self.closing_window = false;
            return;
        }
        self.saving_tab = Some(id);
        self.after_save = then;
        if tab.document.saves_in_place() {
            let path = tab.document.path().to_path_buf();
            self.start_save(path, false, ctx);
        } else {
            self.save_as(ctx);
        }
    }

    /// Asks where to save the tab being saved as a `.gp` file.
    pub(super) fn save_as(&mut self, ctx: &egui::Context) {
        let Some(tab) = self
            .saving_tab
            .and_then(|id| self.tab_index(id))
            .and_then(|index| self.tabs.get(index))
        else {
            self.abandon_save();
            return;
        };
        let stem = tab.document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        if let Err(error) = self.tasks.pick_save_file(format!("{stem}.gp"), ctx) {
            self.notice = Some(error);
            self.abandon_save();
        }
    }

    /// Asks where to export the active tab, in `format`.
    pub(super) fn ask_export(&mut self, format: crate::export::ExportFormat, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let stem = tab.document.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let name = format!("{stem}.{}", format.extension());
        if let Err(error) = self.tasks.pick_export_file(format, name, ctx) {
            self.notice = Some(error);
        }
    }

    /// Starts the export of the active tab in `format` to `path`, in the background.
    fn start_export(
        &mut self,
        format: crate::export::ExportFormat,
        path: PathBuf,
        ctx: &egui::Context,
    ) {
        use crate::export::ExportFormat;
        match format {
            ExportFormat::Pdf => self.start_pdf_export(path, ctx),
            ExportFormat::Midi => {
                let Some(tab) = self.tabs.get(self.active) else {
                    return;
                };
                let song = tab.document.song().clone();
                if let Err(error) = self.tasks.export_midi(song, path, ctx) {
                    self.notice = Some(error);
                }
            }
            ExportFormat::Wav => {
                let Some(tab) = self.tabs.get(self.active) else {
                    return;
                };
                let song = tab.document.song().clone();
                let audio = crate::tasks::AudioExport {
                    soundfont: self.playback.soundfont().map(Path::to_path_buf),
                    modelled: self.playback.modelled_guitars(),
                    sample_rate: self.playback.sample_rate(),
                    models: self.playback.string_models(),
                    chains: self.playback.chain_choices(),
                    bus: Some(self.playback.reverb_bus_choice()),
                };
                if let Err(error) = self.tasks.export_wav(song, path, audio, ctx) {
                    self.notice = Some(error);
                }
            }
            ExportFormat::Gp
            | ExportFormat::Gpx
            | ExportFormat::Gp5
            | ExportFormat::Gp4
            | ExportFormat::Gp3 => {
                let Some(tab) = self.tabs.get(self.active) else {
                    return;
                };
                let song = tab.document.song().clone();
                let Some(file_format) = format.score_format() else {
                    return;
                };
                if let Err(error) = self.tasks.export_score(song, file_format, path, ctx) {
                    self.notice = Some(error);
                }
            }
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
        // Printed, every empty staff may go (there is no cursor to keep).
        let options = gscore_layout::LayoutOptions {
            empty_staves: if self.view.hide_empty_staves {
                gscore_layout::EmptyStaves::Hidden
            } else {
                gscore_layout::EmptyStaves::Shown
            },
            ..options
        };
        // The tracks the score shows, as it shows them.
        let tracks = tab.shown_tracks();
        if let Err(error) = self.tasks.export_pdf(song, tracks, options, path, ctx) {
            self.notice = Some(error);
        }
    }

    /// Saves the tab being saved to `path` in the background.
    fn start_save(&mut self, path: PathBuf, allow_losses: bool, ctx: &egui::Context) {
        // The tab may have been closed meanwhile (while the file dialog was open).
        let Some(tab) = self
            .saving_tab
            .and_then(|id| self.tab_index(id))
            .and_then(|index| self.tabs.get(index))
        else {
            self.abandon_save();
            return;
        };
        let song = tab.document.song().clone();
        let revision = tab.document.editor().revision();
        if let Err(error) = self.tasks.save(song, path, revision, allow_losses, ctx) {
            self.notice = Some(error);
            self.abandon_save();
        }
    }

    /// The save did not happen (cancelled or failed): what waited for it does not happen
    /// either, and a window close that waited for it is called off (a fresh close asks
    /// again).
    fn abandon_save(&mut self) {
        self.after_save = None;
        self.saving_tab = None;
        self.closing_window = false;
    }

    /// Handles finished background jobs.
    pub(super) fn handle_events(&mut self, ctx: &egui::Context) {
        let was_opening = self.tasks.open_progress().is_some();
        for event in self.tasks.poll() {
            match event {
                Event::Picked(Some(picked)) => {
                    let path = self.tasks.remember_picked(&picked);
                    self.open(path, ctx);
                }
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
                Event::PickedSave(Some(picked)) => {
                    let path = if picked.path.extension().is_some_and(|e| e == "gp") {
                        picked.path
                    } else {
                        picked.path.with_extension("gp")
                    };
                    self.start_save(path, false, ctx);
                }
                Event::PickedSoundFont(Some(picked)) => self.playback.use_soundfont(picked.path),
                Event::PickedSoundFont(None) => {}
                Event::PickedImpulse {
                    kind,
                    place,
                    path: Some(path),
                } => self.use_impulse(kind, place, path),
                Event::PickedImpulse { path: None, .. } => {}
                Event::PickedBusImpulse(Some(path)) => self.use_bus_impulse(path),
                Event::PickedBusImpulse(None) => {}
                // Cancelled: whatever was waiting for the save does not happen.
                Event::PickedSave(None) => self.abandon_save(),
                Event::PickedExport { format, picked } => {
                    if let Some(picked) = picked {
                        let path = if picked
                            .path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()))
                        {
                            picked.path
                        } else {
                            picked.path.with_extension(format.extension())
                        };
                        self.start_export(format, path, ctx);
                    }
                }
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
                        if let Some(index) =
                            self.saving_tab.take().and_then(|id| self.tab_index(id))
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
                        self.abandon_save();
                    }
                },
                Event::Failed(error) => {
                    self.notice = Some(error);
                    self.abandon_save();
                }
            }
        }
        // The running open is done (opened, failed or cancelled): the next waiting one
        // starts.
        if was_opening && self.tasks.open_progress().is_none() {
            self.open_next_pending(ctx);
        }
    }

    /// Keeps the window open when it is closed with unsaved changes, and asks about each
    /// changed tab in turn before letting it close.
    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if !requested || self.may_close {
            return;
        }
        if let Some(id) = self.first_dirty_tab() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // Already asking (or saving for it, or another question is up): just stay open.
            if self.closing_window || self.dialog.is_some() || self.saving_tab.is_some() {
                return;
            }
            self.closing_window = true;
            self.dialog = Some(Dialog::Unsaved(Then::CloseTab(id)));
        } else {
            self.may_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The first tab with unsaved changes, if any.
    fn first_dirty_tab(&self) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| tab.document.is_dirty())
            .map(|tab| tab.id)
    }

    /// Makes `document` the active tab.
    pub(super) fn push_tab(&mut self, document: Document) {
        self.playback.song_changed();
        // Start loading the synthesizer's SoundFont now, not on the first note: it takes
        // seconds, and it does so on its own thread.
        self.playback.preload_synth();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.wrapping_add(1);
        self.tabs.push(super::tab::Tab::new(id, document));
        self.active = self.tabs.len() - 1;
    }

    /// Closes `index`, asking first if it has unsaved changes.
    pub(crate) fn close_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if tab.document.is_dirty() {
            // Another question is up (or this tab's save runs): that comes first.
            if self.dialog.is_none() && self.saving_tab != Some(tab.id) {
                self.dialog = Some(Dialog::Unsaved(Then::CloseTab(tab.id)));
            }
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
        let Then::CloseTab(id) = then;
        if let Some(index) = self.tab_index(id) {
            self.remove_tab(index);
        }
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
        let Then::CloseTab(id) = then;
        let title = self
            .tab_index(id)
            .and_then(|index| self.tabs.get(index))
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
                self.save_tab(id, Some(then), ctx);
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
                self.abandon_save();
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
