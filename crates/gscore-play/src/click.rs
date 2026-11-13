//! Metronome clicks: one click per beat of a played measure, and the count-in bar before
//! playback starts. Clicks go to the General MIDI percussion channel (10), so the internal
//! synthesizer and every external synthesizer have a sound for them without a SoundFont.

use std::sync::Arc;

use gscore_model::{BeatPosition, ModelError, Song, TimeSignature, TrackIndex};

use crate::event::{Channel, MidiEvent};
use crate::performance::{Performance, PlayedNote};
use crate::sequence::{MeasureSpan, Sequence, TimedEvent, ticks};
use crate::tempo::{PPQ, TempoMap};
use crate::timeline::tempo_at;

/// The `track` field of a click event: clicks belong to no track (SMF export leaves them
/// out of the track chunks).
pub const CLICK_TRACK: usize = usize::MAX;
/// General MIDI keys: Metronome Bell on the first beat, Metronome Click elsewhere. The
/// built-in synthesizer plays its own click for them (whatever the SoundFont has there);
/// an external synthesizer plays its metronome sounds.
const ACCENT_KEY: u8 = 34;
const CLICK_KEY: u8 = 33;
/// How hard the clicks are played.
const ACCENT_VELOCITY: u8 = 110;
const CLICK_VELOCITY: u8 = 80;
/// How long a click sounds, in ticks (an eighth of a quarter note).
const CLICK_LENGTH: u64 = (PPQ as u64) / 8;
/// The controller on the percussion channel that chooses the click's sound (3 is
/// undefined in General MIDI, so other synthesizers ignore it).
pub const CLICK_SOUND_CONTROLLER: u8 = 3;
/// The most count-in bars.
pub const MAX_COUNT_IN_BARS: u8 = 4;

/// How the built-in synthesizer's metronome sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClickSound {
    /// A short wooden "tock", like a wood block.
    #[default]
    Wood,
    /// A clean electronic beep.
    Beep,
    /// A mellow cowbell.
    Cowbell,
}

impl ClickSound {
    /// Every sound, in menu order.
    pub const ALL: [ClickSound; 3] = [ClickSound::Wood, ClickSound::Beep, ClickSound::Cowbell];

    /// The id the settings store.
    pub fn id(self) -> &'static str {
        match self {
            ClickSound::Wood => "wood",
            ClickSound::Beep => "beep",
            ClickSound::Cowbell => "cowbell",
        }
    }

    /// The sound named by `id`.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sound| sound.id() == id)
    }

    /// A name for menus.
    pub fn label(self) -> &'static str {
        match self {
            ClickSound::Wood => "Wood block",
            ClickSound::Beep => "Electronic beep",
            ClickSound::Cowbell => "Cowbell",
        }
    }

    /// The value of [`CLICK_SOUND_CONTROLLER`] that chooses it.
    pub fn value(self) -> u8 {
        match self {
            ClickSound::Wood => 0,
            ClickSound::Beep => 1,
            ClickSound::Cowbell => 2,
        }
    }

    /// The sound a [`CLICK_SOUND_CONTROLLER`] value chooses (unknown values: the wood).
    pub fn of_value(value: u8) -> Self {
        Self::ALL
            .into_iter()
            .find(|sound| sound.value() == value)
            .unwrap_or_default()
    }
}

/// How the metronome clicks: its volume (0 to 1; 0 is silent) and sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Click {
    /// The volume, 0 to 1.
    pub volume: f32,
    /// The built-in synthesizer's sound.
    pub sound: ClickSound,
}

impl Default for Click {
    fn default() -> Self {
        Click {
            volume: 1.0,
            sound: ClickSound::Wood,
        }
    }
}

/// The event that chooses `sound` for the clicks after it, at `tick`.
pub(crate) fn sound_event(sound: ClickSound, tick: u64) -> TimedEvent {
    TimedEvent {
        tick,
        track: CLICK_TRACK,
        event: MidiEvent::Control {
            channel: Channel::PERCUSSION,
            controller: CLICK_SOUND_CONTROLLER,
            value: sound.value(),
        },
    }
}

