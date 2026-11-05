//! Metronome clicks: one click per beat of a played measure, and the count-in bar before
//! playback starts. Clicks go to the General MIDI percussion channel (10), so the internal
//! synthesizer and every external synthesizer have a sound for them without a SoundFont.

use gscore_model::{BeatPosition, ModelError, Song, TimeSignature, TrackIndex};

use crate::event::{Channel, MidiEvent};
use crate::sequence::{MeasureSpan, Sequence, TimedEvent, ticks};
use crate::tempo::{PPQ, TempoMap};
use crate::timeline::tempo_at;

/// The `track` field of a click event: clicks belong to no track (SMF export leaves them
/// out of the track chunks).
pub(crate) const CLICK_TRACK: usize = usize::MAX;
/// General MIDI keys: the high wood block on the first beat, the low one elsewhere.
const ACCENT_KEY: u8 = 76;
const CLICK_KEY: u8 = 77;
/// How hard the clicks are played.
const ACCENT_VELOCITY: u8 = 110;
const CLICK_VELOCITY: u8 = 80;
/// How long a click sounds, in ticks (an eighth of a quarter note).
const CLICK_LENGTH: u64 = (PPQ as u64) / 8;

/// The clicks of one played measure: one per beat of its time signature, the first accented.
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

/// One measure of clicks before playback starts at `at`, using the tempo and time signature
/// of its measure.
pub fn count_in(song: &Song, at: BeatPosition) -> Result<Sequence, ModelError> {
    let four_four = TimeSignature::new(4, 4)?;
    let signature = song
        .track(TrackIndex::new(0))
        .and_then(|track| track.measure(at.measure))
        .map(|measure| measure.time_signature())
        .transpose()?
        .unwrap_or(four_four);
    let end = ticks(signature.measure_length());
    let span = MeasureSpan {
        measure: at.measure,
        start: 0,
        end,
        signature,
    };
    let mut events = click_events(span);
    events.sort_by_key(|event| event.tick);
    Ok(Sequence {
        tracks: Vec::new(),
        events,
        tempo: TempoMap::new(tempo_at(song, at.measure)),
        measures: Vec::new(),
        end,
        skipped_notes: 0,
        degraded_pitch_bends: 0,
    })
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
        assert_eq!(clicks, 16, "four clicks and their offs in each of two bars");
        assert_eq!(clicked.events.len(), plain.events.len() + clicks);
    }

    #[test]
    fn the_count_in_is_one_bar_at_the_songs_tempo() {
        let song = testing::song(&[GUITAR_6], 1).unwrap();
        let at = BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        };
        let count = count_in(&song, at).unwrap();
        assert_eq!(count.end, 4 * u64::from(PPQ));
        assert_eq!(count.micros(), 2_000_000);
        assert_eq!(count.events.len(), 8);
    }
}
