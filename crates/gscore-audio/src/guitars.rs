//! The modelled guitars inside the synth engine (M22): the tracks whose
//! [`TrackEngine`] is `Modelled` are played string by string from the sequence's
//! [`Performance`] by `gscore-guitar`'s string voices, mixed with the SoundFont's output.
//! The SoundFont still plays every other track (drums, keys, vocals), and the MIDI notes of
//! a modelled track are not sent to it.
//!
//! The voices are made once, with the engine: [`SLOTS`] tracks of [`STRINGS`] strings,
//! each long enough for the lowest bass note bent down. Nothing here allocates or waits,
//! so it runs in the audio callback; notes start at the block in which they are due, like
//! the SoundFont's MIDI events.

use std::sync::Arc;

use gscore_guitar::{
    Attack as StrikeKind, Body, GuitarModel, Pluck, Release, StringModel, StringVoice, frequency,
};
use gscore_play::{Attack, Fade, Performance, PlayedNote, Sequence, StringKind, TrackEngine};

use crate::chains::MAX_TONES;
use crate::synth::BLOCK;

/// How many tracks the modelled strings play at once; further modelled tracks stay on
/// the SoundFont.
pub(crate) const SLOTS: usize = 16;
/// Strings per track (higher strings share voices, which a guitar never needs).
pub(crate) const STRINGS: usize = 8;
/// The lowest pitch a voice is made for, in Hz: below a five-string bass's low B (31 Hz)
/// with room to bend.
const LOWEST: f32 = 16.0;

/// One track's place in the modelled strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    /// The track's index in the sequence (and its performance).
    track: usize,
    /// The song's track it plays (an excerpt's only track is still the song's track it
    /// came from): its tone is that track's.
    tone: usize,
    kind: StringKind,
}

/// The modelled guitars of one synth engine.
pub(crate) struct Guitars {
    voices: Vec<StringVoice>,
    /// The note each voice plays now, if any.
    playing: Vec<Option<PlayedNote>>,
    slots: [Option<Slot>; SLOTS],
    /// How each kind of string sounds (nylon, steel, electric, bass) on a track without a
    /// string model of its own.
    models: [StringModel; 4],
    /// Each track's own string model, by the song's track index (`None`: its kind's).
    track_models: Vec<Option<StringModel>>,
    /// The sequence the slots were set up for (by address; the transport owns it).
    sequence: usize,
    /// The next entry of the performance's `by_start` to start.
    cursor: usize,
    /// Whether the cursor must be found again (after a stop, pause, seek or load).
    resync: bool,
    /// Each slot's strings this block (mono), for the mixer.
    outputs: [[f32; BLOCK]; SLOTS],
    /// Each slot's acoustic body (none for an electric or a bass): the strings' sum runs
    /// through it, as commuted synthesis would through each string.
    bodies: [Body; SLOTS],
}

impl std::fmt::Debug for Guitars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guitars")
            .field("slots", &self.slots)
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}

/// The place of `kind` in [`Guitars::models`].
fn kind_index(kind: StringKind) -> usize {
    match kind {
        StringKind::Nylon => 0,
        StringKind::Steel => 1,
        StringKind::Electric => 2,
        StringKind::Bass => 3,
    }
}

/// The preset of `kind`.
pub(crate) fn default_model(kind: StringKind) -> StringModel {
    model_of(kind).strings()
}

fn model_of(kind: StringKind) -> GuitarModel {
    match kind {
        StringKind::Nylon => GuitarModel::Nylon,
        StringKind::Steel => GuitarModel::Steel,
        StringKind::Electric => GuitarModel::Electric,
        StringKind::Bass => GuitarModel::Bass,
    }
}

fn strike_of(attack: Attack) -> StrikeKind {
    match attack {
        Attack::Picked => StrikeKind::Pick,
        Attack::Legato => StrikeKind::Legato,
        Attack::LeftHandTap | Attack::Tapped => StrikeKind::Tap,
        Attack::Slapped => StrikeKind::Slap,
        Attack::Popped => StrikeKind::Pop,
        Attack::Scrape => StrikeKind::Scrape,
    }
}

/// How far a note has played, 0 to 1, at `tick`.
fn progress(note: &PlayedNote, tick: u64) -> f32 {
    let length = note.end.saturating_sub(note.start).max(1);
    (tick.saturating_sub(note.start) as f32 / length as f32).clamp(0.0, 1.0)
}

