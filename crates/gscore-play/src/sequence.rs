//! Turns a song into a [`Sequence`]: timed MIDI events per track, a tempo map, and where
//! each played measure lies in time. The events are derived from the song's
//! [`Performance`], which the sequence carries for engines that play notes, not MIDI.
//!
//! Notes play on their track's channel. A note with a bend or slide borrows a free channel
//! for its own pitch bend (AGENTS.md section 5, decision 10): the borrowed channel gets the
//! track's program, volume and pan plus a 6-semitone bend range (wider for a wide slide),
//! and is released after the note. When every other channel is in use the note plays
//! without its pitch movement and [`Sequence::degraded_pitch_bends`] counts it.

use std::sync::Arc;

use gscore_model::{
    BeatIndex, BeatKind, BeatPosition, Duration, MeasureIndex, ModelError, Song, TimeSignature,
    TrackIndex, TrackSound,
};

use crate::click::Click;
use crate::event::{Channel, MidiEvent, PITCH_BEND_CENTER, controller};
use crate::performance::{Performance, StringKind};
use crate::swing;
use crate::tempo::{PPQ, TempoMap};

/// Ticks in a whole note.
const WHOLE: u64 = 4 * PPQ as u64;
/// A track as the sequence plays it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceTrack {
    /// The track in the song.
    pub track: TrackIndex,
    /// Its name.
    pub name: String,
    /// The MIDI channel it plays on.
    pub channel: Channel,
    /// Its sound (program, volume, pan).
    pub sound: TrackSound,
}

/// One event at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    /// When, in ticks ([`PPQ`] per quarter note) from the start.
    pub tick: u64,
    /// Which entry of [`Sequence::tracks`] it belongs to.
    pub track: usize,
    /// The message.
    pub event: MidiEvent,
}

/// Where one played measure lies in time. With repeats, a measure appears several times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureSpan {
    /// The measure.
    pub measure: MeasureIndex,
    /// First tick.
    pub start: u64,
    /// Tick after the last musical beat (a fermata's hold comes after it).
    pub end: u64,
    /// Silence after the measure, in ticks: its fermata's hold, or 0.
    pub pause: u64,
    /// Its time signature.
    pub signature: TimeSignature,
}

/// A song ready to play or export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    /// The tracks that play.
    pub tracks: Vec<SequenceTrack>,
    /// All events, sorted by tick. At one tick: note-offs, setup, pitch resets, pitch
    /// movement, then note-ons.
    pub events: Vec<TimedEvent>,
    /// Tempo changes.
    pub tempo: TempoMap,
    /// The played measures, in playing order.
    pub measures: Vec<MeasureSpan>,
    /// The tick where the piece ends.
    pub end: u64,
    /// Notes left out because their pitch could not be worked out (invalid string, fret or
    /// a pitch above MIDI's range). The model lists such notes as issues.
    pub skipped_notes: usize,
    /// Notes whose bend or slide was dropped because every other MIDI channel was in use.
    /// They still play, on their track's channel, without the pitch movement.
    pub degraded_pitch_bends: usize,
    /// The performance the events were derived from (empty for a count-in or a sequence
    /// received without one): the notes an engine that plays strings works from.
    pub performance: Arc<Performance>,
}

impl Sequence {
    /// The channel `track` plays on in this sequence, or `None` if it does not play (it is
    /// muted, not soloed, or not part of the song).
    pub fn channel_of(&self, track: TrackIndex) -> Option<Channel> {
        self.tracks
            .iter()
            .find(|entry| entry.track == track)
            .map(|entry| entry.channel)
    }

    /// The sequence of `tracks` of `song` (all tracks if `None`), without a metronome.
    pub fn new(song: &Song, tracks: Option<&[TrackIndex]>) -> Result<Self, ModelError> {
        Self::with_metronome(song, tracks, false)
    }

    /// The sequence of `tracks` of `song` (all tracks if `None`). With `metronome`, a click
    /// track is added on the percussion channel: one click per beat, accented on the first
    /// beat of every measure.
    pub fn with_metronome(
        song: &Song,
        tracks: Option<&[TrackIndex]>,
        metronome: bool,
    ) -> Result<Self, ModelError> {
        Self::with_click(song, tracks, metronome.then(Click::default))
    }