/// The clicks of one played measure: one per beat of its time signature, the first accented.
#[cfg(test)]
pub(crate) fn click_events(span: MeasureSpan) -> Vec<TimedEvent> {
    click_events_at(span, 1.0)
}

/// The clicks of one measure at `volume` (0 to 1, scaling their velocity); 0 is silent.
pub(crate) fn click_events_at(span: MeasureSpan, volume: f32) -> Vec<TimedEvent> {
    if !(volume.is_finite() && volume > 0.0) {
        return Vec::new();
    }
    let scale = volume.min(1.0);
    let velocity = |base: u8| {
        let scaled = f32::from(base) * scale;
        u8::try_from(scaled.round().clamp(0.0, 127.0) as u32).unwrap_or(base)
    };
    let whole = u64::from(PPQ).saturating_mul(4);
    let beat = whole
        .checked_div(u64::from(span.signature.denominator()))
        .unwrap_or(0);
    let count = u64::from(span.signature.numerator());
    let mut events = Vec::with_capacity(usize::try_from(count.saturating_mul(2)).unwrap_or(0));
    for index in 0..count {
        let tick = span.start.saturating_add(beat.saturating_mul(index));
        let (key, velocity) = if index == 0 {
            (ACCENT_KEY, velocity(ACCENT_VELOCITY))
        } else {
            (CLICK_KEY, velocity(CLICK_VELOCITY))
        };
        events.push(TimedEvent {
            tick,
            track: CLICK_TRACK,
            event: MidiEvent::NoteOn {
                channel: Channel::PERCUSSION,
                key,
                velocity,
            },
        });
        events.push(TimedEvent {
            tick: tick.saturating_add(CLICK_LENGTH),
            track: CLICK_TRACK,
            event: MidiEvent::NoteOff {
                channel: Channel::PERCUSSION,
                key,
            },
        });
    }
    events
}

/// `bars` measures of clicks (1 to [`MAX_COUNT_IN_BARS`]) before playback starts at `at`,
/// using the tempo and time signature of its measure, as `click` says. It clicks whether or
/// not the metronome clicks during the song.
pub fn count_in(
    song: &Song,
    at: BeatPosition,
    bars: u8,
    click: Click,
) -> Result<Sequence, ModelError> {
    let four_four = TimeSignature::new(4, 4)?;
    let signature = song
        .track(TrackIndex::new(0))
        .and_then(|track| track.measure(at.measure))
        .map(|measure| measure.time_signature())
        .transpose()?
        .unwrap_or(four_four);
    let bar = ticks(signature.measure_length());
    let bars = u64::from(bars.clamp(1, MAX_COUNT_IN_BARS));
    let end = bar.saturating_mul(bars);
    let mut events = vec![sound_event(click.sound, 0)];
    for index in 0..bars {
        let start = bar.saturating_mul(index);
        let span = MeasureSpan {
            measure: at.measure,
            start,
            end: start.saturating_add(bar),
            pause: 0,
            signature,
        };
        events.extend(click_events_at(span, click.volume));
    }
    events.sort_by_key(|event| (event.tick, event.event.is_note()));
    Ok(Sequence {
        tracks: Vec::new(),
        events,
        tempo: TempoMap::new(tempo_at(song, at.measure)),
        measures: Vec::new(),
        end,
        skipped_notes: 0,
        degraded_pitch_bends: 0,
        performance: Arc::default(),
    })
}

