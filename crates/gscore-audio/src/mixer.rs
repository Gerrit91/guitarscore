//! The mixer of the synth engine: every track of the sequence is a strip, as on a mixing
//! desk.
//!
//! ```text
//!  SoundFont channels ─┐                                ┌──────────────────────► master chain ─► master ─► out
//!  (or the strings)    ├─► chain ─► fader ─► pan ──────┼─► reverb send ─► reverb bus ─┤
//!                      │                                ├─► comp send ───► comp bus ───┤
//!                      │                                └─► chorus send ─► chorus bus ─┘
//! ```
//!
//! - **Buses** ([`Bus`]): the reverb bus (a reverb or a room, all wet; it hears the sends
//!   in mono and makes them stereo), the compressor bus (a chain, usually a compressor
//!   driven hard: parallel or "New York" compression, blended in under the dry tracks) and
//!   the chorus bus (the tracks' Chorus values, all wet; it hears the sends in mono and
//!   makes them stereo).
//!
//! - **Sources:** a SoundFont track gets the MIDI channels it plays on, each rendered apart
//!   by our fork of RustySynth (`vendor/rustysynth`): its own channel and those it borrows
//!   for bends. A track played by modelled strings gets its strings (mono).
//! - **Chain** first, at the source's full level, so a fader does not change how hard an
//!   amp is driven; then the **fader and pan**, then the post-fader **reverb send**.
//! - The song's **volume, pan and reverb send** (MIDI CC 7, 10 and 91, from its setup, its
//!   automation and the mixer's live changes) move the strip: the synthesizer itself stays
//!   at full volume and centred, so the levels are the same as when it applied them (the
//!   same square law, and our centre pan gives back the synthesizer's).
//! - **Tracks sharing a MIDI channel** (Guitar Pro allows it) are one signal: the first
//!   track with that channel owns it, the others are linked to it (the interface shows
//!   them together); their notes still come and go with their own mute and solo.
//! - **A drum track** stays one strip: its channel's sound is split into the kit's parts
//!   by the fork (kick, snare, hi-hat, toms, cymbals, the rest), each part through its own
//!   chain, then the parts summed and through the track's own chain (the drum bus).
//! - **Meters** measure each strip after its fader, the reverb bus and the master
//!   ([`crate::Meters`]).
//!
//! Nothing here allocates or waits: it runs in the audio callback.

use std::sync::Arc;

use gscore_guitar::Chain;
use gscore_play::{MidiEvent, Sequence, controller};
use rustysynth::PARTS;

use crate::chains::{ChainKnobs, MAX_TONES, TrackTones};
use crate::meters::{Ballistics, Meters, Needle, Peak, Which, peak_of};
use crate::synth::BLOCK;

/// MIDI channels of the synthesizer.
pub(crate) const CHANNELS: usize = 16;
/// The tracks of a sequence that get a strip (a song has at most a few dozen).
pub(crate) const MAX_STRIPS: usize = 64;

/// The gain of a General MIDI channel volume (0-127): a square law, as synthesizers do.
pub(crate) fn volume_gain(volume: u8) -> f32 {
    let share = f32::from(volume.min(127)) / 127.0;
    share * share
}

/// How much louder the reverb bus returns than its reverb plays, in dB. The reverb block is
/// voiced 4-6 dB under the dry signal when all wet (right for an insert), so on the bus a
/// full send came back well under the track; with this a 100 % send returns at about the
/// dry track's loudness (a reverb's peaks still read lower: it spreads a hit over its tail).
pub(crate) const REVERB_RETURN_DB: f32 = 6.0;

/// Left and right gains for a pan (0 left, 64 middle, 127 right), equal power, 1 in the
/// middle.
pub(crate) fn pan_gains(pan: u8) -> (f32, f32) {
    let angle = f32::from(pan.min(127)) / 127.0 * std::f32::consts::FRAC_PI_2;
    let boost = std::f32::consts::SQRT_2;
    (angle.cos() * boost, angle.sin() * boost)
}

/// How a drum part's stereo image moves for `pan` (0 left, 64 centre, 127 right): the
/// weights `(left from left, left from right, right from right, right from left)`. Towards
/// a side the far channel fades out and folds into the near one (scaled by about 0.7, so a
/// hit in the middle stays about as loud), so the whole hit moves instead of half of a
/// stereo sample being lost as a balance would.
fn part_pan(pan: u8) -> (f32, f32, f32, f32) {
    const FOLD: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let p = ((f32::from(pan.min(127)) - 64.0) / 63.0).clamp(-1.0, 1.0);
    if p < 0.0 {
        (1.0, -p * FOLD, 1.0 + p, 0.0)
    } else {
        (1.0 - p, 0.0, 1.0, p * FOLD)
    }
}

/// One track's strip.
#[derive(Debug, Clone, Copy)]
struct Strip {
    /// The song's track it plays (its chain and meter are that track's).
    song_track: usize,
    /// The MIDI channel its sound comes on (a drum track's parts come from it).
    channel: usize,
    /// Whether it is a drum track, its sound split into the kit's parts.
    drums: bool,
    volume: u8,
    pan: u8,
    reverb: u8,
    chorus: u8,
    peak: Peak,
    /// The peak before the chain (after the trim), for the input meter.
    input_peak: Peak,
    /// The peak after the chain (and its output level), for the chain's output meter.
    output_peak: Peak,
}

