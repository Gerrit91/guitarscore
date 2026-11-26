//! The plugin thread: the one thread that makes, owns and drops every [`Effect`] (CLAP's
//! "main thread" for them), and handles their requests. Others talk to it through a
//! [`PluginHandle`]: each request comes back on a channel of its own, so the caller decides
//! whether to wait (off the UI thread) or to look later (the UI).
//!
//! A panic in a request is caught and answered as an error; the thread goes on. A plugin
//! that crashes the process is the separate host process's job (step 3 of M24).

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::{AudioConfig, Effect, Library, Param, PluginError, PluginInfo, Processor};

/// How often the thread looks after its plugins when no request comes.
const IDLE: Duration = Duration::from_millis(15);

/// An effect started on the plugin thread: its processor for the audio thread, and what
/// identifies it there.
#[derive(Debug)]
pub struct Started {
    pub instance: u64,
    pub processor: Processor,
    /// How many frames late its sound comes out.
    pub latency: u32,
}

/// What a plugin offers, for its parameter view.
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    pub info: PluginInfo,
    pub params: Vec<Param>,
    /// Each parameter's value with the settings loaded, in the order of `params`.
    pub values: Vec<f64>,
    pub latency: u32,
}

/// One library's plugins, or why it could not be opened.
pub type Scanned = (PathBuf, Result<Vec<PluginInfo>, PluginError>);

type Reply<T> = Sender<Result<T, PluginError>>;

enum Request {
    Start {
        library: PathBuf,
        id: String,
        state: Vec<u8>,
        config: AudioConfig,
        reply: Reply<Started>,
    },
    Stop {
        instance: u64,
        /// Boxed: a processor is large, the other requests are not.
        processor: Box<Processor>,
    },
    Describe {
        library: PathBuf,
        id: String,
        state: Vec<u8>,
        reply: Reply<Description>,
    },
    Scan {
        dirs: Vec<PathBuf>,
        reply: Sender<Vec<Scanned>>,
    },
    Quit,
}

/// Talks to the plugin thread. Cheap to clone; usable from any thread.
#[derive(Debug, Clone)]
pub struct PluginHandle {
    requests: Sender<Request>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Request::Start { .. } => "Start",
            Request::Stop { .. } => "Stop",
            Request::Describe { .. } => "Describe",
            Request::Scan { .. } => "Scan",
            Request::Quit => "Quit",
        })
    }
}

impl PluginHandle {
    /// Starts plugin `id` of `library` as an effect with the settings `state` (empty: the
    /// plugin's own defaults).
    pub fn start(
        &self,
        library: &Path,
        id: &str,
        state: &[u8],
        config: AudioConfig,
    ) -> Receiver<Result<Started, PluginError>> {
        let (reply, answer) = mpsc::channel();
        self.send(
            Request::Start {
                library: library.to_owned(),
                id: id.to_owned(),
                state: state.to_owned(),
                config,
                reply: reply.clone(),
            },
            &reply,
        );
        answer
    }

    /// Stops the effect started as `instance`; `processor` is the one it came with.
    pub fn stop(&self, instance: u64, processor: Processor) {
        // A thread that is gone has dropped its effects already.
        let _ = self.requests.send(Request::Stop {
            instance,
            processor: Box::new(processor),
        });
    }

    /// The plugin's parameters and latency, with the settings `state` loaded.
    pub fn describe(
        &self,
        library: &Path,
        id: &str,
        state: &[u8],
    ) -> Receiver<Result<Description, PluginError>> {
        let (reply, answer) = mpsc::channel();
        self.send(
            Request::Describe {
                library: library.to_owned(),
                id: id.to_owned(),
                state: state.to_owned(),
                reply: reply.clone(),
            },
            &reply,
        );
        answer
    }

    /// The libraries in `dirs` and their plugins. Opening a library runs its code.
    pub fn scan(&self, dirs: Vec<PathBuf>) -> Receiver<Vec<Scanned>> {
        let (reply, answer) = mpsc::channel();
        if self
            .requests
            .send(Request::Scan {
                dirs,
                reply: reply.clone(),
            })
            .is_err()
        {
            let _ = reply.send(Vec::new());
        }
        answer
    }

    fn send<T>(&self, request: Request, reply: &Reply<T>) {
        if self.requests.send(request).is_err() {
            let _ = reply.send(Err(PluginError::Process(
                "the plugin thread has stopped".into(),
            )));
        }
    }
}