impl Guitars {
    /// The voices for an engine at `sample_rate`.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let model = GuitarModel::Electric.strings();
        let count = SLOTS.saturating_mul(STRINGS);
        Guitars {
            voices: (0..count)
                .map(|_| StringVoice::new(rate, LOWEST, model))
                .collect(),
            playing: vec![None; count],
            slots: [None; SLOTS],
            models: [
                StringKind::Nylon,
                StringKind::Steel,
                StringKind::Electric,
                StringKind::Bass,
            ]
            .map(default_model),
            sequence: 0,
            cursor: 0,
            resync: true,
            outputs: [[0.0; BLOCK]; SLOTS],
            track_models: vec![None; MAX_TONES],
            bodies: std::array::from_fn(|_| Body::new(rate)),
        }
    }

    /// Whether the modelled strings play `track` (an index of `sequence`'s tracks): then
    /// its MIDI notes must not reach the SoundFont.
    pub(crate) fn plays(&self, track: usize) -> bool {
        self.slots.iter().flatten().any(|slot| slot.track == track)
    }

    /// The string model `slot`'s track plays with: its own, or its kind's.
    fn model_for(&self, slot: Slot) -> StringModel {
        self.track_models
            .get(slot.tone)
            .copied()
            .flatten()
            .unwrap_or_else(|| {
                self.models
                    .get(kind_index(slot.kind))
                    .copied()
                    .unwrap_or_else(|| default_model(slot.kind))
            })
    }

    /// Gives the voices of every slot that `matches` the model its track plays with now.
    fn refresh_models(&mut self, matches: impl Fn(Slot) -> bool) {
        for index in 0..SLOTS {
            let Some(slot) = self.slots.get(index).copied().flatten() else {
                continue;
            };
            if !matches(slot) {
                continue;
            }
            let model = self.model_for(slot);
            let first = index.saturating_mul(STRINGS);
            for voice in self
                .voices
                .get_mut(first..first.saturating_add(STRINGS))
                .unwrap_or_default()
            {
                voice.set_model(model);
            }
            if let Some(body) = self.bodies.get_mut(index) {
                body.set(model.body, model.body_mix, model.body_size);
            }
        }
    }

    /// Makes the strings of `kind` sound as `model` on the tracks without a model of their
    /// own, at once for those sounding.
    pub(crate) fn set_model(&mut self, kind: StringKind, model: StringModel) {
        if let Some(slot) = self.models.get_mut(kind_index(kind)) {
            *slot = model;
        }
        self.refresh_models(|slot| slot.kind == kind);
    }

    /// Gives `track` its own string model (`None`: its kind's again), at once.
    pub(crate) fn set_track_model(&mut self, track: usize, model: Option<StringModel>) {
        if let Some(slot) = self.track_models.get_mut(track) {
            *slot = model;
        }
        self.refresh_models(|slot| slot.tone == track);
    }

    /// Stops every string quickly (stop, pause, seek, a new sequence).
    pub(crate) fn silence(&mut self) {
        // The bodies ring out with the strings' quick fade (no reset: no click).
        for voice in &mut self.voices {
            voice.silence();
        }
        self.playing.fill(None);
        self.resync = true;
        // Set the slots up again on the next block: a new sequence may live where a freed
        // one did.
        self.sequence = 0;
    }

    /// Sets the slots up for `sequence` if it is not the one they are set up for: each
    /// modelled track takes the next slot. A slot that keeps its track goes on sounding
    /// (a rebuilt sequence after a mixer change).
    pub(crate) fn follow(&mut self, sequence: &Arc<Sequence>) {
        let address = Arc::as_ptr(sequence) as usize;
        if address == self.sequence {
            return;
        }
        self.sequence = address;
        self.resync = true;
        let mut slots = [None; SLOTS];
        let modelled =
            sequence
                .performance
                .tracks
                .iter()
                .enumerate()
                .filter_map(|(track, entry)| match entry.engine {
                    TrackEngine::Modelled(kind) => Some(Slot {
                        track,
                        tone: entry.track.get(),
                        kind,
                    }),
                    // A hardware synthesizer plays a MIDI-out track; the strings do not.
                    TrackEngine::SoundFont | TrackEngine::MidiOut => None,
                });
        for (place, slot) in slots.iter_mut().zip(modelled) {
            *place = Some(slot);
        }
        for (index, (old, new)) in self.slots.iter().zip(&slots).enumerate() {
            if old == new {
                continue;
            }
            let model = new.map(|slot| self.model_for(slot));
            if let Some(body) = self.bodies.get_mut(index) {
                body.reset();
                if let Some(model) = model {
                    body.set(model.body, model.body_mix, model.body_size);
                }
            }
            let first = index.saturating_mul(STRINGS);
            for string in first..first.saturating_add(STRINGS) {
                if let Some(voice) = self.voices.get_mut(string) {
                    voice.silence();
                    if let Some(model) = model {
                        voice.set_model(model);
                    }
                }
                if let Some(note) = self.playing.get_mut(string) {
                    *note = None;
                }
            }
        }
        self.slots = slots;
    }

    /// Starts and ends the notes due at `tick` (when `playing`), moves the pitches and
    /// fades, and renders `frames` (at most [`BLOCK`]) of each slot's strings, mono, for
    /// the mixer ([`crate::mixer::Strings`]).
    pub(crate) fn render(
        &mut self,
        sequence: Option<&Arc<Sequence>>,
        tick: u64,
        playing: bool,
        frames: usize,
    ) {
        if let Some(sequence) = sequence
            && playing
        {
            self.advance(&sequence.performance, tick);
        }
        let frames = frames.min(BLOCK);
        for (index, output) in self.outputs.iter_mut().enumerate() {
            let output = output.get_mut(..frames).unwrap_or_default();
            output.fill(0.0);
            if self.slots.get(index).copied().flatten().is_none() {
                continue;
            }
            let first = index.saturating_mul(STRINGS);
            for voice in self
                .voices
                .get_mut(first..first.saturating_add(STRINGS))
                .unwrap_or_default()
            {
                voice.render(output);
            }
            if let Some(body) = self.bodies.get_mut(index) {
                body.process(output);
            }
        }
    }

    fn voice_index(&self, note: &PlayedNote) -> Option<usize> {
        let slot = self
            .slots
            .iter()
            .position(|slot| slot.is_some_and(|slot| slot.track == note.track))?;
        let string = usize::from(note.string.max(1).saturating_sub(1)) % STRINGS;
        Some(slot.saturating_mul(STRINGS).saturating_add(string))
    }

    fn advance(&mut self, performance: &Performance, tick: u64) {
        if self.resync {
            self.cursor = performance.by_start.partition_point(|&index| {
                performance
                    .notes
                    .get(index)
                    .is_some_and(|note| note.start < tick)
            });
            self.resync = false;
        }
        // Notes that are over end first, so the next note on their string follows.
        for (voice, slot) in self.voices.iter_mut().zip(self.playing.iter_mut()) {
            if let Some(note) = slot
                && tick >= note.end
            {
                voice.release(if note.damped {
                    Release::Damped
                } else {
                    Release::Natural
                });
                *slot = None;
            }
        }
        while let Some(note) = performance
            .by_start
            .get(self.cursor)
            .and_then(|&index| performance.notes.get(index))
            .copied()
        {
            if note.start > tick {
                break;
            }
            self.cursor = self.cursor.saturating_add(1);
            let Some(index) = self.voice_index(&note) else {
                continue;
            };
            if let Some(voice) = self.voices.get_mut(index) {
                voice.set_level(fade_level(&note, tick));
                voice.pluck(Pluck {
                    frequency: frequency(note.key, 0.0),
                    velocity: f32::from(note.velocity.min(127)) / 127.0,
                    attack: strike_of(note.attack),
                    palm_mute: note.palm_mute,
                    dead: note.dead,
                    harmonic: note.harmonic.is_some(),
                });
            }
            if let Some(slot) = self.playing.get_mut(index) {
                *slot = Some(note);
            }
        }
        // Bends, slides, vibrato and whammy follow the note's pitch shape; fades its level.
        for (voice, note) in self.voices.iter_mut().zip(&self.playing) {
            let Some(note) = note else {
                continue;
            };
            if let Some(shape) = note.pitch {
                voice.set_bend(shape.cents_at(tick.saturating_sub(note.start)) as f32);
            }
            if note.fade.is_some() {
                voice.set_level(fade_level(note, tick));
            }
        }
    }
}

impl crate::mixer::Strings for Guitars {
    fn slot_of(&self, position: usize) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.is_some_and(|slot| slot.track == position))
    }

    fn output(&self, slot: usize) -> &[f32] {
        self.outputs.get(slot).map_or(&[], |output| &output[..])
    }
}

/// The level a note's fade has reached at `tick` (1 without a fade).
fn fade_level(note: &PlayedNote, tick: u64) -> f32 {
    let share = progress(note, tick);
    match note.fade {
        None => 1.0,
        Some(Fade::In) => share,
        Some(Fade::Out) => 1.0 - share,
        Some(Fade::Swell) => 1.0 - (2.0 * share - 1.0).abs(),
    }
}
