//! Work that must not block the UI thread (AGENTS.md rule 9): showing the file dialogs,
//! reading and importing a file, and saving one. Each job runs on its own thread and reports back through a
//! channel that the UI polls once per frame.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gscore_model::Song;
use gscore_session::{FileFormat, FoundSession, Journal, Recovered, SaveError, SaveOptions};

/// A finished background job.
#[derive(Debug)]
pub(crate) enum Event {
    /// The open dialog closed; `None` if it was cancelled.
    Picked(Option<PathBuf>),
    /// The SoundFont dialog closed; `None` if it was cancelled.
    PickedSoundFont(Option<PathBuf>),
    /// The save dialog closed; `None` if it was cancelled.
    PickedSave(Option<PathBuf>),
    /// A save finished or failed.
    Saved {
        /// The file.
        path: PathBuf,
        /// The editor revision that was saved.
        revision: u64,
        /// A message for the status bar, or why nothing was saved.
        result: Result<String, SaveFailure>,
    },
    /// A file was read and imported, or failed to.
    Opened {
        /// The file.
        path: PathBuf,
        /// The song (boxed: it is large and events are moved around), its format and its
        /// recovery journal, or why it could not be opened.
        result: Result<Opened, String>,
    },
    /// Documents a crash left behind, found at startup.
    FoundRecoverable(Vec<FoundSession>),
    /// A document was recovered (into a new journal), or could not be.
    Recovered {
        /// The recovery folder it came from (removed once recovered).
        session: Box<FoundSession>,
        /// The recovered document's parts, or why it failed.
        result: Result<Box<RecoveredDocument>, String>,
    },
    /// A job panicked (the panic hook wrote a crash report); the message says which.
    Failed(String),
}

/// An opened file.
#[derive(Debug)]
pub(crate) struct Opened {
    /// The song.
    pub(crate) song: Box<Song>,
    /// Its format.
    pub(crate) format: FileFormat,
    /// Its recovery journal, or why there is none (`Ok(None)`: no recovery directory).
    pub(crate) journal: Result<Option<Journal>, String>,
}

/// A recovered document, with the new journal it is recorded in.
#[derive(Debug)]
pub(crate) struct RecoveredDocument {
    /// The replayed editor and what came with it.
    pub(crate) recovered: Recovered,
    /// The new journal (already holding the replayed edits), or why there is none.
    pub(crate) journal: Result<Journal, String>,
}

/// Why a save did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveFailure {
    /// The format would lose these details; nothing was written. The user may save anyway.
    Losses(Vec<String>),
    /// Anything else; nothing was written, or the result could not be confirmed.
    Other(String),
}

/// Background jobs and the channel they report through.
#[derive(Debug)]
pub(crate) struct Tasks {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    /// Jobs started and not reported yet.
    running: usize,
}

impl Default for Tasks {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Tasks {
            sender,
            receiver,
            running: 0,
        }
    }
}

/// File name extensions offered by the file dialog.
const EXTENSIONS: &[&str] = &["gp", "gpx", "gp5", "gp4", "gp3"];

impl Tasks {
    /// Whether a job is still running.
    pub(crate) fn busy(&self) -> bool {
        self.running > 0
    }

    /// Reads and imports `path` in the background.
    /// With `recovery`, the document's edits are journaled in that directory.
    pub(crate) fn open(
        &mut self,
        path: PathBuf,
        recovery: Option<PathBuf>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("open", ctx, move || {
            let result = gscore_session::open_with_bytes(&path)
                .map(|(imported, bytes)| Opened {
                    song: Box::new(imported.song),
                    format: imported.format,
                    journal: recovery
                        .map(|root| Journal::start(&root, Some(&path), &bytes))
                        .transpose(),
                })
                .map_err(|e| e.to_string());
            Event::Opened { path, result }
        })
    }

    /// Looks for documents a crash left behind in `recovery`.
    pub(crate) fn find_recoverable(
        &mut self,
        recovery: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("recovery search", ctx, move || {
            Event::FoundRecoverable(gscore_session::find_sessions(&recovery))
        })
    }

