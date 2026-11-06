//! The mixer of the synth engine: every track of the sequence is a strip, as on a mixing
//! desk.
//!
//! ```text
//!  SoundFont channels ─┐                                ┌──────────────────► master ─► out
//!  (or the strings)    ├─► chain ─► fader ─► pan ──────┤                       ▲
//!                      │                                └─► reverb send ─► reverb bus
//! ```
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
//! - **Meters** measure each strip after its fader, the reverb bus and the master
//!   ([`crate::Meters`]).
//!
//! Nothing here allocates or waits: it runs in the audio callback.

use std::sync::Arc;

use gscore_guitar::{BLOCKS, BlockSettings, Chain};
use gscore_play::{MidiEvent, Sequence, controller};

use crate::chains::{MAX_TONES, TrackTones};
use crate::meters::{Meters, Peak, Which, fall_per_block, peak_of};
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

/// Left and right gains for a pan (0 left, 64 middle, 127 right), equal power, 1 in the
/// middle.
pub(crate) fn pan_gains(pan: u8) -> (f32, f32) {
    let angle = f32::from(pan.min(127)) / 127.0 * std::f32::consts::FRAC_PI_2;
    let boost = std::f32::consts::SQRT_2;
    (angle.cos() * boost, angle.sin() * boost)
}

/// One track's strip.
#[derive(Debug, Clone, Copy)]
struct Strip {
    /// The song's track it plays (its chain and meter are that track's).
    song_track: usize,
    volume: u8,
    pan: u8,
    reverb: u8,
    peak: Peak,
}

