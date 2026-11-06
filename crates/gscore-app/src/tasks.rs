//! Work that must not block the UI thread (AGENTS.md rule 9): showing the file dialogs,
//! reading and importing a file, and saving one. Each job runs on its own thread and reports back through a
//! channel that the UI polls once per frame.

#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use gscore_model::Song;
use gscore_play::{Sequence, to_smf};
use gscore_session::{
    FileFormat, FoundSession, Journal, Recovered, RecoveryOptions, SaveError, SaveOptions,
};

/// A finished background job.
#[derive(Debug)]
pub(crate) enum Event {
    /// The open dialog closed; `None` if it was cancelled.
    Picked(Option<Picked>),
    /// The SoundFont dialog closed; `None` if it was cancelled.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PickedSoundFont(Option<Picked>),
    /// The save dialog closed; `None` if it was cancelled.
    PickedSave(Option<Picked>),
    /// The impulse response dialog closed (for place `place` of `track`'s chain as `kind`);
    /// `None` if it was cancelled.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PickedImpulse {
        track: usize,
        kind: gscore_play::StringKind,
        place: usize,
        path: Option<PathBuf>,
    },
    /// The impulse response dialog of the mixer's reverb bus closed; `None` if cancelled.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PickedBusImpulse(Option<PathBuf>),
    /// The export dialog closed; `None` if it was cancelled.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PickedExport {
        /// The chosen export format.
        format: crate::export::ExportFormat,
        /// The file the user chose.
        picked: Option<Picked>,
    },
    /// An export finished (a message for the status bar) or failed (why).
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

/// A file the user picked. A browser hands the bytes over with the name (there is no
/// path), so they travel together; a native dialog gives only a path, and the open reads
/// the file in the background.
#[derive(Debug)]
pub(crate) struct Picked {
    /// The path (native) or file name (browser).
    pub(crate) path: PathBuf,
    /// The file's bytes, when the picker already read them (browser).
    #[cfg(target_arch = "wasm32")]
    pub(crate) bytes: Vec<u8>,
}