    /// Recovers `session` into a new journal in `recovery`, then removes the old folder.
    pub(crate) fn recover(
        &mut self,
        session: FoundSession,
        recovery: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("recovery", ctx, move || {
            let result = gscore_session::recover(&session).map(|mut recovered| {
                // The replayed edits start the new journal, so a second crash loses
                // nothing either; only then does the old folder go.
                let journal =
                    Journal::start(&recovery, recovered.source.as_deref(), &recovered.base)
                        .map(|mut journal| {
                            let applied = recovered.editor.take_applied();
                            journal.record(&applied, recovered.editor.song());
                            (journal, applied.len())
                        })
                        .and_then(|(journal, count)| {
                            // Wait (on this background thread) until the new journal has every
                            // edit on disk before the old copy goes.
                            let started = std::time::Instant::now();
                            while journal.written() < count && journal.error().is_none() {
                                if started.elapsed() > std::time::Duration::from_secs(10) {
                                    return Err(
                                        "the new recovery journal is not written".to_owned()
                                    );
                                }
                                std::thread::sleep(std::time::Duration::from_millis(5));
                            }
                            journal.error().map_or(Ok(journal), Err)
                        });
                if journal.is_ok() {
                    let _ = gscore_session::discard(&session);
                }
                Box::new(RecoveredDocument { recovered, journal })
            });
            Event::Recovered {
                session: Box::new(session),
                result,
            }
        })
    }

    /// Shows the file dialog in the background.
    pub(crate) fn pick_file(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("file dialog", ctx, || {
            let picked = rfd::FileDialog::new()
                .set_title("Open a Guitar Pro file")
                .add_filter("Guitar Pro", EXTENSIONS)
                .pick_file();
            Event::Picked(picked)
        })
    }

    /// Shows the dialog choosing a SoundFont for the synthesizer in the background.
    pub(crate) fn pick_soundfont(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("SoundFont dialog", ctx, || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a SoundFont for the built-in synthesizer")
                .add_filter("SoundFont 2", &["sf2"])
                .pick_file();
            Event::PickedSoundFont(picked)
        })
    }

    /// Shows the save dialog in the background, suggesting `name`.
    pub(crate) fn pick_save_file(
        &mut self,
        name: String,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("save dialog", ctx, move || {
            let picked = rfd::FileDialog::new()
                .set_title("Save as Guitar Pro 7 (.gp)")
                .add_filter("Guitar Pro 7", &["gp"])
                .set_file_name(name)
                .save_file();
            Event::PickedSave(picked)
        })
    }

    /// Saves `song` (editor `revision`) to `path` as `.gp` in the background, with the
    /// safe-save procedure of `gscore-session`.
    pub(crate) fn save(
        &mut self,
        song: Song,
        path: PathBuf,
        revision: u64,
        allow_losses: bool,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("save", ctx, move || {
            let options = SaveOptions {
                format: FileFormat::Gp,
                allow_losses,
            };
            let result = match gscore_session::save(&song, &path, options) {
                Ok(report) => {
                    let name = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    let mut message = format!("Saved {name}");
                    if let Some(backup) = &report.backup {
                        message.push_str(&format!(
                            " (the original is kept as {})",
                            backup.file_name().map_or_else(
                                || backup.display().to_string(),
                                |n| n.to_string_lossy().into_owned()
                            )
                        ));
                    }
                    if !report.losses.is_empty() {
                        message.push_str(&format!(
                            "; {} detail(s) could not be kept",
                            report.losses.len()
                        ));
                    }
                    Ok(message)
                }
                Err(SaveError::Losses(losses)) => Err(SaveFailure::Losses(
                    losses.iter().map(ToString::to_string).collect(),
                )),
                Err(error) => Err(SaveFailure::Other(error.to_string())),
            };
            Event::Saved {
                path,
                revision,
                result,
            }
        })
    }

    /// The events of jobs that finished since the last call.
    pub(crate) fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // Empty or disconnected both mean: nothing more for now.
        while let Ok(event) = self.receiver.try_recv() {
            self.running = self.running.saturating_sub(1);
            events.push(event);
        }
        events
    }

    fn spawn(
        &mut self,
        name: &str,
        ctx: &egui::Context,
        job: impl FnOnce() -> Event + Send + 'static,
    ) -> Result<(), String> {
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let failed = format!("the {name} failed unexpectedly; a crash report was written");
        std::thread::Builder::new()
            .name(format!("gscore-{name}"))
            .spawn(move || {
                // A panic must still report back, or the UI would wait forever.
                let event = catch_unwind(AssertUnwindSafe(job)).unwrap_or(Event::Failed(failed));
                // The UI may be gone already when the app closes; nothing to do then.
                let _ = sender.send(event);
                ctx.request_repaint();
            })
            .map_err(|e| format!("cannot start the {name}: {e}"))?;
        self.running = self.running.saturating_add(1);
        Ok(())
    }
}
