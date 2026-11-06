//! The internal synthesizer: plays sequences with a SoundFont through `rustysynth`, without
//! an audio device. [`SynthEngine`] renders samples on request; the [`crate::SynthPlayer`]
//! calls it from the audio callback, tests and offline rendering call it directly.
//!
//! The engine's clock is the number of frames rendered, so event timing is exact to one
//! rendering block (64 frames, about 1.5 ms at 44.1 kHz) whatever the device's buffer size.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;

use gscore_guitar::{BLOCKS, BlockSettings, Chain, StringModel};

use gscore_play::{Channel, MidiEvent, PlayError, PlayState, Sequence, StringKind, TimedEvent};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::click::ClickVoice;
use crate::guitars::Guitars;
use crate::transport::{Command, Sink, Transport};

/// Something the engine no longer uses, handed back so it is freed off the audio thread.
#[derive(Debug)]
pub(crate) enum Retired {
    Sequence(Arc<Sequence>),
    // Only held until it is dropped elsewhere.
    #[allow(dead_code)]
    Chain(Box<Chain>),
    #[allow(dead_code)]
    ReverbBus(Box<Chain>),
}

/// Frames rendered between two looks at the schedule.
pub const BLOCK: usize = 64;
/// MIDI channels of the synthesizer.
const CHANNELS: u8 = 16;
/// The synthesizer's own volume (rustysynth's default is 0.5); the output is also clipped,
/// so several loud tracks never wrap around.
const MASTER_VOLUME: f32 = 0.5;
/// The pitch bend wheel at rest, as MIDI data bytes (LSB, MSB).
const BEND_CENTER: (i32, i32) = (0, 64);

/// A loaded SoundFont (`.sf2`), shareable between synthesizers.
#[derive(Clone)]
pub struct SoundBank {
    font: Arc<SoundFont>,
    name: String,
}

impl std::fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBank")
            .field("name", &self.name)
            .field("presets", &self.preset_count())
            .finish()
    }
}

impl SoundBank {
    /// Reads the SoundFont file at `path`.
    pub fn open(path: &Path) -> Result<Self, PlayError> {
        let file = File::open(path)
            .map_err(|e| PlayError::Device(format!("cannot open {}: {e}", path.display())))?;
        SoundBank::read(&mut BufReader::new(file)).map_err(|e| match e {
            PlayError::Device(e) => PlayError::Device(format!("{}: {e}", path.display())),
            other => other,
        })
    }

    /// Reads a SoundFont from bytes (a browser fetch or the file a user picked).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PlayError> {
        SoundBank::read(&mut &bytes[..])
    }

    /// Reads a SoundFont from `reader`.
    pub fn read(reader: &mut impl Read) -> Result<Self, PlayError> {
        let font = SoundFont::new(reader)
            .map_err(|e| PlayError::Device(format!("not a usable SoundFont (.sf2) file: {e}")))?;
        let name = font.get_info().get_bank_name().trim().to_owned();
        Ok(SoundBank {
            font: Arc::new(font),
            name,
        })
    }

    /// The name stored in the SoundFont (may be empty).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// How many presets (instruments) it has.
    pub fn preset_count(&self) -> usize {
        self.font.get_presets().len()
    }
}

/// Plays sequences into sample buffers. All methods are cheap and never block; `render`
/// does not allocate, so it may run in an audio callback.
pub struct SynthEngine {
    synth: Synthesizer,
    transport: Transport,
    sample_rate: u32,
    /// Frames rendered so far: the engine's clock.
    frames: u64,
    /// The metronome's own click (keys 33 and 34 on the percussion channel).
    click: ClickVoice,
    /// The modelled strings, for tracks the sequence gives to them.
    guitars: Guitars,
    /// One block of samples per side, rendered before interleaving.
    left: [f32; BLOCK],
    right: [f32; BLOCK],
}

impl std::fmt::Debug for SynthEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynthEngine")
            .field("sample_rate", &self.sample_rate)
            .field("frames", &self.frames)
            .field("state", &self.transport.state())
            .finish_non_exhaustive()
    }
}

impl SynthEngine {
    /// An engine playing with `bank` at `sample_rate` (16 000 to 192 000 Hz).
    pub fn new(bank: &SoundBank, sample_rate: u32) -> Result<Self, PlayError> {
        let rate = i32::try_from(sample_rate)
            .map_err(|_| PlayError::Device(format!("sample rate {sample_rate} is too high")))?;
        let mut settings = SynthesizerSettings::new(rate);
        settings.block_size = BLOCK;
        let mut synth = Synthesizer::new(&bank.font, &settings)
            .map_err(|e| PlayError::Device(format!("cannot start the synthesizer: {e}")))?;
        synth.set_master_volume(MASTER_VOLUME);
        Ok(SynthEngine {
            synth,
            transport: Transport::default(),
            sample_rate,
            frames: 0,
            click: ClickVoice::new(sample_rate),
            guitars: Guitars::new(sample_rate),
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
        })
    }

