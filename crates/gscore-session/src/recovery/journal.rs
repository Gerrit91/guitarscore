//! [`Journal`]: the recovery folder of one open document, written on a thread of its own.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gscore_edit::Recorded;
use gscore_io::{FileFormat, export_to_bytes};
use gscore_model::Song;

use super::{
    BASE, JOURNAL, LOCK, META, SAVED, SNAPSHOT_EVERY, SNAPSHOT_INFO, SNAPSHOT_INTERVAL,
    SNAPSHOT_SONG, checksum, journal_line,
};

/// Jobs queued before entries wait in the journal's own buffer.
const QUEUE: usize = 256;

/// Distinguishes folders created in the same second by the same process.
static NEXT_FOLDER: AtomicU64 = AtomicU64::new(0);

/// How often the journal writes a full snapshot next to the command lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryOptions {
    /// After this many entries.
    pub snapshot_every: usize,
    /// Or after this much time; `Duration::ZERO` disables the time check.
    pub snapshot_interval: Duration,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        RecoveryOptions {
            snapshot_every: SNAPSHOT_EVERY,
            snapshot_interval: SNAPSHOT_INTERVAL,
        }
    }
}

/// Work for the writer thread.
enum Job {
    /// Journal lines to append.
    Append(String),
    /// A snapshot of the song after this many journal entries.
    Snapshot { song: Box<Song>, entries: usize },
    /// The document now belongs to this file (after Save As).
    Source(PathBuf),
    /// The document was closed normally: remove the folder.
    Close,
}

/// The recovery journal of one open document. Recording never waits: entries go to a
/// background thread, and if its queue is full they wait in a buffer for the next call.
/// Dropping the journal without [`Journal::close`] keeps the folder, so the document can
/// be recovered (that is what a crash does).
#[derive(Debug)]
pub struct Journal {
    dir: PathBuf,
    /// `None` once closed or dropped.
    jobs: Option<SyncSender<JobBox>>,
    thread: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
    /// Lines not yet handed to the thread (its queue was full).
    pending: String,
    /// Entries the thread has written and synced to disk.
    written: Arc<AtomicUsize>,
    entries: usize,
    snapshot_entries: usize,
    last_snapshot: Instant,
    options: RecoveryOptions,
}

/// `Job` is not `Debug` (it holds a song); this wrapper keeps the channel type printable.
struct JobBox(Job);

impl std::fmt::Debug for JobBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Job")
    }
}

impl Journal {
    /// Creates a recovery folder in `root` for a document read from `source` (`None` for a
    /// new one) whose bytes as opened are `base`, and locks it. This writes files: call it
    /// off the UI thread (for example where the file is opened).
    pub fn start(root: &Path, source: Option<&Path>, base: &[u8]) -> Result<Self, String> {
        Journal::start_with_options(root, source, base, RecoveryOptions::default())
    }