    /// Like [`Sequence::with_metronome`], clicking as `click` says (`None`: no metronome).
    /// At volume 0 the metronome is silent even though it is on.
    pub fn with_click(
        song: &Song,
        tracks: Option<&[TrackIndex]>,
        click: Option<Click>,
    ) -> Result<Self, ModelError> {
        let performance = Performance::new(song, tracks)?;
        let mut sequence = Sequence::from_performance(Arc::new(performance));
        if let Some(click) = click {
            sequence
                .events
                .push(crate::click::sound_event(click.sound, 0));
            for span in &sequence.measures {
                // Free time has no metronome, as in Guitar Pro.
                if song.free_time(span.measure) == Some(true) {
                    continue;
                }
                sequence
                    .events
                    .extend(crate::click::click_events_at(*span, click.volume));
            }
            sequence.events.sort_by_key(|e| (e.tick, rank(&e.event)));
        }
        Ok(sequence)
    }

    /// This sequence with its guitar and bass tracks played by the modelled strings
    /// ([`Performance::model_guitars`]); the MIDI events stay for MIDI output and export.
    pub fn with_modelled_guitars(self) -> Self {
        self.with_modelled_guitars_where(StringKind::for_program)
    }

    /// The same, the kind of string each preset (General MIDI program) starts from decided
    /// by `kind_of`: a preset the user gave a tone of its own plays that tone's kind of
    /// string, the rest their program's family.
    pub fn with_modelled_guitars_where(
        mut self,
        kind_of: impl Fn(u8) -> Option<StringKind>,
    ) -> Self {
        Arc::make_mut(&mut self.performance).model_guitars_where(kind_of);
        self
    }

    /// This sequence with only the tracks whose song index `keep` accepts. Metronome clicks
    /// stay (they belong to no track), so the beat can still be kept by an output that has
    /// only some of the song's tracks. Track positions in events and in the performance
    /// are renumbered, so an engine that plays the notes (the modelled strings) keeps
    /// working.
    pub fn with_tracks(&self, keep: impl Fn(TrackIndex) -> bool) -> Self {
        let mut new_position = Vec::with_capacity(self.tracks.len());
        let mut tracks = Vec::with_capacity(self.tracks.len());
        let mut performance_tracks = Vec::with_capacity(self.tracks.len());
        for (position, track) in self.tracks.iter().enumerate() {
            if keep(track.track) {
                new_position.push(Some(tracks.len()));
                tracks.push(track.clone());
                if let Some(entry) = self.performance.tracks.get(position) {
                    performance_tracks.push(entry.clone());
                }
            } else {
                new_position.push(None);
            }
        }
        let mut sequence = self.clone();
        sequence.tracks = tracks;
        sequence.events = self
            .events
            .iter()
            .filter_map(|event| {
                if event.track == crate::click::CLICK_TRACK {
                    return Some(*event);
                }
                let track = new_position.get(event.track).copied().flatten()?;
                Some(TimedEvent { track, ..*event })
            })
            .collect();
        let performance = Arc::make_mut(&mut sequence.performance);
        performance.tracks = performance_tracks;
        performance.notes = self
            .performance
            .notes
            .iter()
            .filter_map(|note| {
                let track = new_position.get(note.track).copied().flatten()?;
                Some(crate::performance::PlayedNote { track, ..*note })
            })
            .collect();
        performance.index_by_start();
        sequence
    }

    /// The MIDI sequence that plays `performance` on a General MIDI synthesizer.
    pub fn from_performance(performance: Arc<Performance>) -> Self {
        let midi = crate::midi::derive(&performance);
        let mut events = midi.events;
        events.sort_by_key(|e| (e.tick, rank(&e.event)));
        Sequence {
            tracks: midi.tracks,
            events,
            tempo: performance.tempo.clone(),
            measures: performance.measures.clone(),
            end: performance.end,
            skipped_notes: performance.skipped_notes,
            degraded_pitch_bends: midi.degraded_pitch_bends,
            performance,
        }
    }

