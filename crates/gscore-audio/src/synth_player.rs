//! Playback through the internal synthesizer to the default audio device (`cpal`).
//!
//! A thread of its own loads the SoundFont and opens the device, so the caller never waits
//! for either. The audio callback owns the [`SynthEngine`]: it takes commands from a bounded
//! channel without waiting, renders, and publishes position and state through atomics. It
//! does not allocate, lock or do I/O (AGENTS.md, audio thread rules); a sequence it
//! replaces goes back to the thread to be freed there. A panic inside it is caught: the
//! output goes silent and the error is reported.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use gscore_play::{PlayError, PlayState, Player, Sequence};

use crate::synth::{SoundBank, SynthEngine};
use crate::transport::Command;

/// Commands queued before senders get an error (the callback drains them every few ms).
const QUEUE: usize = 64;
/// Replaced sequences waiting to be freed.
const RETIRED: usize = 8;
/// How often the thread frees replaced sequences and checks for the end.
const HOUSEKEEPING: Duration = Duration::from_millis(100);

/// What the thread and the callback report back.
#[derive(Debug, Default)]
struct Shared {
    position: AtomicU64,
    state: AtomicU8,
    /// The SoundFont is loading or the device opening.
    loading: AtomicBool,
    /// The device failed; the callback stops playback.
    failed: AtomicBool,
    error: Mutex<Option<String>>,
    device: Mutex<Option<String>>,
    bank: Mutex<Option<String>>,
}

impl Shared {
    fn report(&self, error: String) {
        *lock(&self.error) = Some(error);
        self.failed.store(true, Ordering::Release);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn encode(state: PlayState) -> u8 {
    match state {
        PlayState::Stopped => 0,
        PlayState::Playing => 1,
        PlayState::Paused => 2,
    }
}

fn decode(state: u8) -> PlayState {
    match state {
        1 => PlayState::Playing,
        2 => PlayState::Paused,
        _ => PlayState::Stopped,
    }
}

/// Plays sequences with a SoundFont on the default audio output.
#[derive(Debug)]
pub struct SynthPlayer {
    soundfont: PathBuf,
    commands: SyncSender<Command>,
    shared: Arc<Shared>,
    /// Dropping it ends the thread (and the stream).
    quit: Option<SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl SynthPlayer {
    /// Starts loading `soundfont` and opening the default output device in the background.
    /// Commands sent meanwhile are carried out once it is ready; failures show in
    /// [`SynthPlayer::error`].
    pub fn start(soundfont: &Path) -> Result<Self, PlayError> {
        let (commands, receiver) = mpsc::sync_channel(QUEUE);
        let (quit, quit_receiver) = mpsc::sync_channel(1);
        let shared = Arc::new(Shared::default());
        shared.loading.store(true, Ordering::Release);
        let path = soundfont.to_path_buf();
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("gscore-synth".to_owned())
            .spawn(move || run(&path, receiver, &quit_receiver, &thread_shared))
            .map_err(|e| PlayError::Device(format!("cannot start the synthesizer: {e}")))?;
        Ok(SynthPlayer {
            soundfont: soundfont.to_path_buf(),
            commands,
            shared,
            quit: Some(quit),
            thread: Some(thread),
        })
    }

    /// The SoundFont file it plays with.
    pub fn soundfont(&self) -> &Path {
        &self.soundfont
    }

    /// The SoundFont's own name, once loaded (may be empty).
    pub fn bank_name(&self) -> Option<String> {
        lock(&self.shared.bank).clone()
    }

    /// The audio device's name, once it is open.
    pub fn device(&self) -> Option<String> {
        lock(&self.shared.device).clone()
    }

    /// Whether the SoundFont is still loading (or the device opening).
    pub fn is_loading(&self) -> bool {
        self.shared.loading.load(Ordering::Acquire)
    }

    /// Why the synthesizer cannot play, if it cannot (the SoundFont or the device failed).
    pub fn error(&self) -> Option<String> {
        lock(&self.shared.error).clone()
    }

    fn send(&self, command: Command) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.commands.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => {
                PlayError::Device("the synthesizer is not keeping up; try again".to_owned())
            }
            TrySendError::Disconnected(_) => self
                .error()
                .map_or(PlayError::Disconnected, PlayError::Device),
        })
    }
}

impl Player for SynthPlayer {
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
        self.shared.position.load(Ordering::Acquire)
    }

    fn state(&self) -> PlayState {
        decode(self.shared.state.load(Ordering::Acquire))
    }
}

