//! The plugin host process's side (`guitarscore-plugin-host`, or the app started with
//! `--plugin-host`): reads [`ToHost`] messages from stdin and answers on stdout. One process
//! runs one plugin (or answers one describe or scan request), so a plugin that crashes takes
//! only its own process down.
//!
//! Three threads: a reader for the app's messages; the main thread, which owns the plugin
//! (CLAP's main thread) and keeps its editor alive; and the audio thread, which runs the
//! sound. While the editor is open, the plugin's saved settings are sent back whenever they
//! changed, so the app keeps what was set there.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::wire::{FromHost, ToHost, read_frame, write_frame};
use crate::{Effect, Library, PluginError, Processor};

/// How often the main thread looks after an open editor...
const EDITOR_TICK: Duration = Duration::from_millis(10);
/// ...and after the plugin otherwise.
const IDLE_TICK: Duration = Duration::from_millis(50);
/// How often the settings are compared while the editor is open.
const STATE_CHECK: Duration = Duration::from_secs(1);

/// The answers' way out, shared by the main and the audio thread.
type Output = Arc<Mutex<Box<dyn Write + Send>>>;

fn send(output: &Output, answer: &FromHost) -> bool {
    let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
    write_frame(&mut *output, &answer.encode()).is_ok()
}

/// What the reader hands the main thread: a message, or the end of the input.
enum Inbox {
    Message(ToHost),
    Ended(i32),
}

/// Answers requests from `input` on `output` until `Quit` or the end of `input`. Returns the
/// process's exit code: 0, or 1 when the app's side sent something unreadable.
pub fn serve(input: impl Read + Send + 'static, output: impl Write + Send + 'static) -> i32 {
    let output: Output = Arc::new(Mutex::new(Box::new(output)));
    let (to_main, inbox) = mpsc::channel();
    let reader = std::thread::Builder::new()
        .name("host input".into())
        .spawn(move || read_input(input, &to_main));
    if reader.is_err() {
        return 1;
    }
    let mut host = Host::default();
    let code = loop {
        let wait = if host.editor_open() {
            EDITOR_TICK
        } else {
            IDLE_TICK
        };
        match inbox.recv_timeout(wait) {
            Ok(Inbox::Message(ToHost::Quit)) => break 0,
            Ok(Inbox::Message(message)) => {
                if let Some(answer) = host.handle(message, &output)
                    && !send(&output, &answer)
                {
                    break 0;
                }
            }
            Ok(Inbox::Ended(code)) => break code,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break 0,
        }
        host.look_after(&output);
    };
    host.finish();
    code
}

fn read_input(mut input: impl Read, to_main: &Sender<Inbox>) {
    loop {
        let ended = match read_frame(&mut input) {
            Ok(Some(frame)) => match ToHost::decode(&frame) {
                Ok(message) => {
                    if to_main.send(Inbox::Message(message)).is_err() {
                        return;
                    }
                    continue;
                }
                Err(_) => 1,
            },
            Ok(None) => 0,
            Err(_) => 1,
        };
        let _ = to_main.send(Inbox::Ended(ended));
        return;
    }
}

/// The audio thread's work: one block, or a reset.
enum Work {
    Block {
        params: Vec<(u32, f64)>,
        left: Vec<f32>,
        right: Vec<f32>,
    },
    Reset,
}

/// The plugin running, and its audio thread.
struct Running {
    effect: Effect,
    work: Sender<Work>,
    audio: JoinHandle<Processor>,
    /// The settings last sent to the app.
    sent_state: Option<Vec<u8>>,
    checked: Instant,
}

#[derive(Default)]
struct Host {
    running: Option<Running>,
}