impl Picked {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn from_path(path: PathBuf) -> Self {
        Picked { path }
    }
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
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
    /// Browsers hand over a picked file's bytes with its name; they wait here until the
    /// open that `open` starts runs.
    #[cfg(target_arch = "wasm32")]
    picked_bytes: HashMap<PathBuf, Vec<u8>>,
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
            #[cfg(target_arch = "wasm32")]
            picked_bytes: HashMap::new(),
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
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Imports a file the browser already read. A wasm instance cannot block on a thread,
    /// so the import runs here; it is the one step that briefly holds the UI.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn open(
        &mut self,
        path: PathBuf,
        _recovery: Option<PathBuf>,
        _options: RecoveryOptions,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let Some(bytes) = self.picked_bytes.remove(&path) else {
            return Err(format!(
                "cannot read {}: in the browser, choose the file again",
                path.display()
            ));
        };
        self.open_bytes(path, bytes, ctx)
    }

    /// Imports bytes read by a browser drop or picker.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn open_bytes(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let source = path.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            gscore_session::open_bytes(&source, bytes).map(|(imported, _)| Opened {
                song: Box::new(imported.song),
                format: imported.format,
                journal: Ok(None),
            })
        }))
        .map(|result| result.map_err(|error| error.to_string()))
        .unwrap_or_else(|_| {
            Err("the open failed unexpectedly; a crash report was written".to_owned())
        });
        self.deliver(Event::Opened { path, result }, ctx);
        Ok(())
    }

    /// Remembers a picked file's bytes until the open starts (a browser picker always
    /// hands them over; native pickers give only a path) and returns its path.
    pub(crate) fn remember_picked(&mut self, picked: &Picked) -> PathBuf {
        #[cfg(target_arch = "wasm32")]
        {
            self.picked_bytes
                .insert(picked.path.clone(), picked.bytes.clone());
        }
        picked.path.clone()
    }

    /// Reads a dropped browser file and opens it.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn open_dropped(
        &mut self,
        file: egui::DroppedFileHandle,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let path = file.path().to_path_buf();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let event = match file.bytes_async().await {
                Ok(bytes) => match catch_unwind(AssertUnwindSafe(|| {
                    gscore_session::open_bytes(&path, bytes).map(|(imported, _)| Opened {
                        song: Box::new(imported.song),
                        format: imported.format,
                        journal: Ok(None),
                    })
                })) {
                    Ok(Ok(opened)) => Event::Opened {
                        path,
                        result: Ok(opened),
                    },
                    Ok(Err(error)) => Event::Opened {
                        path,
                        result: Err(error.to_string()),
                    },
                    Err(_) => Event::Failed(
                        "the open failed unexpectedly; a crash report was written".to_owned(),
                    ),
                },
                Err(error) => Event::Opened {
                    path,
                    result: Err(error),
                },
            };
            let _ = sender.send(event);
            ctx.request_repaint();
        });
        self.running = self.running.saturating_add(1);
        Ok(())
    }

    /// Hands an event to the UI as if a background job had finished.
    #[cfg(target_arch = "wasm32")]
    fn deliver(&mut self, event: Event, ctx: &egui::Context) {
        let _ = self.sender.send(event);
        self.running = self.running.saturating_add(1);
        ctx.request_repaint();
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
                    let started = crate::Instant::now();
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
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_file(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("file dialog", ctx, || {
            let picked = rfd::FileDialog::new()
                .set_title("Open a Guitar Pro file")
                .add_filter("Guitar Pro", EXTENSIONS)
                .pick_file()
                .map(Picked::from_path);
            Event::Picked(picked)
        })
    }

    /// Shows the dialog choosing a SoundFont for the synthesizer in the background.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_soundfont(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("SoundFont dialog", ctx, || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a SoundFont for the built-in synthesizer")
                .add_filter("SoundFont 2", &["sf2"])
                .pick_file()
                .map(Picked::from_path);
            Event::PickedSoundFont(picked)
        })
    }

    /// Shows the dialog for an impulse response (a WAV file) for place `place` of `kind`'s
    /// chain, in the background.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_impulse(
        &mut self,
        track: usize,
        kind: gscore_play::StringKind,
        place: usize,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("impulse response dialog", ctx, move || {
            let path = rfd::FileDialog::new()
                .set_title("Choose an impulse response (WAV)")
                .add_filter("WAV audio", &["wav"])
                .pick_file();
            Event::PickedImpulse {
                track,
                kind,
                place,
                path,
            }
        })
    }

    /// Shows the impulse response dialog for the mixer's reverb bus.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_bus_impulse(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.spawn("impulse response dialog", ctx, move || {
            let path = rfd::FileDialog::new()
                .set_title("Choose a room's impulse response (WAV)")
                .add_filter("WAV audio", &["wav"])
                .pick_file();
            Event::PickedBusImpulse(path)
        })
    }

    /// The reverb bus's impulse response cannot be chosen in the browser build yet.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pick_bus_impulse(&mut self, _ctx: &egui::Context) -> Result<(), String> {
        Err("loading impulse responses is not available in the browser build yet".to_owned())
    }

    /// Choosing an impulse response is not available in the browser build yet.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pick_impulse(
        &mut self,
        _track: usize,
        _kind: gscore_play::StringKind,
        _place: usize,
        _ctx: &egui::Context,
    ) -> Result<(), String> {
        Err("loading impulse responses is not available in the browser build yet".to_owned())
    }

    /// Shows the save dialog in the background, suggesting `name`.
    #[cfg(not(target_arch = "wasm32"))]
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
                .save_file()
                .map(Picked::from_path);
            Event::PickedSave(picked)
        })
    }

    /// Shows the export dialog in the background, suggesting `name` and offering
    /// `format`'s file type.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_export_file(
        &mut self,
        format: crate::export::ExportFormat,
        name: String,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("export dialog", ctx, move || {
            let picked = rfd::FileDialog::new()
                .set_title(format!("Export as {}", format.label()))
                .add_filter(format.label(), &[format.extension()])
                .set_file_name(name)
                .save_file()
                .map(Picked::from_path);
            Event::PickedExport { format, picked }
        })
    }

    /// The browser's open dialog. The bytes arrive with the picker's answer, so the open
    /// does not have to read anything later.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pick_file(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let handle = rfd::AsyncFileDialog::new()
                .set_title("Open a Guitar Pro file")
                .add_filter("Guitar Pro", EXTENSIONS)
                .pick_file()
                .await;
            let event = match handle {
                Some(handle) => {
                    let name = handle.file_name();
                    match read_web_file(&handle).await {
                        Ok(bytes) => Event::Picked(Some(Picked {
                            path: PathBuf::from(name),
                            bytes,
                        })),
                        Err(error) => Event::Failed(format!("cannot read {name}: {error}")),
                    }
                }
                None => Event::Picked(None),
            };
            let _ = sender.send(event);
            repaint.request_repaint();
        });
        self.running = self.running.saturating_add(1);
        Ok(())
    }

    /// The browser's SoundFont dialog (the site ships its own, so it is not offered).
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn pick_soundfont(&mut self, _ctx: &egui::Context) -> Result<(), String> {
        Err("choosing a SoundFont is not available in the browser build yet".to_owned())
    }

    /// A browser has no save dialog: a save downloads the file under the suggested
    /// name (the browser decides where, usually the downloads folder, and may ask).
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pick_save_file(
        &mut self,
        name: String,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.deliver(
            Event::PickedSave(Some(Picked {
                path: PathBuf::from(name),
                bytes: Vec::new(),
            })),
            ctx,
        );
        Ok(())
    }

    /// Exporting is not available in the browser build yet.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pick_export_file(
        &mut self,
        _format: crate::export::ExportFormat,
        _name: String,
        _ctx: &egui::Context,
    ) -> Result<(), String> {
        Err("exporting files is not available in the browser build yet".to_owned())
    }

    /// Lays out `tracks` of `song` together (one track, or several stacked in every
    /// system) on pages with `options` and writes them to `path` as PDF, in the background. The file is written next to `path` first and then moved there,
    /// so a failed export never leaves half a file.
    pub(crate) fn export_pdf(
        &mut self,
        song: Song,
        tracks: Vec<usize>,
        options: gscore_layout::LayoutOptions,
        path: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("PDF export", ctx, move || {
            Event::Exported(export_pdf(&song, &tracks, &options, &path))
        })
    }

    /// Converts `song` to a Guitar Pro file of `format` at `path`, in the background.
    pub(crate) fn export_score(
        &mut self,
        song: Song,
        format: FileFormat,
        path: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("score export", ctx, move || {
            Event::Exported(export_score(&song, format, &path))
        })
    }

    /// Writes every track of `song` as a Standard MIDI File at `path`, in the background.
    pub(crate) fn export_midi(
        &mut self,
        song: Song,
        path: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        self.spawn("MIDI export", ctx, move || {
            Event::Exported(export_midi(&song, &path))
        })
    }

    /// Renders every track of `song` to a WAV file at `path` as `audio` says, in the
    /// background.
    pub(crate) fn export_wav(
        &mut self,
        song: Song,
        path: PathBuf,
        audio: AudioExport,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (song, path, audio, ctx);
            Err("exporting audio is not available in the browser build yet".to_owned())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.spawn("WAV export", ctx, move || {
                Event::Exported(export_wav(&song, &path, &audio))
            })
        }
    }

    /// Saves `song` (editor `revision`) to `path` as `.gp` in the background, with the
    /// safe-save procedure of `gscore-session`.
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Saves `song` by downloading it: the bytes are built in memory (the same read-back
    /// and loss check as a file save), then written to the browser's save handle.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn save(
        &mut self,
        song: Song,
        path: PathBuf,
        revision: u64,
        allow_losses: bool,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let sender = self.sender.clone();
        let repaint = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let options = SaveOptions {
                format: FileFormat::Gp,
                allow_losses,
            };
            let name = path.file_name().map_or_else(
                || "song.gp".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
            let result = match gscore_session::save_bytes(&song, options) {
                Ok((bytes, losses)) => match download(&name, &bytes) {
                    Ok(()) => {
                        let mut message = format!("Saved {name}");
                        if !losses.is_empty() {
                            message.push_str(&format!(
                                "; {} detail(s) could not be kept",
                                losses.len()
                            ));
                        }
                        Ok(message)
                    }
                    Err(error) => Err(SaveFailure::Other(format!(
                        "cannot download {name}: {error}"
                    ))),
                },
                Err(SaveError::Losses(losses)) => Err(SaveFailure::Losses(
                    losses.iter().map(ToString::to_string).collect(),
                )),
                Err(error) => Err(SaveFailure::Other(error.to_string())),
            };
            let _ = sender.send(Event::Saved {
                path,
                revision,
                result,
            });
            repaint.request_repaint();
        });
        self.running = self.running.saturating_add(1);
        Ok(())
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

    #[cfg(not(target_arch = "wasm32"))]
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

    /// A wasm instance has no threads: the job runs here and reports through the same
    /// channel on the next poll. (It is used only by the recovery and PDF jobs, which the
    /// browser build does not start.)
    #[cfg(target_arch = "wasm32")]
    fn spawn(
        &mut self,
        name: &str,
        ctx: &egui::Context,
        job: impl FnOnce() -> Event + 'static,
    ) -> Result<(), String> {
        let failed = format!("the {name} failed unexpectedly; a crash report was written");
        let event = catch_unwind(AssertUnwindSafe(job)).unwrap_or(Event::Failed(failed));
        self.deliver(event, ctx);
        Ok(())
    }
}

