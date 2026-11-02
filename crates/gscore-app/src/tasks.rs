//! Work that must not block the UI thread (AGENTS.md rule 9): showing the file dialogs,
//! reading and importing a file, and saving one. Each job runs on its own thread and reports back through a
//! channel that the UI polls once per frame.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gscore_model::Song;
use gscore_session::{FileFormat, SaveError, SaveOptions};

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
        /// The song (boxed: it is large and events are moved around) and its format, or
        /// why it could not be opened.
        result: Result<(Box<Song>, FileFormat), String>,
    },
    /// A job panicked (the panic hook wrote a crash report); the message says which.
    Failed(String),
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
    pub(crate) fn open(&mut self, path: PathBuf, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("open", ctx, move || {
            let result = gscore_session::open(&path)
                .map(|imported| (Box::new(imported.song), imported.format))
                .map_err(|e| e.to_string());
            Event::Opened { path, result }
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
