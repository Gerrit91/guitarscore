//! Playback to a MIDI output port (an external or software synthesizer such as FluidSynth)
//! through `midir`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gscore_play::{MidiEvent, PlayError, PlayState, Player, Sequence};
use midir::{MidiOutput, MidiOutputConnection};

use crate::transport::{Command, Sink, Transport};

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
    /// The drum channels of the sequence last loaded (live changes to them go to 10).
    drums: [bool; 16],
    /// How long events are held back (microseconds), so a hardware synthesizer lines up
    /// with the built-in one's audio output.
    delay: Arc<AtomicU64>,
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
        let delay = Arc::new(AtomicU64::new(0));
        let thread_delay = Arc::clone(&delay);
        let thread = std::thread::Builder::new()
            .name("gscore-midi-out".to_owned())
            .spawn(move || run(connection, &receiver, &thread_status, &thread_delay))
            .map_err(|e| PlayError::Device(format!("cannot start the MIDI thread: {e}")))?;
        Ok(MidiOutPlayer {
            port: name,
            drums: [false; 16],
            delay,
            commands,
            status,
            thread: Some(thread),
        })
    }

    /// Holds the events back by `delay`: the built-in synthesizer's audio reaches the ears
    /// that much later than a hardware synthesizer hears its MIDI.
    pub fn set_delay(&self, delay: Duration) {
        /// More than this is a broken device timestamp, not output latency.
        const MAX_DELAY: u64 = 250_000;
        let micros = u64::try_from(delay.as_micros()).unwrap_or(MAX_DELAY);
        self.delay.store(micros.min(MAX_DELAY), Ordering::Release);
    }

    /// The port this player sends to.
    pub fn port(&self) -> &str {
        &self.port
    }

    /// The last output error, if playback stopped because of one.
    pub fn error(&self) -> Option<String> {
        self.status().error
    }

    /// `sequence` as a General MIDI device plays it: every drum track on channel 10 (the
    /// built-in synthesizer gives each its own channel). Remembers its drum channels, so
    /// live changes to them go to 10 as well.
    fn general_midi(&mut self, sequence: Arc<Sequence>) -> Arc<Sequence> {
        self.drums = sequence.drum_channels();
        match sequence.general_midi() {
            Some(general) => Arc::new(general),
            None => sequence,
        }
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
        let sequence = self.general_midi(sequence);
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

    fn send_event(&mut self, event: MidiEvent) -> Result<(), PlayError> {
        let event = Sequence::general_midi_event(&self.drums, event);
        self.send(Command::Send(event))
    }

    fn replace(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        let sequence = self.general_midi(sequence);
        self.send(Command::Replace(sequence))
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

impl Sink for Output {
    fn send(&mut self, event: MidiEvent) {
        if let Err(e) = self.connection.send(&event.bytes()) {
            self.error = Some(format!("MIDI output failed: {e}"));
        }
    }

    fn silence(&mut self, sequence: &Sequence) {
        for event in MidiEvent::silence(sequence.channels()) {
            self.send(event);
        }
    }
}

/// The playing thread: handles commands and sends due events.
fn run(
    connection: MidiOutputConnection,
    commands: &Receiver<Command>,
    status: &Mutex<Status>,
    delay: &AtomicU64,
) {
    let clock = Instant::now();
    let now = || u64::try_from(clock.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut out = Output {
        connection,
        error: None,
    };
    let mut transport = Transport::default();
    // A play held back until the built-in synthesizer's audio output has caught up.
    let mut pending_play: Option<u64> = None;

    loop {
        let at = now();
        if let Some(due) = pending_play
            && at >= due
        {
            pending_play = None;
            let _old = transport.handle(Command::Play, at, &mut out);
        }
        let wait = if pending_play.is_some() || transport.state() == PlayState::Playing {
            TICK
        } else {
            IDLE
        };
        match commands.recv_timeout(wait) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => {
                transport.fail(&mut out);
                break;
            }
            Ok(Command::Play) if transport.state() != PlayState::Playing => {
                out.error = None;
                let delay = delay.load(Ordering::Acquire);
                if delay > 0 {
                    pending_play = Some(at.saturating_add(delay));
                } else {
                    // The old sequence is freed here; this thread may allocate.
                    let _old = transport.handle(Command::Play, at, &mut out);
                }
            }
            Ok(command) => {
                // A pause or a new sequence must not leave this output sounding after the
                // rest has stopped (or playing the old one). A tempo change or a live
                // mixer event leaves a play that is still being held back alone.
                if matches!(
                    command,
                    Command::Pause | Command::Stop | Command::Load(_) | Command::Seek(_)
                ) {
                    pending_play = None;
                }
                // The old sequence is freed here; this thread may allocate.
                let _old = transport.handle(command, at, &mut out);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        transport.advance(now(), &mut out);
        if out.error.is_some() && transport.state() == PlayState::Playing {
            // A failing device stops playback; the error is reported, editing goes on.
            transport.fail(&mut out);
        }
        let mut shared = lock(status);
        shared.position = transport.position();
        shared.state = transport.state();
        shared.error.clone_from(&out.error);
    }
    lock(status).state = PlayState::Stopped;
}