    /// [`Journal::start`] with the interval at which full snapshots are written (the
    /// settings window's autosave interval).
    pub fn start_with_options(
        root: &Path,
        source: Option<&Path>,
        base: &[u8],
        options: RecoveryOptions,
    ) -> Result<Self, String> {
        let failed = |what: &str, e: std::io::Error| format!("recovery journal: {what}: {e}");
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let folder = format!(
            "{seconds}-{}-{}",
            std::process::id(),
            NEXT_FOLDER.fetch_add(1, Ordering::Relaxed)
        );
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).map_err(|e| failed("cannot create the folder", e))?;
        let lock = File::create(dir.join(LOCK)).map_err(|e| failed("cannot create the lock", e))?;
        lock.try_lock()
            .map_err(|e| format!("recovery journal: cannot lock {}: {e}", dir.display()))?;
        write_atomic(&dir, META, meta_text(source).as_bytes())
            .map_err(|e| failed("cannot write the metadata", e))?;
        write_atomic(&dir, BASE, base).map_err(|e| failed("cannot write the base", e))?;
        let journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(JOURNAL))
            .map_err(|e| failed("cannot create the journal", e))?;

        let (jobs, receiver) = mpsc::sync_channel(QUEUE);
        let error = Arc::new(Mutex::new(None));
        let thread_error = Arc::clone(&error);
        let thread_dir = dir.clone();
        let written = Arc::new(AtomicUsize::new(0));
        let thread_written = Arc::clone(&written);
        let thread = std::thread::Builder::new()
            .name("gscore-journal".to_owned())
            .spawn(move || {
                let writer = Writer {
                    dir: &thread_dir,
                    error: &thread_error,
                    written: &thread_written,
                };
                writer.run(lock, journal, &receiver);
            })
            .map_err(|e| failed("cannot start the writer", e))?;
        Ok(Journal {
            dir,
            jobs: Some(jobs),
            thread: Some(thread),
            error,
            pending: String::new(),
            written,
            entries: 0,
            snapshot_entries: 0,
            last_snapshot: Instant::now(),
            options,
        })
    }

    /// The recovery folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many entries are on disk (written and synced): a crash now loses none of them.
    /// Recording returns before this; the writer catches up within milliseconds.
    pub fn written(&self) -> usize {
        self.written.load(Ordering::Acquire)
    }

    /// Why the journal could not be written, if it could not (the document is then not
    /// protected; the app should say so).
    pub fn error(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Records `commands` (as [`gscore_edit::TabEditor::take_applied`] returns them); `song`
    /// is the song after them, for the periodic snapshot.
    pub fn record(&mut self, commands: &[Recorded], song: &Song) {
        if commands.is_empty() {
            return;
        }
        for entry in commands {
            self.pending.push_str(&journal_line(&entry.to_line()));
        }
        self.entries = self.entries.saturating_add(commands.len());
        self.flush();
        let since = self.entries.saturating_sub(self.snapshot_entries);
        let every = self.options.snapshot_every.max(1);
        let time_due = !self.options.snapshot_interval.is_zero()
            && self.last_snapshot.elapsed() >= self.options.snapshot_interval;
        if since >= every || time_due {
            self.snapshot(song);
        }
    }

    /// Records that the document was saved to `path`: there is nothing to recover until
    /// the next edit.
    pub fn saved(&mut self, path: &Path) {
        self.pending.push_str(&journal_line(SAVED));
        self.entries = self.entries.saturating_add(1);
        self.flush();
        self.send(Job::Source(path.to_path_buf()));
    }

    /// Writes a snapshot of `song` now (for example after a contained panic).
    pub fn snapshot(&mut self, song: &Song) {
        self.flush();
        self.snapshot_entries = self.entries;
        self.last_snapshot = Instant::now();
        self.send(Job::Snapshot {
            song: Box::new(song.clone()),
            entries: self.entries,
        });
    }

    /// The document was closed normally (saved, or its changes discarded): the folder is
    /// removed. Waits for the writer to finish.
    pub fn close(mut self) {
        self.flush();
        // Blocking is fine here: closing a document is not a frame of editing.
        if let Some(jobs) = self.jobs.take() {
            if !self.pending.is_empty() {
                let lines = std::mem::take(&mut self.pending);
                let _ = jobs.send(JobBox(Job::Append(lines)));
            }
            let _ = jobs.send(JobBox(Job::Close));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Hands buffered lines to the thread, unless its queue is full.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let Some(jobs) = &self.jobs else {
            return;
        };
        let lines = std::mem::take(&mut self.pending);
        match jobs.try_send(JobBox(Job::Append(lines))) {
            Ok(()) => {}
            Err(TrySendError::Full(JobBox(Job::Append(lines)))) => self.pending = lines,
            Err(_) => self.report("the writer stopped".to_owned()),
        }
    }

    /// Queues `job` (a snapshot or metadata; skipped while the queue is full).
    fn send(&mut self, job: Job) {
        let disconnected = self.jobs.as_ref().is_none_or(|jobs| {
            matches!(
                jobs.try_send(JobBox(job)),
                Err(TrySendError::Disconnected(_))
            )
        });
        if disconnected {
            self.report("the writer stopped".to_owned());
        }
    }

    fn report(&self, error: String) {
        *self.error.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        // Not closed: keep the folder for recovery, but write what is buffered first and
        // let the writer finish (a crash does not get here; what was written stays).
        if let Some(jobs) = self.jobs.take() {
            if !self.pending.is_empty() {
                let lines = std::mem::take(&mut self.pending);
                let _ = jobs.send(JobBox(Job::Append(lines)));
            }
            drop(jobs);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The metadata file's text.
fn meta_text(source: Option<&Path>) -> String {
    // A path with a line break cannot be stored on one line; such a document recovers
    // without its file name.
    source
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.contains('\n'))
        .map_or_else(String::new, |p| format!("source={p}\n"))
}

/// Writes `bytes` to `dir/name` through a temporary file and a rename, so the file is
/// either the old or the new one, never half written.
fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = dir.join(format!("{name}.tmp"));
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, dir.join(name))
}

/// What the writer thread reports to.
struct Writer<'a> {
    dir: &'a Path,
    error: &'a Mutex<Option<String>>,
    written: &'a AtomicUsize,
}

impl Writer<'_> {
    fn report(&self, message: String) {
        *self.error.lock().unwrap_or_else(PoisonError::into_inner) = Some(message);
    }

    /// The writer thread. It holds the folder's lock until it ends.
    fn run(&self, lock: File, mut journal: File, jobs: &Receiver<JobBox>) {
        while let Ok(JobBox(job)) = jobs.recv() {
            match job {
                Job::Append(lines) => {
                    let written = journal
                        .write_all(lines.as_bytes())
                        .and_then(|()| journal.sync_data());
                    match written {
                        Ok(()) => {
                            let count = lines.matches('\n').count();
                            self.written.fetch_add(count, Ordering::Release);
                        }
                        Err(e) => self.report(format!("recovery journal: cannot write: {e}")),
                    }
                }
                Job::Snapshot { song, entries } => {
                    if let Err(e) = write_snapshot(self.dir, &song, entries) {
                        self.report(format!("recovery snapshot: {e}"));
                    }
                }
                Job::Source(path) => {
                    let meta = meta_text(Some(&path));
                    if let Err(e) = write_atomic(self.dir, META, meta.as_bytes()) {
                        self.report(format!("recovery journal: cannot write the metadata: {e}"));
                    }
                }
                Job::Close => {
                    drop(journal);
                    let _ = lock.unlock();
                    drop(lock);
                    if let Err(e) = std::fs::remove_dir_all(self.dir) {
                        self.report(format!("cannot remove {}: {e}", self.dir.display()));
                    }
                    return;
                }
            }
        }
    }
}

/// Writes the snapshot song, then the file saying how many entries it includes and its
/// checksum (so a snapshot from before a crash between the two renames is detected).
fn write_snapshot(dir: &Path, song: &Song, entries: usize) -> Result<(), String> {
    let bytes = export_to_bytes(song, FileFormat::Gp).map_err(|e| e.to_string())?;
    write_atomic(dir, SNAPSHOT_SONG, &bytes).map_err(|e| e.to_string())?;
    let info = format!("entries={entries}\nchecksum={}\n", checksum(&bytes));
    write_atomic(dir, SNAPSHOT_INFO, info.as_bytes()).map_err(|e| e.to_string())
}