    /// The length of the whole sequence in microseconds.
    pub fn micros(&self) -> u64 {
        self.tempo.micros_at(self.end)
    }

    /// Which channels play drums: 10, and those of further drum tracks (each drum track has
    /// a channel of its own, see [`crate::ChannelAllocator`]). Indexed by channel number.
    pub fn drum_channels(&self) -> [bool; 16] {
        let mut drums = [false; 16];
        let mut mark = |channel: Channel| {
            if let Some(slot) = drums.get_mut(usize::from(channel.get())) {
                *slot = true;
            }
        };
        mark(Channel::PERCUSSION);
        for track in self.tracks.iter().filter(|track| track.sound.percussion) {
            mark(track.channel);
        }
        drums
    }

    /// `event` as a General MIDI device takes it: on channel 10 if it goes to one of
    /// `drums` (see [`Sequence::drum_channels`]), which such a device plays drums on only.
    pub fn general_midi_event(drums: &[bool; 16], event: MidiEvent) -> MidiEvent {
        if drums
            .get(usize::from(event.channel().get()))
            .copied()
            .unwrap_or(false)
        {
            event.on_channel(Channel::PERCUSSION)
        } else {
            event
        }
    }

    /// This sequence for a General MIDI device (a MIDI port, a MIDI file): every drum track
    /// on channel 10. `None` if they are already (one drum track or none).
    pub fn general_midi(&self) -> Option<Sequence> {
        let drums = self.drum_channels();
        let others = drums
            .iter()
            .enumerate()
            .any(|(channel, &drum)| drum && channel != usize::from(Channel::PERCUSSION.get()));
        if !others {
            return None;
        }
        let mut sequence = self.clone();
        for track in &mut sequence.tracks {
            if track.sound.percussion {
                track.channel = Channel::PERCUSSION;
            }
        }
        for event in &mut sequence.events {
            event.event = Sequence::general_midi_event(&drums, event.event);
        }
        Some(sequence)
    }

    /// The channels this sequence uses, including borrowed ones.
    pub fn channels(&self) -> Vec<Channel> {
        let mut channels: Vec<Channel> = self.tracks.iter().map(|t| t.channel).collect();
        channels.extend(self.events.iter().map(|event| event.event.channel()));
        channels.sort_unstable();
        channels.dedup();
        channels
    }

    /// The tick where the beat at `at` of `song` (the song this sequence was made from)
    /// is first played: its measure's first pass, plus the beats before it in its voice.
    /// A position past the voice's last beat (the cursor in an empty measure) is the
    /// measure's start. `None` if the measure is never played (or does not exist).
    pub fn tick_of(&self, song: &Song, at: BeatPosition) -> Option<u64> {
        beat_tick(&self.measures, song, at)
    }

    /// The played measure at `tick` (the last one at or after the end).
    pub fn measure_at(&self, tick: u64) -> Option<MeasureSpan> {
        let after = self.measures.partition_point(|m| m.start <= tick);
        self.measures.get(after.saturating_sub(1)).copied()
    }

    /// The beat of `at.track` in `at.voice` that is playing at `tick`, for the cursor to
    /// follow playback. Repeats are handled because [`Self::measure_at`] knows the played
    /// order. A tick before the first beat is beat 0; one past the voice's last beat is the
    /// last beat (or beat 0 in a voice with no beats, or of a measure without that voice:
    /// Guitar Pro stores an empty bar without voices, and the cursor must still move onto
    /// it). `None` if `at.track` does not exist.
    pub fn beat_at(&self, song: &Song, at: BeatPosition, tick: u64) -> Option<BeatPosition> {
        let span = self.measure_at(tick)?;
        let measure = song.track(at.track)?.measure(span.measure)?;
        let first = BeatPosition {
            track: at.track,
            measure: span.measure,
            voice: at.voice,
            beat: BeatIndex::new(0),
        };
        let Some(voice) = measure.voice(at.voice) else {
            return Some(first);
        };
        let feel = song.feel(span.measure).unwrap_or_default();
        let measure_ticks = span.end.saturating_sub(span.start);
        let mut onset = Duration::new(0, 1).ok()?;
        let mut last = 0_usize;
        for (index, beat) in voice.beats().enumerate() {
            if beat.kind() == BeatKind::Empty || beat.is_grace() {
                continue;
            }
            last = index;
            let length = beat.duration().ok()?;
            let next = onset.checked_add(length).ok()?;
            let end =
                span.start
                    .saturating_add(swing::swing_offset(ticks(next), measure_ticks, feel));
            if tick < end {
                return Some(BeatPosition {
                    track: at.track,
                    measure: span.measure,
                    voice: at.voice,
                    beat: BeatIndex::new(index),
                });
            }
            onset = next;
        }
        Some(BeatPosition {
            track: at.track,
            measure: span.measure,
            voice: at.voice,
            beat: BeatIndex::new(last),
        })
    }
}