impl Default for Strip {
    fn default() -> Self {
        Strip {
            song_track: usize::MAX,
            channel: 0,
            drums: false,
            volume: 100,
            pan: 64,
            reverb: 0,
            chorus: 0,
            peak: Peak::default(),
            input_peak: Peak::default(),
            output_peak: Peak::default(),
        }
    }
}

/// The mixer's buses: every track sends into them after its fader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    /// The reverb bus: the tracks' Reverb knob (MIDI CC 91) sends into it.
    Reverb,
    /// The compressor bus: each track's Comp send feeds it.
    Compressor,
    /// The chorus bus: the tracks' Chorus knob (MIDI CC 93) sends into it.
    Chorus,
}

impl Bus {
    /// Every bus, in the order the mixer runs them.
    pub(crate) const ALL: [Bus; 3] = [Bus::Reverb, Bus::Compressor, Bus::Chorus];

    fn index(self) -> usize {
        match self {
            Bus::Reverb => 0,
            Bus::Compressor => 1,
            Bus::Chorus => 2,
        }
    }
}

/// One bus: its chain, level (0-127) and mute, what was sent into it this block and what
/// comes out, and its meters.
struct BusState {
    chain: Option<Box<Chain>>,
    mix: (u8, bool),
    input: [[f32; BLOCK]; 2],
    output: [[f32; BLOCK]; 2],
    peak: Peak,
    needle: Needle,
}

impl BusState {
    fn new() -> Self {
        BusState {
            chain: None,
            mix: (127, false),
            input: [[0.0; BLOCK]; 2],
            output: [[0.0; BLOCK]; 2],
            peak: Peak::default(),
            needle: Needle::default(),
        }
    }
}

/// What the strings give the mixer: each slot's mono output, and which strip it is.
pub(crate) trait Strings {
    /// The slot whose strings play the sequence's track `position`, if any.
    fn slot_of(&self, position: usize) -> Option<usize>;
    /// The strings' output of `slot` this block.
    fn output(&self, slot: usize) -> &[f32];
}

/// What the fork gives the mixer: each drum channel's parts of the block just rendered.
pub(crate) trait DrumParts {
    /// One part of `channel`'s last block (`side` 0 left, 1 right).
    fn part(&self, channel: usize, part: usize, side: usize) -> &[f32];
}

impl DrumParts for rustysynth::Synthesizer {
    fn part(&self, channel: usize, part: usize, side: usize) -> &[f32] {
        self.part_block(channel, part, side)
    }
}

/// The mixer of one engine.
pub(crate) struct Mixer {
    /// The synthesizer's output, channel by channel, left and right.
    channels: [[[f32; BLOCK]; 2]; CHANNELS],
    /// Which strip each channel's sound belongs to.
    channel_strip: [Option<usize>; CHANNELS],
    /// Whether a channel is some strip's own (the others are borrowed for bends).
    home: [bool; CHANNELS],
    strips: [Strip; MAX_STRIPS],
    count: usize,
    /// The sequence the strips were set up for (by address; the transport owns it).
    sequence: usize,
    /// Each track's chain.
    tones: TrackTones,
    /// Each drum track's part chains ([`PARTS`] of them), by the song's track index (as
    /// the chains: a strip's place in the sequence changes with mute, solo and excerpts).
    part_chains: [[Option<Box<Chain>>; PARTS]; MAX_TONES],
    /// Each drum track's part levels (0-127, 127 unity), by the song's track index.
    part_levels: [[u8; PARTS]; MAX_TONES],
    /// Each drum track's part pans (0 left, 64 centre, 127 right), by the song's track
    /// index: a balance, as the parts are stereo.
    part_pans: [[u8; PARTS]; MAX_TONES],
    /// Each drum track's part mutes and solos, by the song's track index.
    part_mutes: [[bool; PARTS]; MAX_TONES],
    part_solos: [[bool; PARTS]; MAX_TONES],
    /// Each drum track's part meters, by the song's track index and part.
    part_peaks: [[Peak; PARTS]; MAX_TONES],
    /// The buses ([`Bus::index`]).
    buses: [BusState; 3],
    /// Each track's send into the compressor bus (0-127), by the song's track index.
    comp_sends: [u8; MAX_TONES],
    /// Each track's input trim before its chain, in dB, by the song's track index.
    trim_db: [f32; MAX_TONES],
    /// Each drum track's room (one short shared room for its parts), by the song's track
    /// index.
    rooms: [Option<Box<Chain>>; MAX_TONES],
    /// What the parts sent into each drum track's room this block, by the song's index.
    room_input: [[[f32; BLOCK]; 2]; MAX_TONES],
    /// Each drum part's send into its kit's room (0-127), by the song's track index.
    part_rooms: [[u8; PARTS]; MAX_TONES],
    /// Each drum kit's own send into its room (0-127), by the song's track index.
    room_sends: [u8; MAX_TONES],
    /// Each track's gain-reduction needle, by the song's track index.
    needles: [Needle; MAX_TONES],
    /// Whether the strips' own signal goes to the output. A stem render (a track stem or a
    /// bus stem) turns it off, so only what the caller wants is written.
    dry: bool,
    /// The master's chain, after the tracks and buses and before its fader.
    master_chain: Option<Box<Chain>>,
    master_peak: Peak,
    /// A strip's signal on its way through, and a drum strip's parts summed.
    work: [[f32; BLOCK]; 2],
    parts_work: [[f32; BLOCK]; 2],
    meters: Arc<Meters>,
    ballistics: Ballistics,
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mixer")
            .field("strips", &self.count)
            .field("tones", &self.tones)
            .finish_non_exhaustive()
    }
}