impl Default for Strip {
    fn default() -> Self {
        Strip {
            song_track: usize::MAX,
            volume: 100,
            pan: 64,
            reverb: 0,
            peak: Peak::default(),
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
    /// The reverb bus every track sends into (all wet), its level (0-127) and mute.
    reverb_bus: Option<Box<Chain>>,
    bus_mix: (u8, bool),
    bus_in: [f32; BLOCK],
    bus_out: [[f32; BLOCK]; 2],
    bus_peak: Peak,
    master_peak: Peak,
    /// A strip's signal on its way through.
    work: [[f32; BLOCK]; 2],
    meters: Arc<Meters>,
    fall: f32,
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
            reverb_bus: None,
            bus_mix: (127, false),
            bus_in: [0.0; BLOCK],
            bus_out: [[0.0; BLOCK]; 2],
            bus_peak: Peak::default(),
            master_peak: Peak::default(),
            work: [[0.0; BLOCK]; 2],
            meters: Arc::new(Meters::default()),
            fall: fall_per_block(sample_rate, BLOCK),
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
                    volume: track.sound.volume,
                    pan: track.sound.pan,
                    reverb: track.sound.reverb,
                    peak: Peak::default(),
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
            }
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
                    _ => return true,
                };
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
                        _ => strip.reverb = value,
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
        settings: [BlockSettings; BLOCKS],
    ) -> Option<Box<Chain>> {
        self.tones.set_chain(track, chain, settings)
    }

    /// New knob values for the chain of `track`, heard at once.
    pub(crate) fn set_chain_settings(&mut self, track: usize, settings: [BlockSettings; BLOCKS]) {
        self.tones.set_settings(track, settings);
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

    /// Mixes `frames` (at most [`BLOCK`]) of the synthesizer's channels (already rendered
    /// into [`Mixer::channel_buffers`]) and of `strings` into `left` and `right`
    /// (overwritten).
    pub(crate) fn mix(
        &mut self,
        frames: usize,
        strings: &impl Strings,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let frames = frames.min(BLOCK).min(left.len()).min(right.len());
        let (Some(left), Some(right)) = (left.get_mut(..frames), right.get_mut(..frames)) else {
            return;
        };
        left.fill(0.0);
        right.fill(0.0);
        if let Some(input) = self.bus_in.get_mut(..frames) {
            input.fill(0.0);
        }
        // Channels no strip owns (sound left over from a sequence just replaced) go
        // straight out.
        for (channel, [from_left, from_right]) in self.channels.iter().enumerate() {
            if self.channel_strip.get(channel).copied().flatten().is_none() {
                add(left, from_left);
                add(right, from_right);
            }
        }
        for position in 0..self.count {
            self.mix_strip(position, frames, strings, left, right);
        }
        self.mix_reverb_bus(frames, left, right);
    }

    /// One strip into `left` and `right` (and its send into the bus).
    fn mix_strip(
        &mut self,
        position: usize,
        frames: usize,
        strings: &impl Strings,
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
        // The source: the strings (mono), or the channels the track plays on (stereo).
        let stereo = match strings.slot_of(position) {
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
        };
        let sounding = peak_of(work_left) > 0.0 || (stereo && peak_of(work_right) > 0.0);
        let chain = self.tones.chain(strip.song_track);
        let ringing = chain.as_ref().is_some_and(|chain| chain.ringing());
        if !sounding && !ringing {
            self.settle_meter(position, 0.0, 0.0);
            return;
        }
        match chain {
            Some(chain) => chain.process_in_place(work_left, work_right, stereo),
            None if !stereo => work_right.copy_from_slice(work_left),
            None => {}
        }
        // The fader and the pan (a balance once the signal is stereo), then the send.
        let gain = volume_gain(strip.volume);
        let (to_left, to_right) = pan_gains(strip.pan);
        let send = if self.reverb_bus.is_some() {
            f32::from(strip.reverb.min(127)) / 127.0
        } else {
            0.0
        };
        let (mut peak_left, mut peak_right) = (0.0_f32, 0.0_f32);
        let bus_in = self.bus_in.get_mut(..frames).unwrap_or_default();
        for ((((l, r), wl), wr), bus) in left
            .iter_mut()
            .zip(right.iter_mut())
            .zip(work_left.iter())
            .zip(work_right.iter())
            .zip(bus_in.iter_mut())
        {
            let (out_l, out_r) = (wl * gain * to_left, wr * gain * to_right);
            *l += out_l;
            *r += out_r;
            peak_left = peak_left.max(out_l.abs());
            peak_right = peak_right.max(out_r.abs());
            if send > 0.0 {
                *bus += 0.5 * (out_l + out_r) * send;
            }
        }
        self.settle_meter(position, peak_left, peak_right);
    }

    /// The meter of strip `position` after a block with these peaks.
    fn settle_meter(&mut self, position: usize, left: f32, right: f32) {
        let fall = self.fall;
        let Some(strip) = self.strips.get_mut(position) else {
            return;
        };
        strip.peak.update(left, right, fall);
        if strip.song_track < MAX_TONES {
            self.meters
                .publish(Which::Track(strip.song_track), strip.peak);
        }
    }

    /// Runs the reverb bus (which rings on after the sends stop) and adds it to the mix.
    fn mix_reverb_bus(&mut self, frames: usize, left: &mut [f32], right: &mut [f32]) {
        let fall = self.fall;
        let Some(bus) = &mut self.reverb_bus else {
            self.bus_peak.update(0.0, 0.0, fall);
            self.meters.publish(Which::Reverb, self.bus_peak);
            return;
        };
        let (level, muted) = self.bus_mix;
        let input = self.bus_in.get(..frames).unwrap_or_default();
        let sent = input.iter().any(|sample| sample.abs() > 1e-9);
        // Silent and its tail over: nothing to do (and it may rest).
        if !sent && !bus.ringing() {
            self.bus_peak.update(0.0, 0.0, fall);
            self.meters.publish(Which::Reverb, self.bus_peak);
            return;
        }
        let [out_left, out_right] = &mut self.bus_out;
        let (out_left, out_right) = (
            out_left.get_mut(..frames).unwrap_or_default(),
            out_right.get_mut(..frames).unwrap_or_default(),
        );
        bus.process(input, out_left, out_right);
        let gain = if muted { 0.0 } else { volume_gain(level) };
        let (mut peak_left, mut peak_right) = (0.0_f32, 0.0_f32);
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
        self.bus_peak.update(peak_left, peak_right, fall);
        self.meters.publish(Which::Reverb, self.bus_peak);
    }

    /// Measures the finished mix (after the metronome is added).
    pub(crate) fn meter_master(&mut self, left: &[f32], right: &[f32]) {
        let fall = self.fall;
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
