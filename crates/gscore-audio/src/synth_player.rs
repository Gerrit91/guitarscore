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
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use gscore_play::{MidiEvent, PlayError, PlayState, Player, Sequence};

use gscore_guitar::{BLOCKS, BlockSettings, ChainSpec};
use gscore_play::StringKind;

use crate::chains::prepare_chain;
use crate::synth::{Retired, SoundBank, SynthEngine};
use crate::transport::Command;

/// Commands queued before senders get an error (the callback drains them every few ms).
const QUEUE: usize = 64;
/// Replaced sequences waiting to be freed.
const RETIRED: usize = 8;
/// How often the thread frees replaced sequences and checks for the end.
const HOUSEKEEPING: Duration = Duration::from_millis(100);
/// How much of the newest callback goes into the smoothed load (the rest is the old
/// value): about the last quarter second at common buffer sizes.
const LOAD_SMOOTHING: f32 = 0.05;

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
    /// The rate the device runs at (0 until it is open).
    sample_rate: AtomicU32,
    /// How long the audio takes to leave the device (microseconds), as the last callback
    /// reported.
    latency: AtomicU64,
    /// The audio callback's load: the share of each buffer's time spent rendering it,
    /// smoothed (`f32` bits).
    load: AtomicU32,
    /// Something to tell the user that is not an error (the rate chosen was not offered).
    notice: Mutex<Option<String>>,
    /// The engine's meters, once it runs (the engine is made on the audio thread).
    meters: Mutex<Option<Arc<crate::Meters>>>,
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

/// An effect chain to prepare on the synth's thread (it knows the device's rate).
#[derive(Debug)]
struct ChainRequest {
    track: usize,
    /// `None`: the track's own chain; `Some(part)`: one of its drum parts.
    part: Option<usize>,
    spec: ChainSpec,
    settings: [BlockSettings; BLOCKS],
}

/// The reverb bus to prepare on the synth's thread.
#[derive(Debug)]
struct BusRequest {
    bus: crate::Bus,
    spec: ChainSpec,
    settings: [BlockSettings; BLOCKS],
    level: u8,
    muted: bool,
}