impl Mixer {
    /// Tells every chain (tracks, drum parts, buses, master) the tempo played now, quarter
    /// notes a minute with the playback speed: a synced delay follows it. No allocation.
    pub(crate) fn set_tempo(&mut self, bpm: f32) {
        self.tones.set_tempo(bpm);
        for chain in self.part_chains.iter_mut().flatten().flatten() {
            chain.set_tempo(bpm);
        }
        for bus in &mut self.buses {
            if let Some(chain) = &mut bus.chain {
                chain.set_tempo(bpm);
            }
        }
        if let Some(chain) = &mut self.master_chain {
            chain.set_tempo(bpm);
        }
    }

    /// A mixer for an engine at `sample_rate`. Allocates.
    pub(crate) fn new(sample_rate: u32) -> Self {
        Mixer {
            channels: [[[0.0; BLOCK]; 2]; CHANNELS],
            channel_strip: [None; CHANNELS],
            home: [false; CHANNELS],
            strips: [Strip::default(); MAX_STRIPS],
            count: 0,
            sequence: 0,
            tones: TrackTones::new(),
            part_chains: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            part_levels: [[127; PARTS]; MAX_TONES],
            part_pans: [[64; PARTS]; MAX_TONES],
            part_mutes: [[false; PARTS]; MAX_TONES],
            part_solos: [[false; PARTS]; MAX_TONES],
            part_peaks: [[Peak::default(); PARTS]; MAX_TONES],
            buses: [BusState::new(), BusState::new(), BusState::new()],
            comp_sends: [0; MAX_TONES],
            trim_db: [0.0; MAX_TONES],
            rooms: std::array::from_fn(|_| None),
            room_input: [[[0.0; BLOCK]; 2]; MAX_TONES],
            part_rooms: [[0; PARTS]; MAX_TONES],
            room_sends: [0; MAX_TONES],
            needles: [Needle::default(); MAX_TONES],
            dry: true,
            master_chain: None,
            master_peak: Peak::default(),
            work: [[0.0; BLOCK]; 2],
            parts_work: [[0.0; BLOCK]; 2],
            meters: Arc::new(Meters::default()),
            ballistics: Ballistics::new(sample_rate, BLOCK),
        }
    }

    /// The meters, to be read elsewhere.
    pub(crate) fn meters(&self) -> Arc<Meters> {
        Arc::clone(&self.meters)
    }

    /// The synthesizer's output buffers, channel by channel, to render into.
    pub(crate) fn channel_buffers(&mut self) -> &mut [[[f32; BLOCK]; 2]; CHANNELS] {
        &mut self.channels
    }

    /// Sets the strips up for `sequence` if they are not yet: one per track, each owning its
    /// channel (the first track of a channel owns it; a later one is linked). A strip that
    /// keeps its track keeps its fader, pan and send (a sequence rebuilt while playing).
    pub(crate) fn follow(&mut self, sequence: &Arc<Sequence>) {
        let address = Arc::as_ptr(sequence) as usize;
        if address == self.sequence {
            return;
        }
        self.sequence = address;
        let old = self.strips;
        let old_count = self.count;
        self.channel_strip = [None; CHANNELS];
        self.home = [false; CHANNELS];
        self.count = sequence.tracks.len().min(MAX_STRIPS);
        for (position, (strip, track)) in self
            .strips
            .iter_mut()
            .zip(&sequence.tracks)
            .take(self.count)
            .enumerate()
        {
            let song_track = track.track.get();
            let channel = usize::from(track.channel.get());
            let kept = old
                .iter()
                .take(old_count)
                .find(|old| old.song_track == song_track);
            *strip = match kept {
                Some(kept) => *kept,
                None => Strip {
                    song_track,
                    channel,
                    drums: track.sound.percussion,
                    volume: track.sound.volume,
                    pan: track.sound.pan,
                    reverb: track.sound.reverb,
                    chorus: track.sound.chorus,
                    peak: Peak::default(),
                    input_peak: Peak::default(),
                    output_peak: Peak::default(),
                },
            };
            if let Some(owner) = self.channel_strip.get_mut(channel)
                && owner.is_none()
            {
                *owner = Some(position);
            }
            if let Some(home) = self.home.get_mut(channel) {
                *home = true;
            }
        }
        // Tracks that no longer play: their meters fall silent.
        for strip in old.iter().take(old_count) {
            if !self
                .strips
                .iter()
                .take(self.count)
                .any(|now| now.song_track == strip.song_track)
                && strip.song_track < MAX_TONES
            {
                self.meters
                    .publish(Which::Track(strip.song_track), Peak::default());
                self.settle_parts(strip.song_track, Peak::default());
            }
        }
        // A track that is no longer a drum kit has no parts to show.
        for position in 0..self.count {
            let Some(strip) = self.strips.get(position).copied() else {
                continue;
            };
            if !strip.drums && strip.song_track < MAX_TONES {
                self.settle_parts(strip.song_track, Peak::default());
            }
        }
    }

    /// Silences the meters of every part of `track` (a drum track that no longer plays or
    /// is no longer a kit).
    fn settle_parts(&mut self, track: usize, peak: Peak) {
        if let Some(peaks) = self.part_peaks.get_mut(track) {
            *peaks = [peak; PARTS];
        }
        for part in 0..PARTS {
            self.meters.publish(Which::Part(track, part), peak);
        }
    }