/// Hands `bytes` to the browser as a download named `name`. (rfd's web save shows its own
/// "click here to download" overlay and reports success even when it is cancelled, so the
/// app would mark an undownloaded song as saved.)
#[cfg(target_arch = "wasm32")]
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let fail = |what: &str| format!("the browser {what}");
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/octet-stream");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| fail("could not prepare the file"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| fail("could not prepare the file"))?;
    let window = web_sys::window().ok_or_else(|| fail("window is missing"))?;
    let anchor = window
        .document()
        .and_then(|document| document.create_element("a").ok())
        .and_then(|element| element.dyn_into::<web_sys::HtmlAnchorElement>().ok())
        .ok_or_else(|| fail("could not start the download"))?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    // The download has started from the URL; free it a moment later (revoking at once can
    // cancel it in some browsers).
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000);
    Ok(())
}

/// Reads a file the browser's picker returned, without the panic on I/O that rfd's own
/// `FileHandle::read` has.
#[cfg(target_arch = "wasm32")]
async fn read_web_file(handle: &rfd::FileHandle) -> Result<Vec<u8>, String> {
    let promise = handle.inner().array_buffer();
    let buffer = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|_| "the browser could not read the file".to_owned())?;
    let array = js_sys::Uint8Array::new(&buffer);
    let mut bytes = vec![0_u8; array.length() as usize];
    array.copy_to(&mut bytes);
    Ok(bytes)
}

