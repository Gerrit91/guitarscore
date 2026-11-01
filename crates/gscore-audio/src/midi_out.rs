//! Playback to a MIDI output port (an external or software synthesizer such as FluidSynth)
//! through `midir`.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gscore_play::{MidiEvent, PlayError, PlayState, Player, Scheduler, Sequence};
use midir::{MidiOutput, MidiOutputConnection};

/// Client name shown to other MIDI programs.
const CLIENT: &str = "GuitarScore";
/// How often the playing thread wakes up to send due events.
const TICK: Duration = Duration::from_millis(1);
/// How long the idle thread waits for a command.
const IDLE: Duration = Duration::from_millis(50);
/// Commands queued before senders wait (they never wait long: the thread drains quickly).
const QUEUE: usize = 64;
/// Ports whose name contains this are skipped when choosing a default port: ALSA's
/// "Midi Through" makes no sound.
const THROUGH: &str = "Midi Through";

/// The names of the MIDI output ports, in system order.
pub fn midi_output_ports() -> Result<Vec<String>, PlayError> {
    let output = MidiOutput::new(CLIENT).map_err(|e| PlayError::Device(e.to_string()))?;
    Ok(output
        .ports()
        .iter()
        .filter_map(|port| output.port_name(port).ok())
        .collect())
}

/// A command for the playing thread.
#[derive(Debug)]
enum Command {
    Load(Arc<Sequence>),
    Play,
    Pause,
    Stop,
    Seek(u64),
    TempoScale(f32),
    Quit,
}

/// What the playing thread reports back.
#[derive(Debug, Clone)]
struct Status {
    position: u64,
    state: PlayState,
    error: Option<String>,
}

/// Plays sequences to one MIDI output port, on a thread of its own.
#[derive(Debug)]
pub struct MidiOutPlayer {
    port: String,
    commands: SyncSender<Command>,
    status: Arc<Mutex<Status>>,
    thread: Option<JoinHandle<()>>,
}

impl MidiOutPlayer {
    /// Connects to the first output port whose name contains `port`, or (for `None`) the
    /// first port that makes sound (not "Midi Through").
    pub fn connect(port: Option<&str>) -> Result<Self, PlayError> {
        let output = MidiOutput::new(CLIENT).map_err(|e| PlayError::Device(e.to_string()))?;
        let ports = output.ports();
        let named: Vec<(String, &midir::MidiOutputPort)> = ports
            .iter()
            .filter_map(|p| output.port_name(p).ok().map(|name| (name, p)))
            .collect();
        let chosen = match port {
            Some(wanted) => named.iter().find(|(name, _)| name.contains(wanted)),
            None => named
                .iter()
                .find(|(name, _)| !name.contains(THROUGH))
                .or_else(|| named.first()),
        };
        let Some((name, port)) = chosen else {
            return Err(PlayError::Device(match port {
                Some(wanted) => format!("no MIDI output port matches \"{wanted}\""),
                None => {
                    "no MIDI output port found; start a synthesizer such as FluidSynth".to_owned()
                }
            }));
        };
        let name = name.clone();
        let connection = output
            .connect(port, "GuitarScore out")
            .map_err(|e| PlayError::Device(format!("cannot connect to {name}: {e}")))?;

        let (commands, receiver) = mpsc::sync_channel(QUEUE);
        let status = Arc::new(Mutex::new(Status {
            position: 0,
            state: PlayState::Stopped,
            error: None,
        }));
        let thread_status = Arc::clone(&status);
        let thread = std::thread::Builder::new()
            .name("gscore-midi-out".to_owned())
            .spawn(move || run(connection, &receiver, &thread_status))
            .map_err(|e| PlayError::Device(format!("cannot start the MIDI thread: {e}")))?;
        Ok(MidiOutPlayer {
            port: name,
            commands,
            status,
            thread: Some(thread),
        })
    }

    /// The port this player sends to.
    pub fn port(&self) -> &str {
        &self.port
    }

    /// The last output error, if playback stopped because of one.
    pub fn error(&self) -> Option<String> {
        self.status().error
    }

    fn send(&self, command: Command) -> Result<(), PlayError> {
        self.commands
            .send(command)
            .map_err(|_| PlayError::Disconnected)
    }

    fn status(&self) -> Status {
        lock(&self.status).clone()
    }
}

impl Player for MidiOutPlayer {
    fn load(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        self.send(Command::Load(sequence))
    }