impl Host {
    fn editor_open(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| running.effect.editor_open())
    }

    fn handle(&mut self, message: ToHost, output: &Output) -> Option<FromHost> {
        match message {
            ToHost::Start {
                library,
                id,
                state,
                config,
            } => Some(FromHost::Started(
                self.start(&library, &id, &state, config, output)
                    .map_err(|e| e.to_string()),
            )),
            ToHost::Process {
                params,
                left,
                right,
            } => match &self.running {
                Some(running) => {
                    let _ = running.work.send(Work::Block {
                        params,
                        left,
                        right,
                    });
                    None
                }
                None => Some(FromHost::Processed(Err("no plugin is running".into()))),
            },
            ToHost::Reset => {
                if let Some(running) = &self.running {
                    let _ = running.work.send(Work::Reset);
                }
                None
            }
            ToHost::OpenEditor { title, scale } => {
                Some(FromHost::EditorOpened(match &mut self.running {
                    Some(running) => {
                        let opened = running.effect.open_editor(&title, scale);
                        running.checked = Instant::now();
                        opened.map_err(|e| e.to_string())
                    }
                    None => Err("no plugin is running".into()),
                }))
            }
            ToHost::CloseEditor => {
                let running = self.running.as_mut()?;
                if !running.effect.editor_open() {
                    return None;
                }
                running.effect.close_editor();
                send_state(running, output);
                Some(FromHost::EditorClosed)
            }
            ToHost::Describe { library, id, state } => {
                Some(FromHost::Described(describe(&library, &id, &state)))
            }
            ToHost::Scan { library } => Some(FromHost::Scanned(
                Library::open(&library)
                    .map(|library| library.plugins())
                    .map_err(|e| e.to_string()),
            )),
            ToHost::Quit => None,
        }
    }

    fn start(
        &mut self,
        library: &std::path::Path,
        id: &str,
        state: &[u8],
        config: crate::AudioConfig,
        output: &Output,
    ) -> Result<u32, PluginError> {
        let library = Library::open(library)?;
        let mut effect = Effect::new(&library, id)?;
        if !state.is_empty() {
            effect.load_state(state)?;
        }
        let latency = effect.latency();
        let processor = effect.activate(config)?;
        let (work, jobs) = mpsc::channel();
        let output = Arc::clone(output);
        let audio = std::thread::Builder::new()
            .name("host audio".into())
            .spawn(move || run_audio(processor, &jobs, &output))
            .map_err(|e| PluginError::Process(e.to_string()))?;
        self.running = Some(Running {
            effect,
            work,
            audio,
            sent_state: Some(state.to_vec()),
            checked: Instant::now(),
        });
        Ok(latency)
    }

    /// The plugin's main-thread requests, its editor's timers and window events, and its
    /// settings sent back when the editor changed them.
    fn look_after(&mut self, output: &Output) {
        let Some(running) = &mut self.running else {
            return;
        };
        let _ = running.effect.idle();
        if !running.effect.editor_open() {
            return;
        }
        if running.effect.tick() {
            // The user closed the window.
            send_state(running, output);
            let _ = send(output, &FromHost::EditorClosed);
            return;
        }
        if running.checked.elapsed() >= STATE_CHECK {
            running.checked = Instant::now();
            send_state(running, output);
        }
    }

    fn finish(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.effect.close_editor();
            drop(running.work);
            if let Ok(processor) = running.audio.join() {
                running.effect.deactivate(processor);
            }
        }
    }
}

/// Sends the plugin's settings if they differ from those last sent.
fn send_state(running: &mut Running, output: &Output) {
    if let Ok(state) = running.effect.save_state()
        && running.sent_state.as_ref() != Some(&state)
    {
        let _ = send(output, &FromHost::State(state.clone()));
        running.sent_state = Some(state);
    }
}

fn run_audio(mut processor: Processor, jobs: &Receiver<Work>, output: &Output) -> Processor {
    while let Ok(job) = jobs.recv() {
        match job {
            Work::Reset => processor.reset(),
            Work::Block {
                params,
                mut left,
                mut right,
            } => {
                for (id, value) in params {
                    processor.set_param(id, value);
                }
                let result = processor
                    .process(&mut left, &mut right)
                    .map(|()| (left, right))
                    .map_err(|e| e.to_string());
                if !send(output, &FromHost::Processed(result)) {
                    break;
                }
            }
        }
    }
    processor
}

type Described = Result<(crate::PluginInfo, Vec<crate::Param>, Vec<f64>, u32), String>;

fn describe(library: &std::path::Path, id: &str, state: &[u8]) -> Described {
    let described = (|| {
        let library = Library::open(library)?;
        let info = library
            .plugins()
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_owned()))?;
        let mut effect = Effect::new(&library, id)?;
        if !state.is_empty() {
            effect.load_state(state)?;
        }
        let params = effect.params();
        let values = params
            .iter()
            .map(|param| effect.value(param.id).unwrap_or(param.default))
            .collect();
        Ok::<_, PluginError>((info, params, values, effect.latency()))
    })();
    described.map_err(|e| e.to_string())
}