/// Plays sequences with a SoundFont on the default audio output.
#[derive(Debug)]
pub struct SynthPlayer {
    soundfont: PathBuf,
    commands: SyncSender<Command>,
    /// Chains to prepare off the audio thread.
    chains: SyncSender<ChainRequest>,
    /// The reverb bus to prepare off the audio thread.
    bus: SyncSender<BusRequest>,
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
        SynthPlayer::start_at(soundfont, None)
    }

    /// Like [`SynthPlayer::start`], asking the device for `sample_rate` (`None`: its
    /// default). A rate the device does not offer falls back to its default, and
    /// [`SynthPlayer::notice`] says so.
    pub fn start_at(soundfont: &Path, sample_rate: Option<u32>) -> Result<Self, PlayError> {
        let (commands, receiver) = mpsc::sync_channel(QUEUE);
        let (chains, chain_requests) = mpsc::sync_channel(QUEUE);
        let (bus, bus_requests) = mpsc::sync_channel(QUEUE);
        let (quit, quit_receiver) = mpsc::sync_channel(1);
        let chain_commands = commands.clone();
        let shared = Arc::new(Shared::default());
        shared.loading.store(true, Ordering::Release);
        let path = soundfont.to_path_buf();
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("gscore-synth".to_owned())
            .spawn(move || {
                run(
                    &path,
                    sample_rate,
                    receiver,
                    &Housekeeping {
                        quit: quit_receiver,
                        chains: chain_requests,
                        bus: bus_requests,
                        commands: chain_commands,
                    },
                    &thread_shared,
                );
            })
            .map_err(|e| PlayError::Device(format!("cannot start the synthesizer: {e}")))?;
        Ok(SynthPlayer {
            soundfont: soundfont.to_path_buf(),
            commands,
            chains,
            bus,
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

    /// The sample rate the device plays at, once it is open.
    pub fn sample_rate(&self) -> Option<u32> {
        Some(self.shared.sample_rate.load(Ordering::Acquire)).filter(|&rate| rate > 0)
    }

    /// How long the sound takes to leave the audio device, once a callback has reported
    /// it. A hardware synthesizer's MIDI can be held back by as much so both line up.
    /// A device that reports an implausible figure is ignored.
    pub fn latency(&self) -> Option<Duration> {
        /// The most a MIDI-out track is held back; longer is a broken timestamp.
        const MAX_LATENCY: u64 = 250_000;
        let micros = self.shared.latency.load(Ordering::Acquire);
        (micros > 0 && micros <= MAX_LATENCY).then(|| Duration::from_micros(micros))
    }

    /// The mixer's meters (every track's real level, the reverb bus's and the master's),
    /// once the synthesizer runs.
    pub fn meters(&self) -> Option<Arc<crate::Meters>> {
        lock(&self.shared.meters).clone()
    }

    /// How busy the audio callback is: the share of each buffer's playing time spent
    /// computing it (0.5: half; above 1 the output drops out), smoothed. `None` until the
    /// device runs.
    pub fn audio_load(&self) -> Option<f32> {
        self.sample_rate()?;
        Some(f32::from_bits(self.shared.load.load(Ordering::Acquire)))
    }

    /// A message for the user that is not an error, once (the chosen sample rate was not
    /// available).
    pub fn notice(&self) -> Option<String> {
        lock(&self.shared.notice).take()
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

impl SynthPlayer {
    /// Makes the modelled strings of `kind` sound as `model`, heard at once (also while
    /// the SoundFont still loads: the change waits in line).
    pub fn set_string_model(
        &self,
        kind: StringKind,
        model: gscore_guitar::StringModel,
    ) -> Result<(), PlayError> {
        self.send(Command::StringModel(kind, model))
    }

    /// Gives `track` (an index of the song's tracks) its own string model (`None`: its
    /// kind's again), heard at once.
    pub fn set_track_model(
        &self,
        track: usize,
        model: Option<gscore_guitar::StringModel>,
    ) -> Result<(), PlayError> {
        self.send(Command::TrackModel(track, model))
    }

    /// Gives `track` the effect chain `spec` with `settings`. The chain (impulse responses
    /// resampled and transformed) is prepared on the synth's own thread and then heard; an
    /// empty chain removes it.
    pub fn set_chain(
        &self,
        track: usize,
        spec: ChainSpec,
        settings: [BlockSettings; BLOCKS],
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track,
                part: None,
                spec,
                settings,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// Gives `part` of the song's drum track `track` the chain `spec` with `settings`,
    /// prepared on the synth's own thread, then heard. An empty chain removes it.
    pub fn set_part_chain(
        &self,
        track: usize,
        part: usize,
        spec: ChainSpec,
        settings: [BlockSettings; BLOCKS],
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track,
                part: Some(part),
                spec,
                settings,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// New knob values for `part` of the song's drum track `track`, heard at once.
    pub fn set_part_chain_settings(
        &self,
        track: usize,
        part: usize,
        settings: [BlockSettings; BLOCKS],
    ) -> Result<(), PlayError> {
        self.send(Command::PartChainSettings(track, part, settings))
    }

    /// The mixing level of `part` of the song's drum track `track` (0-127, 127 unity),
    /// heard at once.
    pub fn set_part_level(&self, track: usize, part: usize, level: u8) -> Result<(), PlayError> {
        self.send(Command::PartLevel(track, part, level))
    }

    /// New knob values for the effect chain of `track`, heard at once.
    pub fn set_chain_settings(
        &self,
        track: usize,
        settings: [BlockSettings; BLOCKS],
    ) -> Result<(), PlayError> {
        self.send(Command::ChainSettings(track, settings))
    }

    /// Gives the modelled tracks the reverb bus `spec` (a chain that runs all wet), prepared
    /// on the synth's own thread with `settings`, at `level` (0-127, 127 unity) and muted or
    /// not.
    pub fn set_reverb_bus(
        &self,
        spec: ChainSpec,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.set_bus(crate::Bus::Reverb, spec, settings, level, muted)
    }

    /// New knob values, level and mute for the reverb bus, heard at once.
    pub fn set_reverb_bus_settings(
        &self,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.set_bus_settings(crate::Bus::Reverb, settings, level, muted)
    }

    /// Gives `bus` the chain `spec` with `settings`, its level and mute; prepared on the
    /// synth's own thread, then heard. An empty chain removes it.
    pub fn set_bus(
        &self,
        bus: crate::Bus,
        spec: ChainSpec,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.bus
            .try_send(BusRequest {
                bus,
                spec,
                settings,
                level,
                muted,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// New knob values, level and mute for `bus`, heard at once.
    pub fn set_bus_settings(
        &self,
        bus: crate::Bus,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.send(Command::BusSettings(bus, settings, level, muted))
    }

    /// The song's track `track` sends `value` (0-127) into the compressor bus.
    pub fn set_comp_send(&self, track: usize, value: u8) -> Result<(), PlayError> {
        self.send(Command::CompSend(track, value))
    }
}

/// What the synth's thread looks after besides the stream.
struct Housekeeping {
    quit: Receiver<()>,
    chains: Receiver<ChainRequest>,
    bus: Receiver<BusRequest>,
    commands: SyncSender<Command>,
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

    fn send_event(&mut self, event: MidiEvent) -> Result<(), PlayError> {
        self.send(Command::Send(event))
    }

    fn replace(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        self.send(Command::Replace(sequence))
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
fn run(
    path: &Path,
    sample_rate: Option<u32>,
    commands: Receiver<Command>,
    house: &Housekeeping,
    shared: &Arc<Shared>,
) {
    let (retired_sender, retired) = mpsc::sync_channel::<Retired>(RETIRED);
    let stream = SoundBank::open(path).and_then(|bank| {
        *lock(&shared.bank) = Some(bank.name().to_owned());
        open_stream(&bank, sample_rate, commands, retired_sender, shared)
    });
    // The error first: whoever sees loading end must also see why it failed.
    let _stream = match stream {
        Ok(stream) => {
            shared.loading.store(false, Ordering::Release);
            stream
        }
        Err(error) => {
            *lock(&shared.error) = Some(error.to_string());
            shared.loading.store(false, Ordering::Release);
            return;
        }
    };
    let rate = shared.sample_rate.load(Ordering::Acquire);
    // Until the player is dropped (the quit channel closes).
    while let Err(mpsc::TryRecvError::Empty) = house.quit.try_recv() {
        // Prepare chains as they are asked for (the newest request of a kind wins).
        match house.chains.recv_timeout(HOUSEKEEPING) {
            Ok(request) => {
                let chain = prepare_chain(&request.spec, &request.settings, rate);
                let command = match request.part {
                    None => Command::Chain(request.track, chain, Box::new(request.settings)),
                    Some(part) => {
                        Command::PartChain(request.track, part, chain, Box::new(request.settings))
                    }
                };
                // Waits for room in the queue: this thread may.
                if house.commands.send(command).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if !matches!(
                    house.quit.recv_timeout(HOUSEKEEPING),
                    Err(RecvTimeoutError::Timeout)
                ) {
                    break;
                }
            }
        }
        // The buses, prepared the same way (one instance each, shared by the tracks).
        while let Ok(request) = house.bus.try_recv() {
            let chain = prepare_chain(&request.spec, &request.settings, rate);
            if house
                .commands
                .send(Command::Bus(request.bus, chain))
                .is_err()
            {
                return;
            }
            if house
                .commands
                .send(Command::BusSettings(
                    request.bus,
                    request.settings,
                    request.level,
                    request.muted,
                ))
                .is_err()
            {
                break;
            }
        }
        while retired.try_recv().is_ok() {}
    }
}

/// Opens the default output device and starts a stream rendering `bank`.
fn open_stream(
    bank: &SoundBank,
    sample_rate: Option<u32>,
    commands: Receiver<Command>,
    retired: SyncSender<Retired>,
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
    let default = device
        .default_output_config()
        .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    let supported = match sample_rate {
        None => default,
        Some(rate) if rate == default.sample_rate() => default,
        Some(rate) => match at_rate(&device, &default, rate) {
            Some(config) => config,
            None => {
                *lock(&shared.notice) = Some(format!(
                    "{name} does not play at {} kHz; using {} kHz",
                    f64::from(rate) / 1000.0,
                    f64::from(default.sample_rate()) / 1000.0
                ));
                default
            }
        },
    };
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let engine = SynthEngine::new(bank, config.sample_rate)?;
    *lock(&shared.meters) = Some(engine.meters());
    shared
        .sample_rate
        .store(config.sample_rate, Ordering::Release);
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

/// A configuration of `device` at `rate`, preferring the default's sample format and
/// channel count.
fn at_rate(
    device: &cpal::Device,
    default: &cpal::SupportedStreamConfig,
    rate: u32,
) -> Option<cpal::SupportedStreamConfig> {
    let ranges: Vec<cpal::SupportedStreamConfigRange> =
        device.supported_output_configs().ok()?.collect();
    let rank = |range: &cpal::SupportedStreamConfigRange| {
        (
            range.sample_format() != default.sample_format(),
            range.channels() != default.channels(),
        )
    };
    let mut fitting: Vec<cpal::SupportedStreamConfigRange> = ranges
        .into_iter()
        .filter(|range| (range.min_sample_rate()..=range.max_sample_rate()).contains(&rate))
        .collect();
    fitting.sort_by_key(rank);
    fitting
        .into_iter()
        .find_map(|range| range.try_with_sample_rate(rate))
}

/// Everything the audio callback owns.
struct Parts {
    engine: SynthEngine,
    commands: Receiver<Command>,
    retired: SyncSender<Retired>,
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
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                if broken {
                    data.fill(T::EQUILIBRIUM);
                    return;
                }
                let rendered = catch_unwind(AssertUnwindSafe(|| callback(&mut parts, data, info)));
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
fn callback<T>(parts: &mut Parts, data: &mut [T], info: &cpal::OutputCallbackInfo)
where
    T: SizedSample + FromSample<f32>,
{
    let timing = info.timestamp();
    let latency = timing.playback.duration_since(timing.callback);
    parts.shared.latency.store(
        u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
        Ordering::Release,
    );
    while let Ok(command) = parts.commands.try_recv() {
        if let Some(old) = parts.engine.apply(command) {
            // Freed by the thread; if its queue is full, here (rare and small).
            let _ = parts.retired.try_send(old);
        }
    }
    if parts.shared.failed.load(Ordering::Acquire) && parts.engine.state() == PlayState::Playing {
        parts.engine.fail();
    }
    // Reading the clock neither blocks nor allocates.
    let started = Instant::now();
    parts
        .engine
        .render_interleaved(data, parts.channels, <T as Sample>::from_sample::<f32>);
    let spent = started.elapsed().as_secs_f32();
    let frames = data.len() / parts.channels.max(1);
    let available = frames as f32 / parts.engine.sample_rate().max(1) as f32;
    let shared = &parts.shared;
    if available > 0.0 {
        let now = spent / available;
        let old = f32::from_bits(shared.load.load(Ordering::Relaxed));
        let smoothed = old + (now - old) * LOAD_SMOOTHING;
        shared.load.store(smoothed.to_bits(), Ordering::Release);
    }
    shared
        .position
        .store(parts.engine.position(), Ordering::Release);
    shared
        .state
        .store(encode(parts.engine.state()), Ordering::Release);
}
