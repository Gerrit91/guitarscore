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

use gscore_play::{MidiEvent, PlayError, PlayState, Sequence};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::transport::{Command, Sink, Transport};

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
        self.apply(Command::Load(sequence))
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

    /// Carries out a player command now.
    pub(crate) fn apply(&mut self, command: Command) -> Option<Arc<Sequence>> {
        let now = self.now();
        self.transport
            .handle(command, now, &mut SynthSink(&mut self.synth))
    }

    /// Stops playing where it is, after the output failed.
    pub(crate) fn fail(&mut self) {
        self.transport.fail(&mut SynthSink(&mut self.synth));
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
        self.transport.advance(now, &mut SynthSink(&mut self.synth));
        if let (Some(l), Some(r)) = (self.left.get_mut(..frames), self.right.get_mut(..frames)) {
            self.synth.render(l, r);
        }
        self.frames = self.frames.saturating_add(frames as u64);
    }
}

/// Sends events to the synthesizer.
struct SynthSink<'a>(&'a mut Synthesizer);

impl Sink for SynthSink<'_> {
    fn send(&mut self, event: MidiEvent) {
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
        // Notes fade out with their release instead of clicking off. Every channel: this
        // allocates nothing, unlike collecting the sequence's channels.
        self.0.note_off_all(false);
        for channel in 0..CHANNELS {
            self.0
                .process_midi_message(i32::from(channel), 0xE0, BEND_CENTER.0, BEND_CENTER.1);
        }
    }
}