/// The messages that give `channel` a track's sound: program (not for drums), volume, pan,
/// reverb and chorus send, and full expression (a damped note may have left it at 0). Used for a sequence's setup and to change a sound while playing.
pub fn sound_events(channel: Channel, sound: TrackSound) -> Vec<MidiEvent> {
    let mut events = Vec::with_capacity(6);
    if !sound.percussion {
        events.push(MidiEvent::ProgramChange {
            channel,
            program: sound.program,
        });
    }
    for (controller, value) in [
        (controller::VOLUME, sound.volume),
        (controller::PAN, sound.pan),
        (controller::REVERB, sound.reverb),
        (controller::CHORUS, sound.chorus),
        (controller::EXPRESSION, 127),
    ] {
        events.push(MidiEvent::Control {
            channel,
            controller,
            value: value.min(127),
        });
    }
    events
}

/// The order of messages at one tick.
pub(crate) fn rank(event: &MidiEvent) -> u8 {
    match event {
        // A reused borrowed channel: release the old note before its channel is set up.
        MidiEvent::NoteOff { .. } => 0,
        MidiEvent::ProgramChange { .. } | MidiEvent::Control { .. } => 1,
        // Reset a bend after the note that used it.
        MidiEvent::PitchBend { value, .. } if *value == PITCH_BEND_CENTER => 2,
        // The initial value or a ramp step, before the note starts.
        MidiEvent::PitchBend { .. } => 3,
        MidiEvent::NoteOn { .. } => 4,
    }
}

/// A duration (in whole notes) in ticks, rounded to the nearest tick.
pub(crate) fn ticks(duration: Duration) -> u64 {
    let numerator = u128::from(duration.numerator()).saturating_mul(u128::from(WHOLE));
    let denominator = u128::from(duration.denominator().max(1));
    let rounded = numerator
        .saturating_add(denominator / 2)
        .checked_div(denominator)
        .unwrap_or(0);
    u64::try_from(rounded).unwrap_or(u64::MAX)
}

/// The tick of the beat `at` at the first pass of its measure in `measures`, or `None` if
/// that measure is never played. Shared by [`Sequence::tick_of`] and
/// [`crate::Timeline::tick_of`], so the time readout and playback agree on where a beat is.
pub(crate) fn beat_tick(measures: &[MeasureSpan], song: &Song, at: BeatPosition) -> Option<u64> {
    let span = measures.iter().find(|s| s.measure == at.measure)?;
    let beats = song
        .track(at.track)
        .and_then(|t| t.measure(at.measure))
        .and_then(|m| m.voice(at.voice));
    let feel = song.feel(at.measure).unwrap_or_default();
    let measure_ticks = span.end.saturating_sub(span.start);
    let mut onset = Duration::new(0, 1).ok()?;
    for beat in beats.iter().flat_map(|v| v.beats().take(at.beat.get())) {
        if beat.kind() == BeatKind::Empty || beat.is_grace() {
            continue;
        }
        // A broken length counts as nothing: the start is still close.
        if let Some(next) = beat.duration().ok().and_then(|d| onset.checked_add(d).ok()) {
            onset = next;
        }
    }
    Some(
        span.start
            .saturating_add(swing::swing_offset(ticks(onset), measure_ticks, feel))
            .min(span.end),
    )
}
