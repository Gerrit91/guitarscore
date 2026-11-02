//! Crash recovery in the app: at the start, documents a crash left in the recovery
//! directory are offered one by one (Restore, Discard, Later); a restored document opens
//! with its edits and undo history, unsaved.

use std::path::PathBuf;

use gscore_session::FoundSession;

use super::GuitarScoreApp;
use super::files::{Dialog, Then};
use crate::document::Document;
use crate::tasks::RecoveredDocument;

impl GuitarScoreApp {
    /// Looks for crashed documents once, in the background.
    pub(super) fn search_recoverable(&mut self, ctx: &egui::Context) {
        if self.recovery_searched {
            return;
        }
        self.recovery_searched = true;
        if let Some(dir) = self.recovery_dir.clone()
            && let Err(error) = self.tasks.find_recoverable(dir, ctx)
        {
            self.notice = Some(error);
        }
    }

    /// The search finished: ask about the first one.
    pub(super) fn found_recoverable(&mut self, mut sessions: Vec<FoundSession>) {
        // Asked newest first; `pop` takes from the end.
        sessions.reverse();
        self.recoverable = sessions;
        self.ask_next_recovery();
    }

    /// Asks about the next crashed document, unless another question is open.
    fn ask_next_recovery(&mut self) {
        if self.dialog.is_none()
            && let Some(session) = self.recoverable.pop()
        {
            self.dialog = Some(Dialog::Recover(Box::new(session)));
        }
    }

    /// Restores `session` in the background.
    pub(super) fn start_recovery(&mut self, session: FoundSession, ctx: &egui::Context) {
        let Some(dir) = self.recovery_dir.clone() else {
            return;
        };
        if let Err(error) = self.tasks.recover(session, dir, ctx) {
            self.notice = Some(error);
        }
    }

    /// A recovery finished: open the document, or say why not (the folder stays).
    pub(super) fn recovered(
        &mut self,
        session: &FoundSession,
        result: Result<Box<RecoveredDocument>, String>,
    ) {
        match result {
            Ok(document) => {
                let RecoveredDocument { recovered, journal } = *document;
                let (journal, problem) = match journal {
                    Ok(journal) => (Some(journal), None),
                    Err(error) => (
                        None,
                        Some(format!(
                            "{error}. The recovered song is not protected against another \
                             crash; save it soon."
                        )),
                    ),
                };
                // Without a file name, a format other than `.gp` makes Save ask for one.
                let (path, format) = match &recovered.source {
                    Some(path) => (path.clone(), recovered.format),
                    None => (
                        PathBuf::from("Recovered.gp"),
                        gscore_session::FileFormat::Gpx,
                    ),
                };
                let mut status = format!("Recovered {} edit(s), not saved yet", recovered.replayed);
                if recovered.lost > 0 {
                    status.push_str(&format!("; {} could not be recovered", recovered.lost));
                }
                if recovered.from_snapshot {
                    status.push_str("; undo starts at the last snapshot");
                }
                self.replace_document(Document::recovered(path, recovered.editor, format, journal));
                self.status = Some(status);
                self.notice = problem;
            }
            Err(error) => {
                self.notice = Some(format!(
                    "Could not recover {}: {error}. The recovery files stay in {}.",
                    describe(session),
                    session.dir.display()
                ));
            }
        }
        self.ask_next_recovery();
    }

    /// The question about one crashed document.
    pub(super) fn recover_dialog(&mut self, ui: &mut egui::Ui, session: FoundSession) {
        ui.heading("Recover unsaved changes?");
        ui.label(format!(
            "GuitarScore did not close normally while {} had {} unsaved edit(s).",
            describe(&session),
            session.edits
        ));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Restore").clicked() {
                self.dialog = None;
                if self.document.as_ref().is_some_and(Document::is_dirty) {
                    self.dialog = Some(Dialog::Unsaved(Then::Restore(Box::new(session.clone()))));
                } else {
                    self.start_recovery(session.clone(), ui.ctx());
                }
            }
            if ui.button("Discard").clicked() {
                self.dialog = None;
                // File work off the UI thread; if it fails, the folder is offered again.
                let discarded = session.clone();
                let _ = std::thread::Builder::new()
                    .name("gscore-discard".to_owned())
                    .spawn(move || gscore_session::discard(&discarded));
                self.ask_next_recovery();
            }
            if ui.button("Later").clicked() {
                // Kept for the next start.
                self.dialog = None;
                self.ask_next_recovery();
            }
        });
    }
}

/// The document's file name, for the question.
fn describe(session: &FoundSession) -> String {
    session
        .source
        .as_deref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || "a new song".to_owned(),
            |name| format!("“{}”", name.to_string_lossy()),
        )
}
