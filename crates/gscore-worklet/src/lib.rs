//! The synthesizer that runs inside the browser's AudioWorklet.
//!
//! An AudioWorklet runs on the Web Audio thread and cannot share the app's wasm instance,
//! so this is a second, tiny module: it owns a [`SynthEngine`] and exposes it to
//! `web/worklet.js` through `wasm-bindgen`. The engine is the same pure-Rust code the
//! desktop plays through `cpal`, so techniques (bends, slides, palm mute, ...) sound the
//! same.
//!
//! The worklet gets a song as `gscore_play::encode_sequence` bytes and a SoundFont as
//! bytes. A SoundFont can arrive after a sequence (a fast click on Play while the font
//! still downloads): the sequence, and what was asked meanwhile (play, seek, tempo),
//! wait for the engine.

// The browser's sample rate and tick position cross the JS boundary as `f64`; the casts
// are clamped first and checked where a wrong value would matter.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use gscore_audio::{SoundBank, SoundMessage, SynthEngine, decode_sound};
use gscore_play::{PlayState, Sequence, decode_sequence, encode_sequence};

/// The synthesizer, as the worklet's JavaScript sees it.
#[wasm_bindgen]
pub struct Synth {
    sample_rate: u32,
    engine: Option<SynthEngine>,
    /// The song last loaded: it waits for the first SoundFont and is loaded again into
    /// the engine of a SoundFont that replaces the previous one.
    sequence: Option<Arc<Sequence>>,
    /// What was asked before there was an engine, applied when the SoundFont arrives.
    pending: Pending,
    /// The sound so far (string models, chains, buses, ...), in the order it came: a new
    /// engine (the first SoundFont, or one that replaces it) is given all of it again.
    /// Knob changes are folded into what they change, so it does not grow while a knob
    /// is dragged.
    sounds: Vec<SoundMessage>,
}

/// Playback requests made while the SoundFont was still loading.
#[derive(Debug, Default)]
struct Pending {
    play: bool,
    seek: Option<u64>,
    tempo_scale: Option<f32>,
}

impl std::fmt::Debug for Synth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Synth")
            .field("sample_rate", &self.sample_rate)
            .field("loaded", &self.engine.is_some())
            .finish()
    }
}