/// The work of [`Tasks::export_pdf`]: the status message, or why it failed.
fn export_pdf(
    song: &Song,
    tracks: &[usize],
    options: &gscore_layout::LayoutOptions,
    path: &std::path::Path,
) -> Result<String, String> {
    let fonts = gscore_layout::Fonts::bundled().map_err(|e| e.to_string())?;
    let indices: Vec<gscore_model::TrackIndex> = tracks
        .iter()
        .copied()
        .map(gscore_model::TrackIndex::new)
        .collect();
    let first = *indices
        .first()
        .ok_or_else(|| "there is no track to export".to_owned())?;
    let layout = gscore_layout::layout_tracks(song, &indices, first, options, &fonts)
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

/// The work of [`Tasks::export_score`]: the status message, or why it failed.
fn export_score(song: &Song, format: FileFormat, path: &std::path::Path) -> Result<String, String> {
    let report = gscore_session::export(song, path, format).map_err(|e| e.to_string())?;
    let name = path.file_name().map_or_else(
        || "export".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(match report.losses.len() {
        0 => format!("Exported {name}"),
        count => format!(
            "Exported {name}; {count} detail{} of the song {} not kept by this format",
            if count == 1 { "" } else { "s" },
            if count == 1 { "was" } else { "were" }
        ),
    })
}

/// The work of [`Tasks::export_midi`]: the status message, or why it failed.
fn export_midi(song: &Song, path: &std::path::Path) -> Result<String, String> {
    // Every track goes into the file: muting and soloing are for listening in the app, and
    // a sequence would leave muted (or unsoloed) tracks out.
    let mut song = song.clone();
    for index in 0..song.track_count() {
        let track = gscore_model::TrackIndex::new(index);
        let _ = song
            .set_track_mute(track, false)
            .map_err(|e| e.to_string())?;
        let _ = song
            .set_track_solo(track, false)
            .map_err(|e| e.to_string())?;
    }
    let sequence = Sequence::new(&song, None).map_err(|e| e.to_string())?;
    let bytes = to_smf(&sequence, song.title()).map_err(|e| e.to_string())?;
    std::fs::write(path, &bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let name = path.file_name().map_or_else(
        || "export.mid".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(format!(
        "Exported {name} ({} track{}, {} measure{})",
        sequence.tracks.len(),
        if sequence.tracks.len() == 1 { "" } else { "s" },
        sequence.measures.len(),
        if sequence.measures.len() == 1 {
            ""
        } else {
            "s"
        }
    ))
}

/// How audio is exported: the SoundFont (or one found on disk), whether guitars and basses
/// play on the modelled strings and how those sound, and the sample rate (`None`: the
/// WAV default).
#[derive(Debug, Clone, Default)]
pub(crate) struct AudioExport {
    pub(crate) soundfont: Option<PathBuf>,
    pub(crate) modelled: bool,
    pub(crate) sample_rate: Option<u32>,
    /// Each kind's default string model.
    pub(crate) models: Vec<(gscore_play::StringKind, gscore_guitar::StringModel)>,
    /// What the user chose to play each track (by track index), as in playback.
    pub(crate) engines: Vec<(usize, gscore_play::TrackEngine)>,
    /// The tracks' own tones and each kind's default chain.
    pub(crate) tones: crate::playback::ExportTones,
    /// The mixer's reverb bus, rendered as it is set.
    pub(crate) bus: Option<gscore_audio::ReverbBusChoice>,
}

/// The work of [`Tasks::export_wav`]: the status message, or why it failed.
fn export_wav(song: &Song, path: &std::path::Path, audio: &AudioExport) -> Result<String, String> {
    let soundfont = audio
        .soundfont
        .clone()
        .or_else(gscore_audio::find_soundfont)
        .ok_or_else(|| "no SoundFont found; choose one in Settings › Audio I/O".to_owned())?;
    let bank = gscore_audio::SoundBank::open(&soundfont).map_err(|e| e.to_string())?;
    // Every track goes into the file, as in the MIDI export.
    let mut song = song.clone();
    for index in 0..song.track_count() {
        let track = gscore_model::TrackIndex::new(index);
        let _ = song
            .set_track_mute(track, false)
            .map_err(|e| e.to_string())?;
        let _ = song
            .set_track_solo(track, false)
            .map_err(|e| e.to_string())?;
    }
    let sequence = crate::playback::with_engines(
        Sequence::new(&song, None).map_err(|e| e.to_string())?,
        audio.modelled,
        &audio.engines,
    );
    let (track_models, chains) = audio.tones.resolve(&sequence);
    let temp = path.with_extension("wav.part");
    let written = std::fs::File::create(&temp)
        .map_err(|e| format!("cannot write {}: {e}", temp.display()))
        .and_then(|file| {
            let mut writer = std::io::BufWriter::new(file);
            let rate = audio.sample_rate.unwrap_or(gscore_audio::WAV_SAMPLE_RATE);
            gscore_audio::write_wav_with(
                &mut writer,
                &sequence,
                &bank,
                rate,
                &audio.models,
                &track_models,
                &chains,
                audio.bus.as_ref(),
            )
            .and_then(|info| {
                std::io::Write::flush(&mut writer)
                    .map_err(|e| {
                        gscore_play::PlayError::Device(format!("cannot write the WAV file: {e}"))
                    })
                    .map(|()| info)
            })
            .map_err(|e| e.to_string())
        });
    let info = match written {
        Ok(info) => info,
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
    };
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("cannot write {}: {error}", path.display()));
    }
    let name = path.file_name().map_or_else(
        || "export.wav".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let seconds = info.frames / u64::from(info.sample_rate.max(1));
    Ok(format!(
        "Exported {name} ({}:{:02}, {} track{})",
        seconds / 60,
        seconds % 60,
        sequence.tracks.len(),
        if sequence.tracks.len() == 1 { "" } else { "s" }
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
        let message = export_pdf(&song, &[0], &options, &path).unwrap();
        assert!(message.contains("pages"), "{message}");
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));

        let missing = dir.join("no such folder").join("song.pdf");
        assert!(export_pdf(&song, &[0], &options, &missing).is_err());
        assert!(export_pdf(&song, &[], &options, &dir.join("none.pdf")).is_err());
        assert!(
            export_pdf(&song, &[9], &options, &dir.join("other.pdf")).is_err(),
            "no track 10"
        );
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1, "only the exported file, no temporary one");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_midi_export_keeps_muted_and_unsoloed_tracks() {
        use gscore_model::testing::{BASS_4, GUITAR_6, song};
        let dir = std::env::temp_dir().join(format!("gscore-midi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut song = song(&[GUITAR_6, BASS_4, GUITAR_6], 2).unwrap();
        let _ = song
            .set_track_mute(gscore_model::TrackIndex::new(0), true)
            .unwrap();
        let _ = song
            .set_track_solo(gscore_model::TrackIndex::new(1), true)
            .unwrap();
        let path = dir.join("song.mid");
        let message = export_midi(&song, &path).unwrap();
        assert!(message.contains("3 tracks"), "{message}");
        let bytes = std::fs::read(&path).unwrap();
        // The header counts the conductor track and the three tracks.
        assert_eq!(bytes.get(10..12), Some(&[0, 4][..]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wav_export_renders_the_song_or_leaves_no_partial_file() {
        use gscore_model::testing::{TestNote, song_with_notes};
        let dir = std::env::temp_dir().join(format!("gscore-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let soundfont = dir.join("sine.sf2");
        std::fs::write(&soundfont, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
        let song =
            song_with_notes(&[(4, &[TestNote::new(1, 5)]), (4, &[]), (4, &[]), (4, &[])]).unwrap();
        let path = dir.join("song.wav");
        let message = export_wav(
            &song,
            &path,
            &AudioExport {
                soundfont: Some(soundfont.clone()),
                ..AudioExport::default()
            },
        )
        .unwrap();
        assert!(message.contains("song.wav"), "{message}");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.get(0..4), Some(&b"RIFF"[..]));
        assert_eq!(bytes.get(8..12), Some(&b"WAVE"[..]));

        // A SoundFont that is not there fails without leaving a file behind.
        let missing = dir.join("missing.wav");
        assert!(
            export_wav(
                &song,
                &missing,
                &AudioExport {
                    soundfont: Some(dir.join("none.sf2")),
                    ..AudioExport::default()
                },
            )
            .is_err()
        );
        assert!(!missing.exists());
        let temp_files = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|e| e == "part"))
            .count();
        assert_eq!(temp_files, 0, "no temporary file was left behind");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
