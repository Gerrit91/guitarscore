//! Turns a song into a [`Sequence`]: timed MIDI events per track, a tempo map, and where
//! each played measure lies in time.
//!
//! Notes play on their track's channel. A note with a bend or slide borrows a free channel
//! for its own pitch bend (AGENTS.md section 5, decision 10): the borrowed channel gets the
//! track's program, volume and pan plus a 6-semitone bend range (wider for a wide slide),
//! and is released after the note. When every other channel is in use the note plays
//! without its pitch movement and [`Sequence::degraded_pitch_bends`] counts it.

use std::collections::HashMap;

use gscore_model::{
    BeatIndex, BeatKind, BeatPosition, BendKind, Duration, MeasureIndex, ModelError, NoteKind,
    SlideKind, Song, TimeSignature, TrackIndex, TrackSound,
};

use crate::channels::ChannelAllocator;
use crate::event::{Channel, MidiEvent, PITCH_BEND_CENTER, controller};
use crate::pitch;
use crate::sounding::{self, Sounding};
use crate::swing;
use crate::tempo::{PPQ, TempoMap};

/// Ticks in a whole note.
const WHOLE: u64 = 4 * PPQ as u64;
/// Dead (muted) notes sound this long at most: a 32nd note.
const DEAD_NOTE_TICKS: u64 = WHOLE / 32;
/// Dead notes are played softer, at this share of their velocity (percent).
const DEAD_NOTE_VELOCITY_PERCENT: u16 = 60;
/// A tied note continues the previous one if that ends within this many ticks of its start
/// (rounding of tuplets).
const TIE_TOLERANCE: u64 = 2;
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
    /// Tick after the last.
    pub end: u64,
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
        // When each played measure starts, by its time signature.
        let (measures, tick) = crate::timeline::measure_spans(song)?;

        let mut tempo_changes: Vec<(u64, u32)> = vec![(0, song.tempo())];
        for span in &measures {
            if let Some(bpm) = song.tempo_change(span.measure) {
                tempo_changes.push((span.start, bpm));
            }
        }

        let selected: Vec<TrackIndex> = match tracks {
            Some(tracks) => tracks.to_vec(),
            None => (0..song.track_count()).map(TrackIndex::new).collect(),
        };
        // Muted tracks do not play; with any soloed track, only soloed tracks play.
        let soloed = song.any_soloed();
        let selected: Vec<TrackIndex> = selected
            .into_iter()
            .filter(|&track| match song.track(track) {
                // A track that does not exist is reported by the loop below.
                None => true,
                Some(view) => !view.is_muted() && (!soloed || view.is_soloed()),
            })
            .collect();
        let mut channels = ChannelAllocator::default();
        let mut seq_tracks = Vec::with_capacity(selected.len());
        let mut events = Vec::new();
        let mut notes: Vec<Sounding> = Vec::new();
        let mut skipped_notes = 0_usize;

        for track_index in selected {
            let (Some(view), Some(sound)) =
                (song.track(track_index), song.played_sound(track_index))
            else {
                return Err(ModelError::Invalid(format!(
                    "the song has no {track_index}"
                )));
            };
            let channel = channels.allocate(sound.percussion);
            let number = seq_tracks.len();
            seq_tracks.push(SequenceTrack {
                track: track_index,
                name: view.name().to_owned(),
                channel,
                sound,
            });
            for event in sound_events(channel, sound) {
                events.push(TimedEvent {
                    tick: 0,
                    track: number,
                    event,
                });
            }

            // The note currently sounding on each string, to extend it on a tie.
            let mut on_string: HashMap<u8, usize> = HashMap::new();
            for span in &measures {
                let Some(measure) = view.measure(span.measure) else {
                    continue;
                };
                let feel = song.feel(span.measure).unwrap_or_default();
                let measure_ticks = span.end.saturating_sub(span.start);
                for voice in measure.voices() {
                    let mut onset = Duration::new(0, 1)?;
                    for beat in voice.beats() {
                        if beat.kind() == BeatKind::Empty {
                            continue;
                        }
                        let length = beat.duration()?;
                        let next = onset.checked_add(length)?;
                        let start = span.start.saturating_add(swing::swing_offset(
                            ticks(onset),
                            measure_ticks,
                            feel,
                        ));
                        let end = span.start.saturating_add(swing::swing_offset(
                            ticks(next),
                            measure_ticks,
                            feel,
                        ));
                        onset = next;
                        if let Some(bpm) = beat.tempo_change() {
                            tempo_changes.push((start, bpm));
                        }
                        if beat.kind() != BeatKind::Normal {
                            continue;
                        }
                        for note in beat.notes() {
                            let kind = note.kind();
                            let string = note.string().map(|s| s.get()).unwrap_or(0);
                            if kind == NoteKind::Tie {
                                let tied = on_string
                                    .get(&string)
                                    .and_then(|&i| notes.get_mut(i))
                                    .filter(|n| n.end.abs_diff(start) <= TIE_TOLERANCE);
                                if let Some(previous) = tied {
                                    previous.end = end;
                                    continue;
                                }
                            }
                            if !matches!(kind, NoteKind::Normal | NoteKind::Tie | NoteKind::Dead) {
                                continue;
                            }
                            let Ok(pitch) = view.sounding_pitch(&note) else {
                                skipped_notes = skipped_notes.saturating_add(1);
                                continue;
                            };
                            let (end, velocity) = if kind == NoteKind::Dead {
                                let soft =
                                    u16::from(note.velocity()) * DEAD_NOTE_VELOCITY_PERCENT / 100;
                                (
                                    end.min(start.saturating_add(DEAD_NOTE_TICKS)),
                                    u8::try_from(soft).unwrap_or(1).max(1),
                                )
                            } else {
                                (end, note.velocity())
                            };
                            // Drums have no pitch to bend or slide.
                            let bend = if sound.percussion {
                                None
                            } else {
                                note.bend()
                                    .filter(|bend| bend.quarter_tones != 0)
                                    .map(|bend| {
                                        (bend.quarter_tones, bend.kind == BendKind::Release)
                                    })
                            };
                            let slides: Vec<SlideKind> = if sound.percussion {
                                Vec::new()
                            } else {
                                note.slides().collect()
                            };
                            on_string.insert(string, notes.len());
                            notes.push(Sounding {
                                track: number,
                                channel,
                                play_channel: channel,
                                key: pitch.get(),
                                velocity,
                                start,
                                end,
                                string,
                                slide: sounding::choose_slide(&slides),
                                bend,
                                shape: None,
                            });
                        }
                    }
                }
            }
        }

        let free_channels = channels.free_channels();
        sounding::apply_slide_targets(&mut notes);
        sounding::apply_bends(&mut notes);
        let degraded_pitch_bends = sounding::assign_pitch_channels(&mut notes, free_channels);

        for note in &notes {
            let borrowed = note.play_channel != note.channel;
            if borrowed && let Some(track) = seq_tracks.get(note.track) {
                for event in sound_events(note.play_channel, track.sound) {
                    events.push(TimedEvent {
                        tick: note.start,
                        track: note.track,
                        event,
                    });
                }
                let range = note
                    .shape
                    .map_or(pitch::BEND_RANGE_SEMITONES, |shape| shape.range());
                for event in pitch::bend_range_setup(note.play_channel, range) {
                    events.push(TimedEvent {
                        tick: note.start,
                        track: note.track,
                        event,
                    });
                }
            }
            if borrowed && let Some(shape) = note.shape {
                for (tick, event) in shape.events(note.play_channel, note.start) {
                    events.push(TimedEvent {
                        tick,
                        track: note.track,
                        event,
                    });
                }
            }
            events.push(TimedEvent {
                tick: note.start,
                track: note.track,
                event: MidiEvent::NoteOn {
                    channel: note.play_channel,
                    key: note.key,
                    velocity: note.velocity,
                },
            });
            let end = note.end.max(note.start.saturating_add(1));
            events.push(TimedEvent {
                tick: end,
                track: note.track,
                event: MidiEvent::NoteOff {
                    channel: note.play_channel,
                    key: note.key,
                },
            });
            if borrowed && note.shape.is_some() {
                // Reset after the note, so a reused channel starts unbent.
                events.push(TimedEvent {
                    tick: end,
                    track: note.track,
                    event: MidiEvent::PitchBend {
                        channel: note.play_channel,
                        value: PITCH_BEND_CENTER,
                    },
                });
            }
        }
        if metronome {
            for span in &measures {
                events.extend(crate::click::click_events(*span));
            }
        }
        events.sort_by_key(|e| (e.tick, rank(&e.event)));

        tempo_changes.sort_by_key(|&(tick, _)| tick);
        let mut tempo = TempoMap::new(song.tempo());
        for (tick, bpm) in tempo_changes {
            tempo.set(tick, bpm);
        }

        Ok(Sequence {
            tracks: seq_tracks,
            events,
            tempo,
            measures,
            end: tick,
            skipped_notes,
            degraded_pitch_bends,
        })
    }

    /// The length of the whole sequence in microseconds.
    pub fn micros(&self) -> u64 {
        self.tempo.micros_at(self.end)
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
            if beat.kind() == BeatKind::Empty {
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
/// reverb and chorus send. Used for a sequence's setup and to change a sound while playing.
pub fn sound_events(channel: Channel, sound: TrackSound) -> Vec<MidiEvent> {
    let mut events = Vec::with_capacity(5);
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
fn rank(event: &MidiEvent) -> u8 {
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
        if beat.kind() == BeatKind::Empty {
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