impl Drop for SynthPlayer {
    fn drop(&mut self) {
        // Closing the quit channel ends the thread, which drops the stream.
        drop(self.quit.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The synth thread: loads, opens the device, then keeps the stream alive and frees what
/// the callback hands back until the player is dropped.
fn run(path: &Path, commands: Receiver<Command>, quit: &Receiver<()>, shared: &Arc<Shared>) {
    let (retired_sender, retired) = mpsc::sync_channel::<Arc<Sequence>>(RETIRED);
    let stream = SoundBank::open(path).and_then(|bank| {
        *lock(&shared.bank) = Some(bank.name().to_owned());
        open_stream(&bank, commands, retired_sender, shared)
    });
    shared.loading.store(false, Ordering::Release);
    let _stream = match stream {
        Ok(stream) => stream,
        Err(error) => {
            *lock(&shared.error) = Some(error.to_string());
            return;
        }
    };
    // Until the player is dropped (the quit channel closes).
    while let Err(RecvTimeoutError::Timeout) = quit.recv_timeout(HOUSEKEEPING) {
        while retired.try_recv().is_ok() {}
    }
}

/// Opens the default output device and starts a stream rendering `bank`.
fn open_stream(
    bank: &SoundBank,
    commands: Receiver<Command>,
    retired: SyncSender<Arc<Sequence>>,
    shared: &Arc<Shared>,
) -> Result<Stream, PlayError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| PlayError::Device("no audio output device found".to_owned()))?;
    let name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "the default audio output".to_owned());
    let supported = device
        .default_output_config()
        .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let engine = SynthEngine::new(bank, config.sample_rate)?;
    let parts = Parts {
        engine,
        commands,
        retired,
        shared: Arc::clone(shared),
        channels: usize::from(config.channels),
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, parts),
        SampleFormat::F64 => build::<f64>(&device, &config, parts),
        SampleFormat::I16 => build::<i16>(&device, &config, parts),
        SampleFormat::I32 => build::<i32>(&device, &config, parts),
        SampleFormat::U16 => build::<u16>(&device, &config, parts),
        SampleFormat::U8 => build::<u8>(&device, &config, parts),
        other => Err(PlayError::Device(format!(
            "{name} wants {other} samples, which the synthesizer cannot produce"
        ))),
    }
    .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    stream
        .play()
        .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    *lock(&shared.device) = Some(name);
    Ok(stream)
}

/// Everything the audio callback owns.
struct Parts {
    engine: SynthEngine,
    commands: Receiver<Command>,
    retired: SyncSender<Arc<Sequence>>,
    shared: Arc<Shared>,
    channels: usize,
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut parts: Parts,
) -> Result<Stream, PlayError>
where
    T: SizedSample + FromSample<f32>,
{
    let errors = Arc::clone(&parts.shared);
    let mut broken = false;
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if broken {
                    data.fill(T::EQUILIBRIUM);
                    return;
                }
                let rendered = catch_unwind(AssertUnwindSafe(|| callback(&mut parts, data)));
                if rendered.is_err() {
                    // The panic hook wrote a report; stay silent from now on.
                    broken = true;
                    data.fill(T::EQUILIBRIUM);
                    parts.shared.failed.store(true, Ordering::Release);
                    parts
                        .shared
                        .state
                        .store(encode(PlayState::Stopped), Ordering::Release);
                    if let Ok(mut error) = parts.shared.error.try_lock() {
                        *error = Some("the synthesizer failed unexpectedly".to_owned());
                    }
                }
            },
            move |error: cpal::Error| match error.kind() {
                // Glitches the stream survives.
                cpal::ErrorKind::Xrun
                | cpal::ErrorKind::RealtimeDenied
                | cpal::ErrorKind::DeviceChanged => {}
                _ => errors.report(format!("audio output failed: {error}")),
            },
            None,
        )
        .map_err(|e| PlayError::Device(e.to_string()))
}

/// One audio callback: commands, then samples, then the status.
fn callback<T>(parts: &mut Parts, data: &mut [T])
where
    T: SizedSample + FromSample<f32>,
{
    while let Ok(command) = parts.commands.try_recv() {
        if let Some(old) = parts.engine.apply(command) {
            // Freed by the thread; if its queue is full, here (rare and small).
            let _ = parts.retired.try_send(old);
        }
    }
    if parts.shared.failed.load(Ordering::Acquire) && parts.engine.state() == PlayState::Playing {
        parts.engine.fail();
    }
    parts
        .engine
        .render_interleaved(data, parts.channels, <T as Sample>::from_sample::<f32>);
    let shared = &parts.shared;
    shared
        .position
        .store(parts.engine.position(), Ordering::Release);
    shared
        .state
        .store(encode(parts.engine.state()), Ordering::Release);
}