    /// Takes an event for the synthesizer: the strips' controllers (volume, pan, reverb
    /// send) stay here and `false` comes back; anything else is the synthesizer's (`true`).
    /// `track` is the sequence's track the event belongs to, if known: a note on a channel
    /// that is no strip's own lends that channel to it (a bend).
    pub(crate) fn take(&mut self, event: &MidiEvent, track: Option<usize>) -> bool {
        match *event {
            MidiEvent::NoteOn { channel, .. } => {
                let channel = usize::from(channel.get());
                if let Some(track) = track.filter(|&track| track < self.count)
                    && !self.home.get(channel).copied().unwrap_or(true)
                    && let Some(owner) = self.channel_strip.get_mut(channel)
                {
                    *owner = Some(track);
                }
                true
            }
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => {
                let target = match controller {
                    controller::VOLUME => 0,
                    controller::PAN => 1,
                    controller::REVERB => 2,
                    controller::CHORUS => 3,
                    _ => return true,
                };
                // A borrowed channel's setup carries its next borrower's values, sent before
                // that borrower's note takes the channel over: they would land on the strip
                // that borrowed it last (a drum kit or guitar panned hard to the other side
                // for a while). A strip takes its values from its own channel only.
                if !self
                    .home
                    .get(usize::from(channel.get()))
                    .copied()
                    .unwrap_or(false)
                {
                    return false;
                }
                let strip = self
                    .channel_strip
                    .get(usize::from(channel.get()))
                    .copied()
                    .flatten()
                    .and_then(|position| self.strips.get_mut(position));
                if let Some(strip) = strip {
                    let value = value.min(127);
                    match target {
                        0 => strip.volume = value,
                        1 => strip.pan = value,
                        2 => strip.reverb = value,
                        _ => strip.chorus = value,
                    }
                }
                false
            }
            _ => true,
        }
    }

    /// Gives `track` the chain `chain` (`None`: none) with `settings`. Returns the chain it
    /// replaces, to be freed off the audio thread.
    pub(crate) fn set_chain(
        &mut self,
        track: usize,
        chain: Option<Box<Chain>>,
        knobs: ChainKnobs,
    ) -> Option<Box<Chain>> {
        self.tones.set_chain(track, chain, knobs)
    }

    /// New knob values for the chain of `track`, heard at once.
    pub(crate) fn set_chain_settings(&mut self, track: usize, knobs: ChainKnobs) {
        self.tones.set_settings(track, knobs);
    }

    /// Gives `part` of the song's drum track `track` the chain `chain` (`None`: none) with
    /// its knob values. Returns the chain it replaces, to be freed off the audio thread.
    pub(crate) fn set_part_chain(
        &mut self,
        track: usize,
        part: usize,
        chain: Option<Box<Chain>>,
        knobs: ChainKnobs,
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        if let Some(chain) = &mut chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
        let slot = self
            .part_chains
            .get_mut(track)
            .and_then(|parts| parts.get_mut(part))?;
        std::mem::replace(slot, chain)
    }