#[wasm_bindgen]
impl Synth {
    /// A silent synthesizer; it makes sound once a SoundFont is loaded.
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> Result<Synth, JsValue> {
        let rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            // The browser's rate is sane; the clamp only guards nonsense.
            sample_rate.clamp(8_000.0, 192_000.0)
        } else {
            44_100.0
        };
        let rate = u32::try_from(rate as i64).unwrap_or(44_100);
        Ok(Synth {
            sample_rate: rate,
            engine: None,
            sequence: None,
            pending: Pending::default(),
            sounds: Vec::new(),
        })
    }

    /// Loads a SoundFont (SF2) and starts the engine. Replaces a previous one; the song
    /// stays loaded, and a play, seek or tempo asked while it loaded takes effect now.
    pub fn load_soundfont(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let bank = SoundBank::from_bytes(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut engine = SynthEngine::new(&bank, self.sample_rate)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        for sound in &self.sounds {
            engine.apply_sound(sound.clone());
        }
        if let Some(sequence) = &self.sequence {
            let _ = engine.load(Arc::clone(sequence));
        }
        let pending = std::mem::take(&mut self.pending);
        if let Some(scale) = pending.tempo_scale {
            engine.set_tempo_scale(scale);
        }
        if let Some(tick) = pending.seek {
            engine.seek(tick);
        }
        if pending.play {
            engine.play();
        }
        self.engine = Some(engine);
        Ok(())
    }

    /// Loads a song encoded by `gscore_play::encode_sequence`.
    pub fn load_sequence(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.swap_sequence(bytes, false)
    }

    /// Swaps in a rebuilt sequence (a live mixer change), keeping the position.
    pub fn replace_sequence(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.swap_sequence(bytes, true)
    }

    fn swap_sequence(&mut self, bytes: &[u8], replace: bool) -> Result<(), JsValue> {
        let sequence = Arc::new(decode_sequence(bytes).map_err(|e| JsValue::from_str(&e))?);
        match &mut self.engine {
            Some(engine) if replace => engine.replace(Arc::clone(&sequence)),
            Some(engine) => {
                let _ = engine.load(Arc::clone(&sequence));
            }
            // A new song before the engine: what was asked for the old one is void.
            None if !replace => self.pending = Pending::default(),
            None => {}
        }
        self.sequence = Some(sequence);
        Ok(())
    }

    /// Takes one change to the sound (`gscore_audio::encode_sound`): a string model, a
    /// track's chain, a bus, a drum part. Heard at once, and kept for a later SoundFont.
    pub fn sound(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let message = decode_sound(bytes).map_err(|e| JsValue::from_str(&e))?;
        if let Some(engine) = &mut self.engine {
            engine.apply_sound(message.clone());
        }
        remember(&mut self.sounds, message);
        Ok(())
    }

    /// Sends one MIDI message now: a live volume or pan change. `status` is the high nibble
    /// (0x9 note on, 0x8 note off, 0xC program, 0xB control, 0xE pitch bend).
    pub fn send_midi(&mut self, status: u8, data1: u8, data2: u8) -> Result<(), JsValue> {
        let Some(engine) = &mut self.engine else {
            return Ok(());
        };
        let channel = gscore_play::Channel::new(status & 0x0F)
            .ok_or_else(|| JsValue::from_str("bad MIDI channel"))?;
        let event = match status >> 4 {
            0x9 => gscore_play::MidiEvent::NoteOn {
                channel,
                key: data1 & 0x7F,
                velocity: data2 & 0x7F,
            },
            0x8 => gscore_play::MidiEvent::NoteOff {
                channel,
                key: data1 & 0x7F,
            },
            0xC => gscore_play::MidiEvent::ProgramChange {
                channel,
                program: data1 & 0x7F,
            },
            0xB => gscore_play::MidiEvent::Control {
                channel,
                controller: data1 & 0x7F,
                value: data2 & 0x7F,
            },
            0xE => gscore_play::MidiEvent::PitchBend {
                channel,
                value: u16::from(data1 & 0x7F) | (u16::from(data2 & 0x7F) << 7),
            },
            _ => return Err(JsValue::from_str("unsupported MIDI message")),
        };
        engine.send_event(event);
        Ok(())
    }

    /// Starts or resumes playback.
    pub fn play(&mut self) {
        match &mut self.engine {
            Some(engine) => engine.play(),
            None => self.pending.play = true,
        }
    }

    /// Pauses where it is.
    pub fn pause(&mut self) {
        match &mut self.engine {
            Some(engine) => engine.pause(),
            None => self.pending.play = false,
        }
    }

    /// Stops and returns to the start.
    pub fn stop(&mut self) {
        match &mut self.engine {
            Some(engine) => engine.stop(),
            None => self.pending = Pending::default(),
        }
    }

    /// Moves to `tick`.
    pub fn seek(&mut self, tick: f64) {
        let tick = if tick.is_finite() && tick > 0.0 {
            u64::try_from(tick as u128).unwrap_or(u64::MAX)
        } else {
            0
        };
        match &mut self.engine {
            Some(engine) => engine.seek(tick),
            None => self.pending.seek = Some(tick),
        }
    }

    /// Plays faster or slower than written.
    pub fn set_tempo_scale(&mut self, scale: f32) {
        match &mut self.engine {
            Some(engine) => engine.set_tempo_scale(scale),
            None => self.pending.tempo_scale = Some(scale),
        }
    }

    /// Renders `left.len()` frames; the JavaScript arrays are stereo.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Some(engine) = &mut self.engine else {
            left.fill(0.0);
            right.fill(0.0);
            return;
        };
        engine.render(left, right);
    }

    /// The position in ticks.
    pub fn position(&self) -> f64 {
        self.engine.as_ref().map_or(0, |engine| engine.position()) as f64
    }

    /// Playing (1), paused (2) or stopped (0).
    pub fn state(&self) -> u8 {
        match self.engine.as_ref().map(SynthEngine::state) {
            Some(PlayState::Playing) => 1,
            Some(PlayState::Paused) => 2,
            Some(PlayState::Stopped) | None => 0,
        }
    }

    /// The sequence as the worklet received it, for tests.
    pub fn round_trip(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        let sequence = decode_sequence(bytes).map_err(|e| JsValue::from_str(&e))?;
        Ok(encode_sequence(&sequence))
    }
}

/// Keeps `message` in `sounds` for a later engine: it takes the place of an earlier one
/// for the same thing (the same track's chain, the same bus), so the list holds one entry
/// per thing, not one per knob step.
fn remember(sounds: &mut Vec<SoundMessage>, message: SoundMessage) {
    use SoundMessage as M;
    // What a message is about: its kind and the track, part or bus it names.
    fn key(message: &SoundMessage) -> (u8, usize, usize) {
        match message {
            M::StringModel(kind, _) => (0, *kind as usize, 0),
            M::TrackModel(track, _) => (1, *track, 0),
            M::Chain(track, ..) => (2, *track, 0),
            M::ChainSettings(track, _) => (3, *track, 0),
            M::PartChain(track, part, ..) => (4, *track, *part),
            M::PartChainSettings(track, part, _) => (5, *track, *part),
            M::PartLevel(track, part, _) => (6, *track, *part),
            M::PartPan(track, part, _) => (7, *track, *part),
            M::PartMute(track, part, _) => (8, *track, *part),
            M::PartSolo(track, part, _) => (9, *track, *part),
            M::MasterChain(..) => (10, 0, 0),
            M::MasterChainSettings(_) => (11, 0, 0),
            M::Bus(bus, ..) => (12, *bus as usize, 0),
            M::BusSettings(bus, ..) => (13, *bus as usize, 0),
            M::CompSend(track, _) => (14, *track, 0),
            M::Trim(track, _) => (15, *track, 0),
            M::DrumRoom(track) => (16, *track, 0),
            M::PartRoom(track, part, _) => (17, *track, *part),
        }
    }
    let new = key(&message);
    // A new chain makes the knob values sent for the old one void (it brings its own).
    let voided = match new.0 {
        2 => Some(3),
        4 => Some(5),
        10 => Some(11),
        12 => Some(13),
        _ => None,
    };
    sounds.retain(|old| {
        let old = key(old);
        old != new && Some(old) != voided.map(|kind| (kind, new.1, new.2))
    });
    sounds.push(message);
}
