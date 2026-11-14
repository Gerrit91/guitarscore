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

use gscore_guitar::ChainSpec;
use gscore_play::StringKind;

use crate::chains::{ChainKnobs, prepare_chain};
use crate::synth::{Retired, SoundBank, SynthEngine};
use crate::transport::Command;

/// Commands queued before senders get an error (the callback drains them every few ms).
const QUEUE: usize = 64;
/// How long (microseconds) a sent play, pause or stop stands for the state while the
/// callback has not carried it out: a few buffers at most, unless the device is stuck.
const PENDING_AT_MOST: u64 = 300_000;
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
    /// The position carried on between callbacks, for the screen.
    clock: crate::clock::Clock,
    /// How many play, pause, stop and load commands the callback has carried out.
    transports: AtomicU64,
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
    /// The master's chain (after the tracks and buses).
    master: bool,
    /// A drum track's room.
    room: bool,
    spec: ChainSpec,
    knobs: ChainKnobs,
}

/// The reverb bus to prepare on the synth's thread.
#[derive(Debug)]
struct BusRequest {
    bus: crate::Bus,
    spec: ChainSpec,
    knobs: ChainKnobs,
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
    /// How many play, pause, stop and load commands were sent, and the state the last one
    /// asks for: until the callback has carried them out, that is the state.
    transports: AtomicU64,
    requested: AtomicU8,
    /// When the last of them was sent (microseconds since `epoch`): past
    /// [`PENDING_AT_MOST`], the callback's own state counts again.
    asked_at: AtomicU64,
    epoch: Instant,
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
            transports: AtomicU64::new(0),
            requested: AtomicU8::new(encode(PlayState::Stopped)),
            asked_at: AtomicU64::new(0),
            epoch: Instant::now(),
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
        let asks = transport_state(&command);
        self.commands.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => {
                PlayError::Device("the synthesizer is not keeping up; try again".to_owned())
            }
            TrySendError::Disconnected(_) => self
                .error()
                .map_or(PlayError::Disconnected, PlayError::Device),
        })?;
        if let Some(state) = asks {
            self.requested.store(encode(state), Ordering::Release);
            self.asked_at.store(self.micros(), Ordering::Release);
            self.transports.fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
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
        knobs: ChainKnobs,
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track,
                part: None,
                master: false,
                room: false,
                spec,
                knobs,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// Gives the master the chain `spec` with `settings`, prepared on the synth's own
    /// thread, then heard. It runs after the tracks and buses, before the metronome's
    /// click. An empty chain removes it.
    pub fn set_master_chain(&self, spec: ChainSpec, knobs: ChainKnobs) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track: 0,
                part: None,
                master: true,
                room: false,
                spec,
                knobs,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// New knob values for the master's chain, heard at once.
    pub fn set_master_chain_settings(&self, knobs: ChainKnobs) -> Result<(), PlayError> {
        self.send(Command::MasterChainSettings(knobs))
    }

    /// Gives `part` of the song's drum track `track` the chain `spec` with `settings`,
    /// prepared on the synth's own thread, then heard. An empty chain removes it.
    pub fn set_part_chain(
        &self,
        track: usize,
        part: usize,
        spec: ChainSpec,
        knobs: ChainKnobs,
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track,
                part: Some(part),
                master: false,
                room: false,
                spec,
                knobs,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// New knob values for `part` of the song's drum track `track`, heard at once.
    pub fn set_part_chain_settings(
        &self,
        track: usize,
        part: usize,
        knobs: ChainKnobs,
    ) -> Result<(), PlayError> {
        self.send(Command::PartChainSettings(track, part, knobs))
    }

    /// The mixing level of `part` of the song's drum track `track` (0-127, 127 unity),
    /// heard at once.
    pub fn set_part_level(&self, track: usize, part: usize, level: u8) -> Result<(), PlayError> {
        self.send(Command::PartLevel(track, part, level))
    }

    /// The pan of `part` of the song's drum track `track` (0 left, 64 centre, 127 right),
    /// heard at once.
    pub fn set_part_pan(&self, track: usize, part: usize, pan: u8) -> Result<(), PlayError> {
        self.send(Command::PartPan(track, part, pan))
    }

    /// Mutes or unmutes `part` of the song's drum track `track`, heard at once.
    pub fn set_part_mute(&self, track: usize, part: usize, muted: bool) -> Result<(), PlayError> {
        self.send(Command::PartMute(track, part, muted))
    }

    /// Soloes or un-soloes `part` of the song's drum track `track`, heard at once.
    pub fn set_part_solo(&self, track: usize, part: usize, soloed: bool) -> Result<(), PlayError> {
        self.send(Command::PartSolo(track, part, soloed))
    }

    /// New knob values for the effect chain of `track`, heard at once.
    pub fn set_chain_settings(&self, track: usize, knobs: ChainKnobs) -> Result<(), PlayError> {
        self.send(Command::ChainSettings(track, knobs))
    }

    /// Gives the modelled tracks the reverb bus `spec` (a chain that runs all wet), prepared
    /// on the synth's own thread with `settings`, at `level` (0-127, 127 unity) and muted or
    /// not.
    pub fn set_reverb_bus(
        &self,
        spec: ChainSpec,
        knobs: ChainKnobs,
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.set_bus(crate::Bus::Reverb, spec, knobs, level, muted)
    }

    /// New knob values, level and mute for the reverb bus, heard at once.
    pub fn set_reverb_bus_settings(
        &self,
        knobs: ChainKnobs,
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.set_bus_settings(crate::Bus::Reverb, knobs, level, muted)
    }

    /// Gives `bus` the chain `spec` with `settings`, its level and mute; prepared on the
    /// synth's own thread, then heard. An empty chain removes it.
    pub fn set_bus(
        &self,
        bus: crate::Bus,
        spec: ChainSpec,
        knobs: ChainKnobs,
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
                knobs,
                level,
                muted,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// New knob values, level and mute for `bus`, heard at once.
    pub fn set_bus_settings(
        &self,
        bus: crate::Bus,
        knobs: ChainKnobs,
        level: u8,
        muted: bool,
    ) -> Result<(), PlayError> {
        self.send(Command::BusSettings(bus, knobs, level, muted))
    }

    /// The song's track `track` sends `value` (0-127) into the compressor bus.
    pub fn set_comp_send(&self, track: usize, value: u8) -> Result<(), PlayError> {
        self.send(Command::CompSend(track, value))
    }

    /// Sets the input trim of the song's track `track`, in dB, before its chain.
    pub fn set_trim(&self, track: usize, db: f32) -> Result<(), PlayError> {
        self.send(Command::Trim(track, db))
    }

    /// Gives the drum kit `track` its room, prepared on the synth's own thread from `spec`
    /// and `knobs` (one short shared room behind the kit's parts).
    pub fn set_track_room(
        &self,
        track: usize,
        spec: ChainSpec,
        knobs: ChainKnobs,
    ) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        self.chains
            .try_send(ChainRequest {
                track,
                part: None,
                master: false,
                room: true,
                spec,
                knobs,
            })
            .map_err(|_| PlayError::Device("the synthesizer is not keeping up".to_owned()))
    }

    /// A drum part's send into its kit's room (0-127).
    pub fn set_part_room(&self, track: usize, part: usize, value: u8) -> Result<(), PlayError> {
        self.send(Command::PartRoom(track, part, value))
    }
}

/// What the synth's thread looks after besides the stream.
struct Housekeeping {
    quit: Receiver<()>,
    chains: Receiver<ChainRequest>,
    bus: Receiver<BusRequest>,
    commands: SyncSender<Command>,
}

impl SynthPlayer {
    /// Microseconds since the player was made.
    fn micros(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_micros()).unwrap_or(u64::MAX)
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

    fn send_event(&mut self, event: MidiEvent) -> Result<(), PlayError> {
        self.send(Command::Send(event))
    }

    fn replace(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        self.send(Command::Replace(sequence))
    }

    fn position(&self) -> u64 {
        self.shared.position.load(Ordering::Acquire)
    }

    fn display_position(&self) -> u64 {
        if self.state() == PlayState::Playing {
            self.shared.clock.now()
        } else {
            self.position()
        }
    }

    fn state(&self) -> PlayState {
        // A pause or stop the callback has not carried out yet: what it asks for, so the
        // app does not see the old state for another buffer. Not while loading (the
        // commands wait for the device).
        let running = !self.shared.loading.load(Ordering::Acquire)
            && !self.shared.failed.load(Ordering::Acquire)
            && self.shared.sample_rate.load(Ordering::Acquire) > 0
            && self.error().is_none();
        let recent = self
            .micros()
            .saturating_sub(self.asked_at.load(Ordering::Acquire))
            < PENDING_AT_MOST;
        if running
            && recent
            && self.shared.transports.load(Ordering::Acquire)
                < self.transports.load(Ordering::Acquire)
        {
            // Only a stop or pause: a play is not playing until its seek is done, or the
            // position would still be the old one.
            let requested = decode(self.requested.load(Ordering::Acquire));
            if requested != PlayState::Playing {
                return requested;
            }
        }
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
                let chain = prepare_chain(&request.spec, &request.knobs, rate);
                let command = if request.master {
                    Command::MasterChain(chain, Box::new(request.knobs))
                } else if request.room {
                    Command::TrackRoom(request.track, chain)
                } else {
                    match request.part {
                        None => Command::Chain(request.track, chain, Box::new(request.knobs)),
                        Some(part) => {
                            Command::PartChain(request.track, part, chain, Box::new(request.knobs))
                        }
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
            let chain = prepare_chain(&request.spec, &request.knobs, rate);
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
                    request.knobs,
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
    // Counted as carried out only once the state they leave is published (below).
    let mut transports = 0_u64;
    while let Ok(command) = parts.commands.try_recv() {
        if transport_state(&command).is_some() {
            transports = transports.saturating_add(1);
        }
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
    let start = parts.engine.position();
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
    let playing = parts.engine.state() == PlayState::Playing;
    shared
        .clock
        .publish(start, parts.engine.bpm().filter(|_| playing));
    shared
        .state
        .store(encode(parts.engine.state()), Ordering::Release);
    if transports > 0 {
        shared.transports.fetch_add(transports, Ordering::AcqRel);
    }
}

/// The state a play, pause, stop or load command leaves the transport in; `None` for the
/// other commands.
fn transport_state(command: &Command) -> Option<PlayState> {
    match command {
        Command::Play => Some(PlayState::Playing),
        Command::Pause => Some(PlayState::Paused),
        Command::Stop | Command::Load(_) => Some(PlayState::Stopped),
        _ => None,
    }
}
