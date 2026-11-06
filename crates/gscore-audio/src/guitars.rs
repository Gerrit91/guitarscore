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
    Attack as StrikeKind, BLOCKS, BlockSettings, Chain, GuitarModel, Pluck, Release, StringModel,
    StringVoice, frequency,
};
use gscore_play::{Attack, Fade, Performance, PlayedNote, Sequence, StringKind, TrackEngine};

use crate::chains::ChainSet;
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
    kind: StringKind,
    /// Which instance of its kind's chain it runs through (the n-th track of that kind).
    instance: usize,
}

/// The modelled guitars of one synth engine.
pub(crate) struct Guitars {
    voices: Vec<StringVoice>,
    /// The note each voice plays now, if any.
    playing: Vec<Option<PlayedNote>>,
    slots: [Option<Slot>; SLOTS],
    /// How each kind of string sounds (nylon, steel, electric, bass).
    models: [StringModel; 4],
    /// The effect chain of each kind, if it has one, and its knob values.
    chains: [Option<Box<ChainSet>>; 4],
    chain_settings: [[BlockSettings; BLOCKS]; 4],
    /// The reverb bus every modelled track sends into (a computed reverb, all wet), its
    /// knob values, and its level (-127 to 127; 127 plays it as it comes) and mute.
    reverb_bus: Option<Box<Chain>>,
    bus_mix: (u8, bool),
    /// The bus's input (mono: the sends summed) and its output of one block.
    bus_in: [f32; BLOCK],
    bus_out: [[f32; BLOCK]; 2],
    /// The sequence the slots were set up for (by address; the transport owns it).
    sequence: usize,
    /// The next entry of the performance's `by_start` to start.
    cursor: usize,
    /// Whether the cursor must be found again (after a stop, pause, seek or load).
    resync: bool,
    mono: [f32; BLOCK],
    /// A track's output after its chain.
    wet: [[f32; BLOCK]; 2],
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

/// The gain of a General MIDI channel volume (0-127): a square law, as synthesizers do.
fn volume_gain(volume: u8) -> f32 {
    let share = f32::from(volume.min(127)) / 127.0;
    share * share
}

/// Left and right gains for a pan (0 left, 64 middle, 127 right), equal power, 1 in the
/// middle.
fn pan_gains(pan: u8) -> (f32, f32) {
    let angle = f32::from(pan.min(127)) / 127.0 * std::f32::consts::FRAC_PI_2;
    let boost = std::f32::consts::SQRT_2;
    (angle.cos() * boost, angle.sin() * boost)
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
            mono: [0.0; BLOCK],
            wet: [[0.0; BLOCK]; 2],
            chains: [None, None, None, None],
            chain_settings: [[BlockSettings::default(); BLOCKS]; 4],
            reverb_bus: None,
            bus_mix: (127, false),
            bus_in: [0.0; BLOCK],
            bus_out: [[0.0; BLOCK]; 2],
        }
    }

    /// Whether the modelled strings play `track` (an index of `sequence`'s tracks): then
    /// its MIDI notes must not reach the SoundFont.
    pub(crate) fn plays(&self, track: usize) -> bool {
        self.slots.iter().flatten().any(|slot| slot.track == track)
    }

    /// Makes the strings of `kind` sound as `model`, at once for those sounding.
    pub(crate) fn set_model(&mut self, kind: StringKind, model: StringModel) {
        if let Some(slot) = self.models.get_mut(kind_index(kind)) {
            *slot = model;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.is_some_and(|slot| slot.kind == kind) {
                let first = index.saturating_mul(STRINGS);
                for voice in self
                    .voices
                    .get_mut(first..first.saturating_add(STRINGS))
                    .unwrap_or_default()
                {
                    voice.set_model(model);
                }
            }
        }
    }

    /// Gives `kind` the effect chain `chain` (`None`: none), with the knob values it has.
    /// Returns the chain it replaces, to be freed off the audio thread.
    pub(crate) fn set_chain(
        &mut self,
        kind: StringKind,
        chain: Option<Box<ChainSet>>,
    ) -> Option<Box<ChainSet>> {
        let index = kind_index(kind);
        let mut chain = chain;
        if let (Some(chain), Some(settings)) = (&mut chain, self.chain_settings.get(index)) {
            for instance in &mut chain.instances {
                instance.set_settings(settings);
            }
        }
        self.chains
            .get_mut(index)
            .and_then(|slot| std::mem::replace(slot, chain))
    }

    /// New knob values for the chain of `kind`, heard at once.
    pub(crate) fn set_chain_settings(
        &mut self,
        kind: StringKind,
        settings: [BlockSettings; BLOCKS],
    ) {
        let index = kind_index(kind);
        if let Some(slot) = self.chain_settings.get_mut(index) {
            *slot = settings;
        }
        if let Some(Some(chain)) = self.chains.get_mut(index) {
            for instance in &mut chain.instances {
                instance.set_settings(&settings);
            }
        }
    }

    /// Gives the reverb bus `chain` (`None`: none) with its knob values. Returns the chain
    /// it replaces, to be freed off the audio thread.
    pub(crate) fn set_reverb_bus(
        &mut self,
        chain: Option<Box<Chain>>,
        settings: [BlockSettings; BLOCKS],
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        if let Some(chain) = &mut chain {
            chain.set_settings(&settings);
        }
        std::mem::replace(&mut self.reverb_bus, chain)
    }

    /// New knob values, level and mute for the reverb bus, heard at once.
    pub(crate) fn set_reverb_bus_settings(
        &mut self,
        settings: [BlockSettings; BLOCKS],
        level: u8,
        muted: bool,
    ) {
        self.bus_mix = (level.min(127), muted);
        if let Some(bus) = &mut self.reverb_bus {
            bus.set_settings(&settings);
        }
    }

    /// Stops every string quickly (stop, pause, seek, a new sequence).
    pub(crate) fn silence(&mut self) {
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
                        kind,
                        instance: 0,
                    }),
                    TrackEngine::SoundFont => None,
                });
        let mut counts = [0_usize; 4];
        for (place, mut slot) in slots.iter_mut().zip(modelled) {
            if let Some(count) = counts.get_mut(kind_index(slot.kind)) {
                slot.instance = *count;
                *count = count.saturating_add(1);
            }
            *place = Some(slot);
        }
        for (index, (old, new)) in self.slots.iter().zip(&slots).enumerate() {
            if old == new {
                continue;
            }
            let first = index.saturating_mul(STRINGS);
            for string in first..first.saturating_add(STRINGS) {
                if let Some(voice) = self.voices.get_mut(string) {
                    voice.silence();
                    if let Some(slot) = new
                        && let Some(&model) = self.models.get(kind_index(slot.kind))
                    {
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
    /// fades, and adds the strings' next `left.len()` samples (at most [`BLOCK`]) to the
    /// output.
    pub(crate) fn render(
        &mut self,
        sequence: Option<&Arc<Sequence>>,
        tick: u64,
        playing: bool,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        if let Some(sequence) = sequence {
            let performance = &sequence.performance;
            if playing {
                self.advance(performance, tick);
            }
            self.mix(Some(performance), left, right);
        } else {
            self.mix(None, left, right);
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

    fn mix(&mut self, performance: Option<&Performance>, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len()).min(BLOCK);
        if let Some(input) = self.bus_in.get_mut(..frames) {
            input.fill(0.0);
        }
        for (slot_index, slot) in self.slots.iter().enumerate() {
            let Some(slot) = slot else {
                continue;
            };
            let first = slot_index.saturating_mul(STRINGS);
            let voices = self
                .voices
                .get_mut(first..first.saturating_add(STRINGS))
                .unwrap_or_default();
            // The track's chain instance; a track past the instances made plays dry.
            let chain = self
                .chains
                .get_mut(kind_index(slot.kind))
                .and_then(Option::as_mut)
                .and_then(|chain| chain.instances.get_mut(slot.instance));
            let sounding = voices.iter().any(StringVoice::is_sounding);
            if !sounding && !chain.as_ref().is_some_and(|chain| chain.ringing()) {
                continue;
            }
            let mono = self.mono.get_mut(..frames).unwrap_or_default();
            mono.fill(0.0);
            for voice in voices {
                voice.render(mono);
            }
            let [wet_left, wet_right] = &mut self.wet;
            let (wet_left, wet_right) = (
                wet_left.get_mut(..frames).unwrap_or_default(),
                wet_right.get_mut(..frames).unwrap_or_default(),
            );
            match chain {
                Some(chain) => chain.process(mono, wet_left, wet_right),
                None => {
                    wet_left.copy_from_slice(mono);
                    wet_right.copy_from_slice(mono);
                }
            }
            let sound = performance
                .and_then(|performance| performance.tracks.get(slot.track))
                .map(|track| track.sound);
            let (volume, pan) = sound.map_or((100, 64), |sound| (sound.volume, sound.pan));
            let gain = volume_gain(volume);
            // The pan works as a balance once the chain has made the track stereo.
            let (to_left, to_right) = pan_gains(pan);
            // The track's reverb send (the mixer's Reverb knob) goes into the bus after
            // its fader and pan, as a mixing desk sends; a mono sum is enough for the
            // room, which makes it stereo again.
            let send = if self.reverb_bus.is_some() {
                f32::from(sound.map_or(0, |sound| sound.reverb).min(127)) / 127.0
            } else {
                0.0
            };
            let bus_in = self.bus_in.get_mut(..frames).unwrap_or_default();
            for ((((l, r), wl), wr), bus) in left
                .iter_mut()
                .zip(right.iter_mut())
                .zip(wet_left.iter())
                .zip(wet_right.iter())
                .zip(bus_in.iter_mut())
            {
                let (l_dry, r_dry) = (wl * gain * to_left, wr * gain * to_right);
                *l += l_dry;
                *r += r_dry;
                if send > 0.0 {
                    *bus += 0.5 * (l_dry + r_dry) * send;
                }
            }
        }
        self.mix_reverb_bus(left, right, frames);
    }

    /// Runs the reverb bus (which rings on after the sends stop) and adds it to the mix.
    fn mix_reverb_bus(&mut self, left: &mut [f32], right: &mut [f32], frames: usize) {
        let Some(bus) = &mut self.reverb_bus else {
            return;
        };
        let (level, muted) = self.bus_mix;
        let input = self.bus_in.get(..frames).unwrap_or_default();
        let sent = input.iter().any(|sample| sample.abs() > 1e-9);
        // Silent and its tail over: nothing to do (and it may rest).
        if !sent && !bus.ringing() {
            return;
        }
        let [out_left, out_right] = &mut self.bus_out;
        let (out_left, out_right) = (
            out_left.get_mut(..frames).unwrap_or_default(),
            out_right.get_mut(..frames).unwrap_or_default(),
        );
        bus.process(input, out_left, out_right);
        if muted {
            return;
        }
        let gain = volume_gain(level);
        for (((l, r), bl), br) in left
            .iter_mut()
            .zip(right.iter_mut())
            .zip(out_left.iter())
            .zip(out_right.iter())
        {
            *l += bl * gain;
            *r += br * gain;
        }
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