    fn play(&mut self) -> Result<(), PlayError> {
        self.send(Command::Play)
    }

    fn pause(&mut self) -> Result<(), PlayError> {
        self.send(Command::Pause)
    }

    fn stop(&mut self) -> Result<(), PlayError> {
        self.send(Command::Stop)
    }

    fn seek(&mut self, tick: u64) -> Result<(), PlayError> {
        self.send(Command::Seek(tick))
    }

    fn set_tempo_scale(&mut self, scale: f32) -> Result<(), PlayError> {
        self.send(Command::TempoScale(scale))
    }

    fn position(&self) -> u64 {
        self.status().position
    }

    fn state(&self) -> PlayState {
        self.status().state
    }
}

impl Drop for MidiOutPlayer {
    fn drop(&mut self) {
        // Silence the synth and end the thread; a thread that already ended (or panicked;
        // the panic hook wrote a report) is fine here.
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A poisoned lock only means a panic elsewhere; the status is still usable.
fn lock(status: &Mutex<Status>) -> std::sync::MutexGuard<'_, Status> {
    status.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The connection and the last error sending to it.
struct Output {
    connection: MidiOutputConnection,
    error: Option<String>,
}

impl Output {
    fn send(&mut self, events: impl IntoIterator<Item = MidiEvent>) {
        for event in events {
            if let Err(e) = self.connection.send(&event.bytes()) {
                self.error = Some(format!("MIDI output failed: {e}"));
            }
        }
    }

    /// Silences the channels `scheduler`'s sequence uses.
    fn silence(&mut self, scheduler: &Scheduler) {
        self.send(MidiEvent::silence(scheduler.sequence().channels()));
    }
}

/// The playing thread: handles commands and sends due events.
fn run(connection: MidiOutputConnection, commands: &Receiver<Command>, status: &Mutex<Status>) {
    let clock = Instant::now();
    let now = || u64::try_from(clock.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut out = Output {
        connection,
        error: None,
    };
    let mut scheduler: Option<Scheduler> = None;
    let mut state = PlayState::Stopped;

    loop {
        let wait = if state == PlayState::Playing {
            TICK
        } else {
            IDLE
        };
        match commands.recv_timeout(wait) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(s) = &scheduler {
                    out.silence(s);
                }
                break;
            }
            Ok(Command::Load(sequence)) => {
                if let Some(s) = &scheduler {
                    out.silence(s);
                }
                scheduler = Some(Scheduler::new(sequence));
                state = PlayState::Stopped;
            }
            Ok(Command::Play) => {
                if let Some(s) = &mut scheduler
                    && state != PlayState::Playing
                {
                    out.error = None;
                    if state == PlayState::Stopped || s.finished() {
                        // From the start: send the setup (programs, volume, pan) first.
                        let setup = s.seek(0, now());
                        out.send(setup.iter().map(|e| e.event));
                    } else {
                        s.resume(now());
                    }
                    state = PlayState::Playing;
                }
            }
            Ok(Command::Pause) => {
                if let Some(s) = &scheduler
                    && state == PlayState::Playing
                {
                    out.silence(s);
                    state = PlayState::Paused;
                }
            }
            Ok(Command::Stop) => {
                if let Some(s) = &mut scheduler {
                    out.silence(s);
                    let _ = s.seek(0, now());
                }
                state = PlayState::Stopped;
            }
            Ok(Command::Seek(tick)) => {
                if let Some(s) = &mut scheduler {
                    out.silence(s);
                    let setup = s.seek(tick, now());
                    out.send(setup.iter().map(|e| e.event));
                    if state == PlayState::Stopped {
                        state = PlayState::Paused;
                    }
                }
            }
            Ok(Command::TempoScale(scale)) => {
                if let Some(s) = &mut scheduler {
                    s.set_tempo_scale(scale, now());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        if state == PlayState::Playing
            && let Some(s) = &mut scheduler
        {
            let due: Vec<MidiEvent> = s.advance(now()).iter().map(|e| e.event).collect();
            out.send(due);
            if s.finished() {
                out.silence(s);
                state = PlayState::Stopped;
            }
            if out.error.is_some() {
                // A failing device stops playback; the error is reported, editing goes on.
                out.silence(s);
                state = PlayState::Stopped;
            }
        }
        let mut shared = lock(status);
        shared.position = scheduler.as_ref().map_or(0, Scheduler::position);
        shared.state = state;
        shared.error.clone_from(&out.error);
    }
    lock(status).state = PlayState::Stopped;
}
