//! Work that must not block the UI thread (AGENTS.md rule 9): showing the file dialogs,
//! reading and importing a file, and saving one. Each job runs on its own thread and reports back through a
//! channel that the UI polls once per frame.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use gscore_model::Song;
use gscore_session::{
    FileFormat, FoundSession, Journal, Recovered, RecoveryOptions, SaveError, SaveOptions,
};

/// A finished background job.
#[derive(Debug)]
pub(crate) enum Event {
    /// The open dialog closed; `None` if it was cancelled.
    Picked(Option<PathBuf>),
    /// The SoundFont dialog closed; `None` if it was cancelled.
    PickedSoundFont(Option<PathBuf>),
    /// The save dialog closed; `None` if it was cancelled.
    PickedSave(Option<PathBuf>),
    /// The PDF export dialog closed; `None` if it was cancelled.
    PickedPdf(Option<PathBuf>),
    /// A PDF export finished (a message for the status bar) or failed (why).
    Exported(Result<String, String>),
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

/// How far an open has got, shared with its worker thread.
#[derive(Debug, Clone, Default)]
pub(crate) struct OpenProgress {
    total: Arc<AtomicU64>,
    done: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
}

impl OpenProgress {
    fn set(&self, done: u64, total: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.done.store(done, Ordering::Relaxed);
    }

    /// The share of the file read, if its size is known.
    pub(crate) fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return None;
        }
        let done = self.done.load(Ordering::Relaxed).min(total);
        Some(done as f32 / total as f32)
    }

    /// All bytes are read: the importer is working now (which cannot report progress).
    pub(crate) fn importing(&self) -> bool {
        let total = self.total.load(Ordering::Relaxed);
        total > 0 && self.done.load(Ordering::Relaxed) >= total
    }

    /// Asks the worker to stop at its next chance.
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Background jobs and the channel they report through.
#[derive(Debug)]
pub(crate) struct Tasks {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    /// Jobs started and not reported yet.
    running: usize,
    /// The open that is running, for its progress bar.
    open: Option<OpenProgress>,
    /// The path of the file being opened, if one is (so it is not opened twice).
    open_path: Option<PathBuf>,
    /// The user cancelled the running open; its result is dropped.
    open_cancelled: bool,
}

impl Default for Tasks {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Tasks {
            sender,
            receiver,
            running: 0,
            open: None,
            open_path: None,
            open_cancelled: false,
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
        options: RecoveryOptions,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let progress = OpenProgress::default();
        self.open = Some(progress.clone());
        self.open_path = Some(path.clone());
        self.open_cancelled = false;
        let spawned = self.spawn("open", ctx, move || {
            let shared = progress.clone();
            // A panic still reports as this open's failure, so the open is known to be over
            // (and the next waiting one can start).
            let result = catch_unwind(AssertUnwindSafe(|| {
                gscore_session::open_with_progress(
                    &path,
                    |done, total| shared.set(done, total),
                    || shared.cancelled(),
                )
                .map(|(imported, bytes)| Opened {
                    song: Box::new(imported.song),
                    format: imported.format,
                    journal: recovery
                        .map(|root| {
                            Journal::start_with_options(&root, Some(&path), &bytes, options)
                        })
                        .transpose(),
                })
                .map_err(|e| e.to_string())
            }))
            .unwrap_or_else(|_| {
                Err("the open failed unexpectedly; a crash report was written".to_owned())
            });
            Event::Opened { path, result }
        });
        if spawned.is_err() {
            self.open = None;
            self.open_path = None;
        }
        spawned
    }

    /// The open that is running, for its progress bar.
    pub(crate) fn open_progress(&self) -> Option<&OpenProgress> {
        self.open.as_ref()
    }

    /// The path of the file being opened, if one is.
    pub(crate) fn opening_path(&self) -> Option<&std::path::Path> {
        self.open_path.as_deref()
    }