    /// The sample rate it renders at.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Makes `sequence` the one to play, stopped at its start. Returns the sequence it
    /// replaces, so an audio callback can hand it elsewhere to be freed.
    pub fn load(&mut self, sequence: Arc<Sequence>) -> Option<Arc<Sequence>> {
        match self.apply(Command::Load(sequence)) {
            Some(Retired::Sequence(old)) => Some(old),
            _ => None,
        }
    }

    /// Starts or resumes playing.
    pub fn play(&mut self) {
        let _ = self.apply(Command::Play);
    }

    /// Pauses, keeping the position.
    pub fn pause(&mut self) {
        let _ = self.apply(Command::Pause);
    }

    /// Stops and goes back to the start.
    pub fn stop(&mut self) {
        let _ = self.apply(Command::Stop);
    }

    /// Moves to `tick`.
    pub fn seek(&mut self, tick: u64) {
        let _ = self.apply(Command::Seek(tick));
    }

    /// Plays faster (above 1) or slower (below 1) than written.
    pub fn set_tempo_scale(&mut self, scale: f32) {
        let _ = self.apply(Command::TempoScale(scale));
    }

    /// Swaps in a rebuilt sequence where playback is (a live mixer change).
    pub fn replace(&mut self, sequence: Arc<Sequence>) {
        let _ = self.apply(Command::Replace(sequence));
    }

    /// Sends one MIDI message now (a live mixer change).
    pub fn send_event(&mut self, event: MidiEvent) {
        let _ = self.apply(Command::Send(event));
    }

    /// The current position in ticks.
    pub fn position(&self) -> u64 {
        self.transport.position()
    }

    /// Playing, paused or stopped.
    pub fn state(&self) -> PlayState {
        self.transport.state()
    }

    /// Renders the next `left.len()` frames (sounds keep ringing out after a stop). The
    /// slices must be equally long; extra samples of the longer one are set to silence.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let (left, rest_left) = left.split_at_mut(frames);
        let (right, rest_right) = right.split_at_mut(frames);
        rest_left.fill(0.0);
        rest_right.fill(0.0);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            self.render_block(l.len());
            let len = l.len();
            l.copy_from_slice(self.left.get(..len).unwrap_or_default());
            r.copy_from_slice(self.right.get(..len).unwrap_or_default());
        }
    }

    /// Renders into an interleaved buffer of `channels` per frame: left and right go to the
    /// first two channels (mono gets their mix, further channels silence). Samples are
    /// clipped to -1..1 and converted with `convert`.
    pub fn render_interleaved<T>(
        &mut self,
        output: &mut [T],
        channels: usize,
        convert: impl Fn(f32) -> T,
    ) {
        let channels = channels.max(1);
        for frames in output.chunks_mut(BLOCK.saturating_mul(channels)) {
            let count = frames.len() / channels;
            self.render_block(count);
            for (index, frame) in frames.chunks_mut(channels).enumerate() {
                let l = self
                    .left
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(-1.0, 1.0);
                let r = self
                    .right
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(-1.0, 1.0);
                match frame {
                    [mono] => *mono = convert(0.5 * (l + r)),
                    [first, second, rest @ ..] => {
                        *first = convert(l);
                        *second = convert(r);
                        for sample in rest {
                            *sample = convert(0.0);
                        }
                    }
                    [] => {}
                }
            }
        }
    }

    /// Makes the modelled strings of `kind` sound as `model` (heard at once).
    pub fn set_string_model(&mut self, kind: StringKind, model: StringModel) {
        self.guitars.set_model(kind, model);
    }

    /// Gives `track` (an index of the song's tracks) its own string model (`None`:
    /// its kind's again), heard at once.
    pub fn set_track_model(&mut self, track: usize, model: Option<StringModel>) {
        self.guitars.set_track_model(track, model);
    }

    /// Gives `track` the effect chain `chain` (`None`: none, see
    /// [`crate::prepare_chain`]) with the knob values `settings`, heard at once. Returns
    /// the chain it replaces, to be freed elsewhere when this runs in an audio callback.
    pub fn set_chain(
        &mut self,
        track: usize,
        chain: Option<Box<Chain>>,
        settings: [BlockSettings; BLOCKS],
    ) -> Option<Box<Chain>> {
        self.guitars.set_chain(track, chain, settings)
    }

    /// New knob values for the effect chain of `track`, heard at once.
    pub fn set_chain_settings(&mut self, track: usize, settings: [BlockSettings; BLOCKS]) {
        self.guitars.set_chain_settings(track, settings);
    }

    /// Gives the tracks a reverb bus (`None`: none), a prepared chain that runs all wet.
    /// Returns the one it replaces, to be freed elsewhere.
    pub fn set_reverb_bus(&mut self, chain: Option<Box<Chain>>) -> Option<Box<Chain>> {
        self.guitars
            .set_reverb_bus(chain, [BlockSettings::default(); BLOCKS])
    }

    /// New knob values, level and mute for the reverb bus, heard at once.
    pub fn set_reverb_bus_settings(
        &mut self,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) {
        self.guitars.set_reverb_bus_settings(settings, level, muted);
    }

    /// Carries out a player command now. What it replaces comes back, to be freed off
    /// the audio thread.
    pub(crate) fn apply(&mut self, command: Command) -> Option<Retired> {
        match command {
            Command::StringModel(kind, model) => {
                self.set_string_model(kind, model);
                return None;
            }
            Command::TrackModel(track, model) => {
                self.set_track_model(track, model);
                return None;
            }
            Command::Chain(track, chain, settings) => {
                return self.set_chain(track, chain, *settings).map(Retired::Chain);
            }
            Command::ChainSettings(track, settings) => {
                self.set_chain_settings(track, settings);
                return None;
            }
            Command::ReverbBus(chain) => {
                return self.set_reverb_bus(chain).map(Retired::ReverbBus);
            }
            Command::ReverbBusSettings(settings, level, muted) => {
                self.set_reverb_bus_settings(settings, level, muted);
                return None;
            }
            _ => {}
        }
        let now = self.now();
        self.transport
            .handle(
                command,
                now,
                &mut SynthSink(&mut self.synth, &mut self.click, &mut self.guitars),
            )
            .map(Retired::Sequence)
    }

    /// Stops playing where it is, after the output failed.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the worklet stops via `stop`
    pub(crate) fn fail(&mut self) {
        self.transport.fail(&mut SynthSink(
            &mut self.synth,
            &mut self.click,
            &mut self.guitars,
        ));
    }

    /// The clock: microseconds of rendered audio.
    fn now(&self) -> u64 {
        let micros = u128::from(self.frames) * 1_000_000 / u128::from(self.sample_rate.max(1));
        u64::try_from(micros).unwrap_or(u64::MAX)
    }

    /// Sends what is due, then renders `frames` (at most [`BLOCK`]) into the block buffers.
    fn render_block(&mut self, frames: usize) {
        let frames = frames.min(BLOCK);
        let now = self.now();
        if let Some(sequence) = self.transport.sequence() {
            self.guitars.follow(sequence);
        }
        self.transport.advance(
            now,
            &mut SynthSink(&mut self.synth, &mut self.click, &mut self.guitars),
        );
        if let (Some(l), Some(r)) = (self.left.get_mut(..frames), self.right.get_mut(..frames)) {
            self.synth.render(l, r);
            self.guitars.render(
                self.transport.sequence(),
                self.transport.position(),
                self.transport.state() == PlayState::Playing,
                l,
                r,
            );
            self.click.add(l, r);
        }
        self.frames = self.frames.saturating_add(frames as u64);
    }
}