/// The plugin thread. Dropping it stops the thread after its plugins are dropped.
#[derive(Debug)]
pub struct PluginThread {
    handle: PluginHandle,
    thread: Option<JoinHandle<()>>,
}

impl PluginThread {
    /// Starts the thread.
    pub fn spawn() -> Result<Self, PluginError> {
        let (requests, inbox) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("plugins".into())
            .spawn(move || Owner::default().run(&inbox))
            .map_err(|e| PluginError::Process(format!("cannot start the plugin thread: {e}")))?;
        Ok(Self {
            handle: PluginHandle { requests },
            thread: Some(thread),
        })
    }

    /// A handle for sending requests.
    pub fn handle(&self) -> PluginHandle {
        self.handle.clone()
    }
}

impl Drop for PluginThread {
    fn drop(&mut self) {
        let _ = self.handle.requests.send(Request::Quit);
        if let Some(thread) = self.thread.take() {
            // A thread that panicked has nothing left to drop.
            let _ = thread.join();
        }
    }
}

/// What the plugin thread owns.
#[derive(Default)]
struct Owner {
    libraries: HashMap<PathBuf, Library>,
    effects: HashMap<u64, Effect>,
    next: u64,
}

impl Owner {
    fn run(mut self, inbox: &Receiver<Request>) {
        loop {
            match inbox.recv_timeout(IDLE) {
                Ok(Request::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(request) => {
                    // A panic in one request must not end the thread (its reply channel
                    // is dropped, which the caller sees as an error).
                    let _ = catch_unwind(AssertUnwindSafe(|| self.handle(request)));
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let _ = catch_unwind(AssertUnwindSafe(|| self.idle()));
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Start {
                library,
                id,
                state,
                config,
                reply,
            } => {
                let _ = reply.send(self.start(&library, &id, &state, config));
            }
            Request::Stop {
                instance,
                processor,
            } => {
                if let Some(mut effect) = self.effects.remove(&instance) {
                    effect.deactivate(*processor);
                }
            }
            Request::Describe {
                library,
                id,
                state,
                reply,
            } => {
                let _ = reply.send(self.describe(&library, &id, &state));
            }
            Request::Scan { dirs, reply } => {
                let found = crate::bundles_in(&dirs)
                    .into_iter()
                    .map(|path| {
                        let plugins = self.library(&path).map(|library| library.plugins());
                        (path, plugins)
                    })
                    .collect();
                let _ = reply.send(found);
            }
            Request::Quit => {}
        }
    }

    fn library(&mut self, path: &Path) -> Result<Library, PluginError> {
        if let Some(library) = self.libraries.get(path) {
            return Ok(library.clone());
        }
        let library = Library::open(path)?;
        self.libraries.insert(path.to_owned(), library.clone());
        Ok(library)
    }

    fn effect(&mut self, path: &Path, id: &str, state: &[u8]) -> Result<Effect, PluginError> {
        let library = self.library(path)?;
        let mut effect = Effect::new(&library, id)?;
        if !state.is_empty() {
            effect.load_state(state)?;
        }
        Ok(effect)
    }

    fn start(
        &mut self,
        path: &Path,
        id: &str,
        state: &[u8],
        config: AudioConfig,
    ) -> Result<Started, PluginError> {
        let mut effect = self.effect(path, id, state)?;
        let latency = effect.latency();
        let processor = effect.activate(config)?;
        let instance = self.next;
        self.next = self.next.wrapping_add(1);
        self.effects.insert(instance, effect);
        Ok(Started {
            instance,
            processor,
            latency,
        })
    }

    fn describe(
        &mut self,
        path: &Path,
        id: &str,
        state: &[u8],
    ) -> Result<Description, PluginError> {
        let library = self.library(path)?;
        let info = library
            .plugins()
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_owned()))?;
        let mut effect = self.effect(path, id, state)?;
        let params = effect.params();
        let values = params
            .iter()
            .map(|param| effect.value(param.id).unwrap_or(param.default))
            .collect();
        Ok(Description {
            info,
            params,
            values,
            latency: effect.latency(),
        })
    }

    fn idle(&mut self) {
        for effect in self.effects.values_mut() {
            // Requests that need the owner (a restart) come with step 3; the log is
            // dropped for now.
            let _ = effect.idle();
        }
    }
}