    /// Cancels the running open: its result will be dropped when it finishes.
    pub(crate) fn cancel_open(&mut self) {
        if let Some(progress) = &self.open {
            progress.cancel();
            self.open_cancelled = true;
        }
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
        options: RecoveryOptions,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("recovery", ctx, move || {
            let result = gscore_session::recover(&session).map(|mut recovered| {
                // The replayed edits start the new journal, so a second crash loses
                // nothing either; only then does the old folder go.
                let journal = Journal::start_with_options(
                    &recovery,
                    recovered.source.as_deref(),
                    &recovered.base,
                    options,
                )
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
                            return Err("the new recovery journal is not written".to_owned());
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

    /// Shows the PDF export dialog in the background, suggesting `name`.
    pub(crate) fn pick_pdf_file(
        &mut self,
        name: String,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("export dialog", ctx, move || {
            let picked = rfd::FileDialog::new()
                .set_title("Export as PDF")
                .add_filter("PDF", &["pdf"])
                .set_file_name(name)
                .save_file();
            Event::PickedPdf(picked)
        })
    }

    /// Lays out `track` of `song` on pages with `options` and writes it to `path` as PDF,
    /// in the background. The file is written next to `path` first and then moved there,
    /// so a failed export never leaves half a file.
    pub(crate) fn export_pdf(
        &mut self,
        song: Song,
        track: usize,
        options: gscore_layout::LayoutOptions,
        path: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("PDF export", ctx, move || {
            Event::Exported(export_pdf(&song, track, &options, &path))
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

    /// The events of jobs that finished since the last call. A cancelled open's result is
    /// dropped here.
    pub(crate) fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // Empty or disconnected both mean: nothing more for now.
        while let Ok(event) = self.receiver.try_recv() {
            self.running = self.running.saturating_sub(1);
            events.push(event);
        }
        let opened = events
            .iter()
            .any(|event| matches!(event, Event::Opened { .. }));
        if opened {
            self.open = None;
            self.open_path = None;
            if self.open_cancelled {
                self.open_cancelled = false;
                events.retain(|event| !matches!(event, Event::Opened { .. }));
            }
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

/// The work of [`Tasks::export_pdf`]: the status message, or why it failed.
fn export_pdf(
    song: &Song,
    track: usize,
    options: &gscore_layout::LayoutOptions,
    path: &std::path::Path,
) -> Result<String, String> {
    let fonts = gscore_layout::Fonts::bundled().map_err(|e| e.to_string())?;
    let layout =
        gscore_layout::layout_track(song, gscore_model::TrackIndex::new(track), options, &fonts)
            .map_err(|e| e.to_string())?;
    let pdf = gscore_layout::to_pdf(
        &layout,
        &gscore_layout::Theme::LIGHT,
        options.staff_space_mm,
        &gscore_layout::PdfFonts::bundled(),
        song.title(),
    )
    .map_err(|e| e.to_string())?;
    let name = path.file_name().map_or_else(
        || "export.pdf".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temp = path.with_file_name(format!(".{name}.gscore-tmp-{}", std::process::id()));
    std::fs::write(&temp, &pdf)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("cannot write {}: {e}", path.display())
        })?;
    let pages = layout.pages.len();
    Ok(format!(
        "Exported {name} ({pages} page{})",
        if pages == 1 { "" } else { "s" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_export_writes_pages_or_fails_without_leaving_a_file() {
        let dir = std::env::temp_dir().join(format!("gscore-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let song = gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], 80).unwrap();
        let options = gscore_layout::LayoutOptions::default()
            .with_page_setup(&song.page_setup())
            .on_pages();
        let path = dir.join("song.pdf");
        let message = export_pdf(&song, 0, &options, &path).unwrap();
        assert!(message.contains("pages"), "{message}");
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));

        let missing = dir.join("no such folder").join("song.pdf");
        assert!(export_pdf(&song, 0, &options, &missing).is_err());
        assert!(
            export_pdf(&song, 9, &options, &dir.join("other.pdf")).is_err(),
            "no track 10"
        );
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1, "only the exported file, no temporary one");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
