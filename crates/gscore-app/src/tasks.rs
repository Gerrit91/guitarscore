//! Work that must not block the UI thread (AGENTS.md rule 9): showing the file dialog and
//! reading and importing a file. Each job runs on its own thread and reports back through a
//! channel that the UI polls once per frame.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use gscore_model::Song;

/// A finished background job.
#[derive(Debug)]
pub(crate) enum Event {
    /// The file dialog closed; `None` if it was cancelled.
    Picked(Option<PathBuf>),
    /// A file was read and imported, or failed to.
    Opened {
        /// The file.
        path: PathBuf,
        /// The song (boxed: it is large and events are moved around), or why it could
        /// not be opened.
        result: Result<Box<Song>, String>,
    },
    /// A job panicked (the panic hook wrote a crash report); the message says which.
    Failed(String),
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
                .map(|imported| Box::new(imported.song))
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