/// The count-in `clicks` and the song's `sequence` from `start` (a tick of it) as one
/// sequence, so the song follows the last click without a gap. The song's setup
/// (programs, volumes) comes first; its events from `start` on follow the clicks, moved by
/// the clicks' length minus `start` (a tick `t >= start` of the song plays at
/// `t - start + clicks.end`). The clicks keep their own tempo, then the song's tempo at
/// `start` and its later changes take over.
pub fn lead_in(sequence: &Sequence, clicks: &Sequence, start: u64) -> Sequence {
    let bar = clicks.end;
    let start = start.min(sequence.end);
    let moved = |tick: u64| tick.saturating_sub(start).saturating_add(bar);
    let is_setup = |event: &TimedEvent| event.tick == 0 && !event.event.is_note();
    let mut events: Vec<TimedEvent> = sequence
        .events
        .iter()
        .filter(|event| is_setup(event))
        .copied()
        .collect();
    events.extend(
        clicks
            .events
            .iter()
            .filter(|event| event.tick < bar)
            .copied(),
    );
    events.extend(
        sequence
            .events
            .iter()
            .filter(|event| event.tick >= start && !is_setup(event))
            .map(|event| TimedEvent {
                tick: moved(event.tick),
                ..*event
            }),
    );
    // The clicks' tempo, then the song's at `start` and after it.
    let song_points = sequence.tempo.points();
    let at_start = song_points
        .iter()
        .take_while(|(tick, _)| *tick <= start)
        .last()
        .or_else(|| song_points.first())
        .map(|&(_, rate)| rate);
    let mut points: Vec<(u64, u32)> = clicks.tempo.points().first().copied().into_iter().collect();
    points.extend(at_start.map(|rate| (bar, rate)));
    points.extend(
        song_points
            .iter()
            .filter(|(tick, _)| *tick > start)
            .map(|&(tick, rate)| (moved(tick), rate)),
    );
    let pauses: Vec<(u64, u64)> = sequence
        .tempo
        .pauses()
        .iter()
        .filter(|&&(tick, _)| tick > start)
        .map(|&(tick, micros)| (moved(tick), micros))
        .collect();
    let tempo = TempoMap::from_parts(points, pauses).unwrap_or_else(|| clicks.tempo.clone());
    let measures: Vec<MeasureSpan> = sequence
        .measures
        .iter()
        .filter(|span| span.start >= start)
        .map(|span| MeasureSpan {
            start: moved(span.start),
            end: moved(span.end),
            ..*span
        })
        .collect();
    let end = moved(sequence.end);
    // The notes the song plays from `start` on, moved like its events.
    let source = &sequence.performance;
    let mut performance = Performance {
        tracks: source.tracks.clone(),
        notes: source
            .notes
            .iter()
            .filter(|note| note.start >= start)
            .map(|note| PlayedNote {
                start: moved(note.start),
                end: moved(note.end),
                onset: moved(note.onset.max(start)),
                ..*note
            })
            .collect(),
        tempo: tempo.clone(),
        measures: measures.clone(),
        end,
        skipped_notes: source.skipped_notes,
        by_start: Vec::new(),
    };
    performance.index_by_start();
    Sequence {
        tracks: sequence.tracks.clone(),
        events,
        tempo,
        measures,
        end,
        skipped_notes: sequence.skipped_notes,
        degraded_pitch_bends: sequence.degraded_pitch_bends,
        performance: Arc::new(performance),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use gscore_model::testing::{self, GUITAR_6};
    use gscore_model::{BeatIndex, MeasureIndex, VoiceIndex};

    use super::*;

    #[test]
    fn every_beat_gets_an_accented_or_plain_click() {
        let (measures, _) =
            crate::timeline::measure_spans(&testing::song(&[GUITAR_6], 1).unwrap()).unwrap();
        let events = click_events(measures[0]);
        assert_eq!(events.len(), 8, "four clicks, each with an off");
        assert!(matches!(
            events[0].event,
            MidiEvent::NoteOn {
                channel: Channel::PERCUSSION,
                key: ACCENT_KEY,
                ..
            }
        ));
        assert_eq!(events[0].tick, 0);
        assert_eq!(events[2].tick, u64::from(PPQ));
        assert!(matches!(
            events[2].event,
            MidiEvent::NoteOn { key: CLICK_KEY, .. }
        ));
    }

    #[test]
    fn the_click_volume_scales_the_velocities_and_zero_is_silent() {
        let (measures, _) =
            crate::timeline::measure_spans(&testing::song(&[GUITAR_6], 1).unwrap()).unwrap();
        let full = click_events(measures[0]);
        let quiet = click_events_at(measures[0], 0.5);
        assert_eq!(quiet.len(), full.len());
        for (loud, soft) in full.iter().zip(&quiet) {
            if let (MidiEvent::NoteOn { velocity: a, .. }, MidiEvent::NoteOn { velocity: b, .. }) =
                (loud.event, soft.event)
            {
                assert!(b < a && b > 0);
            }
        }
        assert!(click_events_at(measures[0], 0.0).is_empty());
    }

    #[test]
    fn the_metronome_adds_a_click_track_to_the_sequence() {
        let song = testing::song(&[GUITAR_6], 2).unwrap();
        let plain = Sequence::new(&song, None).unwrap();
        let clicked = Sequence::with_metronome(&song, None, true).unwrap();
        let clicks = clicked
            .events
            .iter()
            .filter(|event| event.track == CLICK_TRACK)
            .count();
        assert_eq!(
            clicks, 17,
            "the sound, then four clicks and their offs in each of two bars"
        );
        assert_eq!(clicked.events.len(), plain.events.len() + clicks);
    }

    #[test]
    fn the_song_follows_its_count_in_without_a_gap() {
        let song = testing::song(&[GUITAR_6], 3).unwrap();
        let at = |measure: usize| BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(measure),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        };
        let sequence = Sequence::new(&song, None).unwrap();
        let clicks = count_in(&song, at(1), 1, Click::default()).unwrap();
        let bar = clicks.end;
        let start = sequence.tick_of(&song, at(1)).unwrap();
        let joined = lead_in(&sequence, &clicks, start);
        // The clicks fill the first bar; the song's second bar starts right after it.
        let notes = |sequence: &Sequence, track: usize| -> Vec<u64> {
            sequence
                .events
                .iter()
                .filter(|event| {
                    event.track == track && matches!(event.event, MidiEvent::NoteOn { .. })
                })
                .map(|event| event.tick)
                .collect()
        };
        assert!(notes(&joined, CLICK_TRACK).iter().all(|&tick| tick < bar));
        let from_start: Vec<u64> = notes(&sequence, 0)
            .into_iter()
            .filter(|&tick| tick >= start)
            .map(|tick| tick - start + bar)
            .collect();
        assert_eq!(notes(&joined, 0), from_start);
        assert_eq!(joined.end, sequence.end - start + bar);
        // The song's setup still comes first.
        assert!(
            joined
                .events
                .iter()
                .take_while(|e| e.tick == 0)
                .any(|e| !e.event.is_note())
        );
        // One continuous clock: the song starts exactly one bar of clicks in.
        assert_eq!(joined.tempo.micros_at(bar), clicks.micros());
    }

    #[test]
    fn the_count_in_is_one_bar_at_the_songs_tempo() {
        let song = testing::song(&[GUITAR_6], 1).unwrap();
        let at = BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        };
        let count = count_in(&song, at, 1, Click::default()).unwrap();
        assert_eq!(count.end, 4 * u64::from(PPQ));
        assert_eq!(count.micros(), 2_000_000);
        // The sound first, then four clicks and their offs.
        assert_eq!(count.events.len(), 9);
        assert!(matches!(
            count.events[0].event,
            MidiEvent::Control {
                controller: CLICK_SOUND_CONTROLLER,
                ..
            }
        ));
        // Two bars are twice as long, with twice the clicks, in the chosen sound.
        let beep = Click {
            volume: 1.0,
            sound: ClickSound::Beep,
        };
        let two = count_in(&song, at, 2, beep).unwrap();
        assert_eq!(two.end, 8 * u64::from(PPQ));
        assert_eq!(two.events.len(), 17);
        assert!(matches!(
            two.events[0].event,
            MidiEvent::Control { value: 1, .. }
        ));
    }
}