/// Sends events to the synthesizer; the metronome's keys go to its own click, and the
/// notes of tracks the modelled strings play go nowhere (the strings play them).
struct SynthSink<'a>(&'a mut Synthesizer, &'a mut ClickVoice, &'a mut Guitars);

impl Sink for SynthSink<'_> {
    fn send_timed(&mut self, event: &TimedEvent) {
        // Note-offs still go through: a note the SoundFont began before the track was
        // given to the strings (switched while playing) must end.
        let note = matches!(
            event.event,
            MidiEvent::NoteOn { .. } | MidiEvent::PitchBend { .. }
        );
        if note && self.2.plays(event.track) {
            return;
        }
        self.send(event.event);
    }

    fn send(&mut self, event: MidiEvent) {
        match event {
            MidiEvent::NoteOn {
                channel: Channel::PERCUSSION,
                key,
                velocity,
            } if ClickVoice::plays(key) => {
                self.1.trigger(key, velocity);
                return;
            }
            MidiEvent::NoteOff {
                channel: Channel::PERCUSSION,
                key,
            } if ClickVoice::plays(key) => return,
            MidiEvent::Control {
                channel: Channel::PERCUSSION,
                controller,
                value,
            } if ClickVoice::chooses(controller) => {
                self.1.choose(value);
                return;
            }
            _ => {}
        }
        let (channel, command, data1, data2) = match event {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => (channel, 0x90, key, velocity),
            MidiEvent::NoteOff { channel, key } => (channel, 0x80, key, 0),
            MidiEvent::ProgramChange { channel, program } => (channel, 0xC0, program, 0),
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => (channel, 0xB0, controller, value),
            MidiEvent::PitchBend { channel, value } => {
                let value = value.min(0x3FFF);
                let lsb = u8::try_from(value & 0x7F).unwrap_or(0);
                let msb = u8::try_from(value >> 7).unwrap_or(64);
                (channel, 0xE0, lsb, msb)
            }
        };
        self.0.process_midi_message(
            i32::from(channel.get()),
            command,
            i32::from(data1 & 0x7F),
            i32::from(data2 & 0x7F),
        );
    }

    fn silence(&mut self, _: &Sequence) {
        self.2.silence();
        // Notes fade out with their release instead of clicking off. Every channel: this
        // allocates nothing, unlike collecting the sequence's channels.
        self.0.note_off_all(false);
        for channel in 0..CHANNELS {
            self.0
                .process_midi_message(i32::from(channel), 0xE0, BEND_CENTER.0, BEND_CENTER.1);
        }
    }
}