    /// New knob values for `part` of the song's drum track `track`, heard at once.
    pub(crate) fn set_part_chain_settings(&mut self, track: usize, part: usize, knobs: ChainKnobs) {
        if let Some(chain) = self
            .part_chains
            .get_mut(track)
            .and_then(|parts| parts.get_mut(part))
            .and_then(Option::as_mut)
        {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
    }

    /// The mixing level of `part` of the song's drum track `track` (0-127, 127 unity),
    /// heard at once.
    pub(crate) fn set_part_level(&mut self, track: usize, part: usize, level: u8) {
        if let Some(slot) = self
            .part_levels
            .get_mut(track)
            .and_then(|levels| levels.get_mut(part))
        {
            *slot = level.min(127);
        }
    }

    /// The pan of `part` of the song's drum track `track` (0 left, 64 centre, 127 right),
    /// heard at once.
    pub(crate) fn set_part_pan(&mut self, track: usize, part: usize, pan: u8) {
        if let Some(slot) = self
            .part_pans
            .get_mut(track)
            .and_then(|pans| pans.get_mut(part))
        {
            *slot = pan.min(127);
        }
    }

    /// Mutes or unmutes `part` of the song's drum track `track`, heard at once.
    pub(crate) fn set_part_mute(&mut self, track: usize, part: usize, muted: bool) {
        if let Some(slot) = self
            .part_mutes
            .get_mut(track)
            .and_then(|mutes| mutes.get_mut(part))
        {
            *slot = muted;
        }
    }

    /// Soloes or un-soloes `part` of the song's drum track `track`, heard at once.
    pub(crate) fn set_part_solo(&mut self, track: usize, part: usize, soloed: bool) {
        if let Some(slot) = self
            .part_solos
            .get_mut(track)
            .and_then(|solos| solos.get_mut(part))
        {
            *slot = soloed;
        }
    }

    /// Gives the master the chain `chain` (`None`: none) with its knob values. Returns the
    /// chain it replaces, to be freed off the audio thread.
    pub(crate) fn set_master_chain(
        &mut self,
        chain: Option<Box<Chain>>,
        knobs: ChainKnobs,
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        if let Some(chain) = &mut chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
        std::mem::replace(&mut self.master_chain, chain)
    }

    /// New knob values for the master's chain, heard at once.
    pub(crate) fn set_master_chain_settings(&mut self, knobs: ChainKnobs) {
        if let Some(chain) = &mut self.master_chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
    }

    /// Gives `bus` the chain `chain` (`None`: none) with its knob values. Returns the
    /// chain it replaces, to be freed off the audio thread.
    pub(crate) fn set_bus(
        &mut self,
        bus: Bus,
        chain: Option<Box<Chain>>,
        knobs: ChainKnobs,
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        if let Some(chain) = &mut chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
        let state = self.buses.get_mut(bus.index())?;
        std::mem::replace(&mut state.chain, chain)
    }

    /// New knob values, level and mute for `bus`, heard at once.
    pub(crate) fn set_bus_settings(&mut self, bus: Bus, knobs: ChainKnobs, level: u8, muted: bool) {
        let Some(state) = self.buses.get_mut(bus.index()) else {
            return;
        };
        state.mix = (level.min(127), muted);
        if let Some(chain) = &mut state.chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
    }

    /// Whether the strips' own sound goes to the output (`true`, a mix) or is left out so
    /// only the buses' returns are written (`false`, a bus stem). The sends and the meters
    /// work either way.
    pub(crate) fn set_dry(&mut self, dry: bool) {
        self.dry = dry;
    }

    /// Gives the song's drum track `track` its room (`None`: no room). Returns the chain it
    /// replaces, to be freed off the audio thread.
    pub(crate) fn set_track_room(
        &mut self,
        track: usize,
        chain: Option<Box<Chain>>,
    ) -> Option<Box<Chain>> {
        self.rooms
            .get_mut(track)
            .and_then(|slot| std::mem::replace(slot, chain))
    }

    /// The song's drum kit `track` sends `value` (0-127) of the whole kit into its room.
    pub(crate) fn set_track_room_send(&mut self, track: usize, value: u8) {
        if let Some(send) = self.room_sends.get_mut(track) {
            *send = value.min(127);
        }
    }

    /// New knob values for the song's drum kit `track`'s room, heard at once.
    pub(crate) fn set_track_room_settings(&mut self, track: usize, knobs: ChainKnobs) {
        if let Some(chain) = self.rooms.get_mut(track).and_then(Option::as_mut) {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
    }

    /// The song's drum part `part` of track `track` sends `value` (0-127) into the kit's
    /// room.
    pub(crate) fn set_part_room(&mut self, track: usize, part: usize, value: u8) {
        if let Some(slot) = self
            .part_rooms
            .get_mut(track)
            .and_then(|rooms| rooms.get_mut(part))
        {
            *slot = value.min(127);
        }
    }

    /// Sets the input trim of the song's track `track`, in dB (0: as recorded), before its
    /// chain: how hard an amp or a compressor is driven.
    pub(crate) fn set_trim(&mut self, track: usize, db: f32) {
        if let Some(trim) = self.trim_db.get_mut(track) {
            *trim = if db.is_finite() {
                db.clamp(-24.0, 24.0)
            } else {
                0.0
            };
        }
    }

    /// The song's track `track` sends `value` (0-127) into the compressor bus.
    pub(crate) fn set_comp_send(&mut self, track: usize, value: u8) {
        if let Some(send) = self.comp_sends.get_mut(track) {
            *send = value.min(127);
        }
    }

    /// Mixes `frames` (at most [`BLOCK`]) of the synthesizer's channels (already rendered
    /// into [`Mixer::channel_buffers`]) and of `strings` into `left` and `right`
    /// (overwritten).
    pub(crate) fn mix(
        &mut self,
        frames: usize,
        strings: &impl Strings,
        drums: &dyn DrumParts,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let frames = frames.min(BLOCK).min(left.len()).min(right.len());
        let (Some(left), Some(right)) = (left.get_mut(..frames), right.get_mut(..frames)) else {
            return;
        };
        left.fill(0.0);
        right.fill(0.0);
        for bus in &mut self.buses {
            for side in &mut bus.input {
                side.fill(0.0);
            }
        }
        // Channels no strip owns (sound left over from a sequence just replaced) go
        // straight out (never into a bus stem).
        if self.dry {
            for (channel, [from_left, from_right]) in self.channels.iter().enumerate() {
                if self.channel_strip.get(channel).copied().flatten().is_none() {
                    add(left, from_left);
                    add(right, from_right);
                }
            }
        }
        for position in 0..self.count {
            self.mix_strip(position, frames, strings, drums, left, right);
        }
        for bus in Bus::ALL {
            self.mix_bus(bus, frames, left, right);
        }
        // The master's chain hears the sum of the tracks and buses (never the metronome's
        // click: that is added after this, so a limiter here does not pump with it).
        if let Some(chain) = &mut self.master_chain {
            let sounding = peak_of(left) > 0.0 || peak_of(right) > 0.0;
            if sounding || chain.ringing() {
                chain.process_stereo(left, right);
            }
        }
    }

    /// One strip into `left` and `right` (and its send into the bus).
    fn mix_strip(
        &mut self,
        position: usize,
        frames: usize,
        strings: &impl Strings,
        drums: &dyn DrumParts,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let Some(strip) = self.strips.get(position).copied() else {
            return;
        };
        let [work_left, work_right] = &mut self.work;
        let (Some(work_left), Some(work_right)) =
            (work_left.get_mut(..frames), work_right.get_mut(..frames))
        else {
            return;
        };
        // The source: the strings (mono), the channels the track plays on (stereo), or -
        // for a drum track - its channel's parts, each through its own chain, summed.
        let mut part_reduction = 0.0_f32;
        // A drum track whose channel another drum track owns (both had to share channel 10:
        // more tracks than channels) gets nothing of its own, as any shared channel.
        let owns_channel =
            self.channel_strip.get(strip.channel).copied().flatten() == Some(position);
        let stereo = if strip.drums && !owns_channel {
            work_left.fill(0.0);
            work_right.fill(0.0);
            true
        } else if strip.drums {
            let channel = strip.channel;
            let [sum_left, sum_right] = &mut self.parts_work;
            let (Some(sum_left), Some(sum_right)) =
                (sum_left.get_mut(..frames), sum_right.get_mut(..frames))
            else {
                return;
            };
            sum_left.fill(0.0);
            sum_right.fill(0.0);
            // A soloed part silences the kit's other parts.
            let any_solo = self
                .part_solos
                .get(strip.song_track)
                .is_some_and(|solos| solos.iter().any(|solo| *solo));
            for part in 0..PARTS {
                let level = self
                    .part_levels
                    .get(strip.song_track)
                    .and_then(|levels| levels.get(part))
                    .copied()
                    .unwrap_or(127)
                    .min(127);
                let muted = self
                    .part_mutes
                    .get(strip.song_track)
                    .and_then(|mutes| mutes.get(part))
                    .copied()
                    .unwrap_or(false);
                let soloed = self
                    .part_solos
                    .get(strip.song_track)
                    .and_then(|solos| solos.get(part))
                    .copied()
                    .unwrap_or(false);
                let audible = !muted && (!any_solo || soloed);
                for (work, sample) in work_left.iter_mut().zip(
                    drums
                        .part(channel, part, 0)
                        .iter()
                        .chain(std::iter::repeat(&0.0)),
                ) {
                    *work = *sample;
                }
                for (work, sample) in work_right.iter_mut().zip(
                    drums
                        .part(channel, part, 1)
                        .iter()
                        .chain(std::iter::repeat(&0.0)),
                ) {
                    *work = *sample;
                }
                // A part that is silent with its chain's tail over is not run (a kit has
                // six parts, and most of them rest most of the time), as a strip's chain.
                let hit = peak_of(work_left) > 0.0 || peak_of(work_right) > 0.0;
                if let Some(chain) = self
                    .part_chains
                    .get_mut(strip.song_track)
                    .and_then(|parts| parts.get_mut(part))
                    .and_then(Option::as_mut)
                    .filter(|chain| hit || chain.ringing())
                {
                    chain.process_in_place(work_left, work_right, true);
                    part_reduction = part_reduction.min(chain.take_gain_reduction());
                }
                // The level, and the pan (the part is stereo already: centre leaves it as
                // it is).
                let pan = self
                    .part_pans
                    .get(strip.song_track)
                    .and_then(|pans| pans.get(part))
                    .copied()
                    .unwrap_or(64);
                let (ll, lr, rr, rl) = part_pan(pan);
                let gain = f32::from(level) / 127.0;
                if level < 127 || pan != 64 {
                    for (l, r) in work_left.iter_mut().zip(work_right.iter_mut()) {
                        let (from_l, from_r) = (*l, *r);
                        *l = (from_l * ll + from_r * lr) * gain;
                        *r = (from_r * rr + from_l * rl) * gain;
                    }
                }
                // Its Room send, after the level and pan: into the kit's one shared room.
                let room_send = self
                    .part_rooms
                    .get(strip.song_track)
                    .and_then(|rooms| rooms.get(part))
                    .copied()
                    .unwrap_or(0)
                    .min(127);
                if audible
                    && room_send > 0
                    && let Some([room_left, room_right]) = self.room_input.get_mut(strip.song_track)
                {
                    let send = f32::from(room_send) / 127.0;
                    for (index, (l, r)) in work_left.iter().zip(work_right.iter()).enumerate() {
                        if let (Some(into_l), Some(into_r)) =
                            (room_left.get_mut(index), room_right.get_mut(index))
                        {
                            *into_l += l * send;
                            *into_r += r * send;
                        }
                    }
                }
                // The part's meter, after its chain, level and mute.
                let fall = self.ballistics;
                let peak = self
                    .part_peaks
                    .get_mut(strip.song_track)
                    .and_then(|peaks| peaks.get_mut(part))
                    .map(|peak| {
                        let (left, right) = if audible {
                            (peak_of(work_left), peak_of(work_right))
                        } else {
                            (0.0, 0.0)
                        };
                        peak.update(left, right, fall);
                        *peak
                    });
                if let Some(peak) = peak {
                    self.meters
                        .publish(Which::Part(strip.song_track, part), peak);
                }
                if !audible {
                    continue;
                }
                add(sum_left, work_left);
                add(sum_right, work_right);
            }
            work_left.copy_from_slice(sum_left);
            work_right.copy_from_slice(sum_right);
            // The kit's own Room knob: the whole kit (after the parts' levels, before the
            // track's chain) into the same room.
            let kit_send = self
                .room_sends
                .get(strip.song_track)
                .copied()
                .unwrap_or(0)
                .min(127);
            if kit_send > 0
                && let Some([room_left, room_right]) = self.room_input.get_mut(strip.song_track)
            {
                let send = f32::from(kit_send) / 127.0;
                for (index, (l, r)) in work_left.iter().zip(work_right.iter()).enumerate() {
                    if let (Some(into_l), Some(into_r)) =
                        (room_left.get_mut(index), room_right.get_mut(index))
                    {
                        *into_l += l * send;
                        *into_r += r * send;
                    }
                }
            }
            // The kit's room: the parts' sends through one short shared room, its return
            // part of the kit (before the track's own chain and fader, so the kit's
            // processing and its sends hear it; a bus stem leaves only the dry sound out).
            if let Some(input) = self.room_input.get_mut(strip.song_track) {
                let sent = {
                    let [left, right] = &*input;
                    left.iter().chain(right.iter()).any(|sample| *sample != 0.0)
                };
                let ringing = self
                    .rooms
                    .get(strip.song_track)
                    .and_then(Option::as_ref)
                    .is_some_and(|chain| chain.ringing());
                if sent || ringing {
                    let mut room_left = *input.first().unwrap_or(&[0.0; BLOCK]);
                    let mut room_right = *input.get(1).unwrap_or(&[0.0; BLOCK]);
                    if let Some(chain) = self
                        .rooms
                        .get_mut(strip.song_track)
                        .and_then(Option::as_mut)
                        && let (Some(left), Some(right)) =
                            (room_left.get_mut(..frames), room_right.get_mut(..frames))
                    {
                        chain.process_in_place(left, right, true);
                        add(work_left, left);
                        add(work_right, right);
                    }
                }
                input.fill([0.0; BLOCK]);
            }
            true
        } else {
            match strings.slot_of(position) {
                Some(slot) => {
                    let output = strings.output(slot);
                    for (work, sample) in work_left
                        .iter_mut()
                        .zip(output.iter().chain(std::iter::repeat(&0.0)))
                    {
                        *work = *sample;
                    }
                    false
                }
                None => {
                    work_left.fill(0.0);
                    work_right.fill(0.0);
                    for (channel, [from_left, from_right]) in self.channels.iter().enumerate() {
                        if self.channel_strip.get(channel).copied().flatten() == Some(position) {
                            add(work_left, from_left);
                            add(work_right, from_right);
                        }
                    }
                    true
                }
            }
        };
        // The input trim, before the chain (and its meter, so the amp's drive can be set
        // by how hard the input hits it).
        let trim = self.trim_db.get(strip.song_track).copied().unwrap_or(0.0);
        let trim_gain = if trim.abs() > 1e-3 {
            10.0_f32.powf(trim / 20.0)
        } else {
            1.0
        };
        if (trim_gain - 1.0).abs() > 1e-6 {
            for sample in work_left.iter_mut().chain(work_right.iter_mut()) {
                *sample *= trim_gain;
            }
        }
        // Its input meter: what reaches the chain, after the trim.
        let fall = self.ballistics;
        if strip.song_track < MAX_TONES
            && let Some(strip) = self.strips.get_mut(position)
        {
            strip
                .input_peak
                .update(peak_of(work_left), peak_of(work_right), fall);
            let peak = strip.input_peak;
            let track = strip.song_track;
            self.meters.publish(Which::Input(track), peak);
        }
        let sounding = peak_of(work_left) > 0.0 || (stereo && peak_of(work_right) > 0.0);
        let chain = self.tones.chain(strip.song_track);
        let ringing = chain.as_ref().is_some_and(|chain| chain.ringing());
        if !sounding && !ringing {
            self.settle_meter(position, 0.0, 0.0, 0.0);
            self.settle_chain_output(position);
            return;
        }
        let reduction = match chain {
            Some(chain) => {
                chain.process_in_place(work_left, work_right, stereo);
                chain.take_gain_reduction().min(part_reduction)
            }
            None => {
                if !stereo {
                    work_right.copy_from_slice(work_left);
                }
                part_reduction
            }
        };
        // What leaves the chain (and its output level), before the fader and pan: its own
        // meter, for setting the chain's output.
        let (chain_peak, track) = match self.strips.get_mut(position) {
            Some(strip) => {
                strip
                    .output_peak
                    .update(peak_of(work_left), peak_of(work_right), self.ballistics);
                (strip.output_peak, strip.song_track)
            }
            None => (Peak::default(), usize::MAX),
        };
        if track < MAX_TONES {
            self.meters.publish(Which::ChainOutput(track), chain_peak);
        }
        // The fader and the pan (a balance once the signal is stereo), then the sends.
        let gain = volume_gain(strip.volume);
        let (to_left, to_right) = pan_gains(strip.pan);
        let send = |bus: &BusState, value: u8| {
            if bus.chain.is_some() {
                f32::from(value.min(127)) / 127.0
            } else {
                0.0
            }
        };
        let [reverb, compressor, chorus] = &mut self.buses;
        let reverb_send = send(reverb, strip.reverb);
        let comp_send = send(
            compressor,
            self.comp_sends.get(strip.song_track).copied().unwrap_or(0),
        );
        let chorus_send = send(chorus, strip.chorus);
        let [reverb_in, _] = &mut reverb.input;
        let [comp_left, comp_right] = &mut compressor.input;
        let [chorus_in, _] = &mut chorus.input;
        let dry = self.dry;
        let (mut peak_left, mut peak_right) = (0.0_f32, 0.0_f32);
        for (index, ((l, r), (wl, wr))) in left
            .iter_mut()
            .zip(right.iter_mut())
            .zip(work_left.iter().zip(work_right.iter()))
            .enumerate()
        {
            let (out_l, out_r) = (wl * gain * to_left, wr * gain * to_right);
            if dry {
                *l += out_l;
                *r += out_r;
            }
            peak_left = peak_left.max(out_l.abs());
            peak_right = peak_right.max(out_r.abs());
            // The reverb hears a mono sum (it makes its own stereo); the compressor bus
            // keeps both sides.
            if reverb_send > 0.0
                && let Some(bus) = reverb_in.get_mut(index)
            {
                *bus += 0.5 * (out_l + out_r) * reverb_send;
            }
            if comp_send > 0.0
                && let (Some(bus_l), Some(bus_r)) =
                    (comp_left.get_mut(index), comp_right.get_mut(index))
            {
                *bus_l += out_l * comp_send;
                *bus_r += out_r * comp_send;
            }
            // The chorus hears a mono sum too, and makes its own stereo.
            if chorus_send > 0.0
                && let Some(bus) = chorus_in.get_mut(index)
            {
                *bus += 0.5 * (out_l + out_r) * chorus_send;
            }
        }
        self.settle_meter(position, peak_left, peak_right, reduction);
    }

    /// Silences the chain-output meter of strip `position` (its chain is not running).
    fn settle_chain_output(&mut self, position: usize) {
        let fall = self.ballistics;
        if let Some(strip) = self.strips.get_mut(position) {
            strip.output_peak.update(0.0, 0.0, fall);
            let peak = strip.output_peak;
            let track = strip.song_track;
            if track < MAX_TONES {
                self.meters.publish(Which::ChainOutput(track), peak);
            }
        }
    }

    /// The meter of strip `position` after a block with these peaks and its chain's
    /// deepest gain reduction (dB).
    fn settle_meter(&mut self, position: usize, left: f32, right: f32, reduction: f32) {
        let fall = self.ballistics;
        let Some(strip) = self.strips.get_mut(position) else {
            return;
        };
        strip.peak.update(left, right, fall);
        let track = strip.song_track;
        if track < MAX_TONES {
            self.meters.publish(Which::Track(track), strip.peak);
            if let Some(needle) = self.needles.get_mut(track) {
                needle.update(reduction, fall);
                self.meters.publish_reduction(Some(track), *needle);
            }
        }
    }

    /// Runs `bus` (a reverb rings on after the sends stop) and adds it to the mix.
    fn mix_bus(&mut self, bus: Bus, frames: usize, left: &mut [f32], right: &mut [f32]) {
        let fall = self.ballistics;
        let which = match bus {
            Bus::Reverb => Which::Reverb,
            Bus::Compressor => Which::Compressor,
            Bus::Chorus => Which::Chorus,
        };
        let Some(state) = self.buses.get_mut(bus.index()) else {
            return;
        };
        let [in_left, in_right] = &state.input;
        let (in_left, in_right) = (
            in_left.get(..frames).unwrap_or_default(),
            in_right.get(..frames).unwrap_or_default(),
        );
        let sent = in_left
            .iter()
            .chain(in_right.iter())
            .any(|sample| sample.abs() > 1e-9);
        let mut reduction = 0.0;
        let played = match &mut state.chain {
            // Silent and its tail over: nothing to do (and it may rest).
            Some(chain) if sent || chain.ringing() => {
                let [out_left, out_right] = &mut state.output;
                let (out_left, out_right) = (
                    out_left.get_mut(..frames).unwrap_or_default(),
                    out_right.get_mut(..frames).unwrap_or_default(),
                );
                match bus {
                    // The reverb and the chorus hear the sends in mono and make them
                    // stereo; the compressor keeps both sides of the sends.
                    Bus::Reverb | Bus::Chorus => {
                        chain.process(in_left, out_left, out_right);
                    }
                    Bus::Compressor => {
                        out_left.copy_from_slice(in_left);
                        out_right.copy_from_slice(in_right);
                        chain.process_stereo(out_left, out_right);
                    }
                }
                reduction = chain.take_gain_reduction();
                true
            }
            _ => false,
        };
        let (mut peak_left, mut peak_right) = (0.0_f32, 0.0_f32);
        if played {
            let (level, muted) = state.mix;
            let gain = if muted {
                0.0
            } else if bus == Bus::Reverb {
                volume_gain(level) * 10.0_f32.powf(REVERB_RETURN_DB / 20.0)
            } else {
                volume_gain(level)
            };
            let [out_left, out_right] = &state.output;
            for (((l, r), bl), br) in left
                .iter_mut()
                .zip(right.iter_mut())
                .zip(out_left.iter())
                .zip(out_right.iter())
            {
                let (out_l, out_r) = (bl * gain, br * gain);
                *l += out_l;
                *r += out_r;
                peak_left = peak_left.max(out_l.abs());
                peak_right = peak_right.max(out_r.abs());
            }
        }
        state.peak.update(peak_left, peak_right, fall);
        self.meters.publish(which, state.peak);
        if bus == Bus::Compressor {
            state.needle.update(reduction, fall);
            self.meters.publish_reduction(None, state.needle);
        }
    }

    /// Measures the finished mix (after the metronome is added).
    pub(crate) fn meter_master(&mut self, left: &[f32], right: &[f32]) {
        let fall = self.ballistics;
        self.master_peak.update(peak_of(left), peak_of(right), fall);
        self.meters.publish(Which::Master, self.master_peak);
    }
}

/// Adds `from` to `to`, sample by sample (as far as both reach).
fn add(to: &mut [f32], from: &[f32]) {
    for (to, from) in to.iter_mut().zip(from) {
        *to += from;
    }
}

#[cfg(test)]
#[path = "mixer_tests.rs"]
mod tests;
