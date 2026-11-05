//! When each played measure lies in time, and the song's tempo map, without building the
//! events: for showing the position and the total length in the app.

use gscore_model::{
    BeatKind, BeatPosition, Duration, MeasureIndex, ModelError, Song, TimeSignature, TrackIndex,
};

use crate::order::playback_order;
use crate::sequence::{MeasureSpan, beat_tick, ticks};
use crate::swing;
use crate::tempo::{PPQ, TempoMap};

/// The played measures of `song` and the tick where the piece ends. The same measure can
/// appear several times (repeats).
pub(crate) fn measure_spans(song: &Song) -> Result<(Vec<MeasureSpan>, u64), ModelError> {
    let order = playback_order(song);
    let four_four = TimeSignature::new(4, 4)?;
    let mut measures = Vec::with_capacity(order.len());
    let mut tick: u64 = 0;
    for &measure in &order {
        let signature = song
            .track(TrackIndex::new(0))
            .and_then(|track| track.measure(measure))
            .map(|measure| measure.time_signature())
            .transpose()?
            .unwrap_or(four_four);
        let end = tick.saturating_add(ticks(signature.measure_length()));
        let pause = song.fermata(measure).map_or(0, fermata_hold);
        measures.push(MeasureSpan {
            measure,
            start: tick,
            end,
            pause,
            signature,
        });
        // A fermata holds the end of its measure in real time; it does not move any tick.
        tick = end;
    }
    Ok((measures, tick))
}

/// How long a fermata holds, in ticks: a quarter note for the ordinary one.
fn fermata_hold(fermata: gscore_model::Fermata) -> u64 {
    let (numerator, denominator) = fermata.hold_quarters();
    u64::from(PPQ)
        .saturating_mul(u64::from(numerator))
        .checked_div(u64::from(denominator.max(1)))
        .unwrap_or(0)
}

/// A song's played measures and tempo, without its events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    /// The played measures, in playing order.
    pub measures: Vec<MeasureSpan>,
    /// Tempo changes, including ones on a beat.
    pub tempo: TempoMap,
    /// The tick where the piece ends.
    pub end: u64,
}

impl Timeline {
    /// The timeline of `song`.
    pub fn new(song: &Song) -> Result<Self, ModelError> {
        let (measures, end) = measure_spans(song)?;
        let mut changes: Vec<(u64, u32)> = vec![(0, song.tempo())];
        for span in &measures {
            if let Some(bpm) = song.tempo_change(span.measure) {
                changes.push((span.start, bpm));
            }
            // A tempo change on a beat (as the sequence also honours).
            let feel = song.feel(span.measure).unwrap_or_default();
            let measure_ticks = span.end.saturating_sub(span.start);
            for track in song.tracks() {
                let Some(measure) = track.measure(span.measure) else {
                    continue;
                };
                for voice in measure.voices() {
                    let mut onset = Duration::new(0, 1)?;
                    for beat in voice.beats() {
                        if beat.kind() == BeatKind::Empty || beat.is_grace() {
                            continue;
                        }
                        let length = beat.duration()?;
                        let start = span.start.saturating_add(swing::swing_offset(
                            ticks(onset),
                            measure_ticks,
                            feel,
                        ));
                        onset = onset.checked_add(length)?;
                        if let Some(bpm) = beat.tempo_change() {
                            changes.push((start, bpm));
                        }
                    }
                }
            }
        }
        changes.sort_by_key(|&(tick, _)| tick);
        let mut tempo = TempoMap::new(song.tempo());
        for (tick, bpm) in changes {
            tempo.set(tick, bpm);
        }
        for span in &measures {
            tempo.hold_ticks(span.end, span.pause);
        }
        Ok(Timeline {
            measures,
            tempo,
            end,
        })
    }

    /// The length of the whole song in microseconds.
    pub fn micros(&self) -> u64 {
        self.tempo.micros_at(self.end)
    }

    /// The time of `tick` in microseconds from the start.
    pub fn micros_at(&self, tick: u64) -> u64 {
        self.tempo.micros_at(tick)
    }

    /// The tick of the beat `at` at the first pass of its measure (where playing from that
    /// beat starts), or `None` if the measure is never played.
    pub fn tick_of(&self, song: &Song, at: BeatPosition) -> Option<u64> {
        beat_tick(&self.measures, song, at)
    }

    /// The tempo in effect at `tick`, in beats per minute, including changes on a beat.
    pub fn bpm_at(&self, tick: u64) -> u32 {
        self.tempo.bpm_at(tick)
    }

    /// The first and last tick of the measures `first` to `last` (0-based, inclusive), or
    /// `None` if one of them is never played. Repeats are taken into account: the range
    /// starts at the first pass of `first` and ends at the last pass of `last`.
    pub fn measure_range(&self, first: usize, last: usize) -> Option<(u64, u64)> {
        if last < first {
            return None;
        }
        let start = self
            .measures
            .iter()
            .find(|span| span.measure.get() == first)?
            .start;
        let end = self
            .measures
            .iter()
            .rev()
            .find(|span| span.measure.get() == last)?
            .end;
        (end >= start).then_some((start, end))
    }

    /// The length of the measures `first` to `last` in microseconds, or `None` if one of
    /// them is never played.
    pub fn measure_micros(&self, first: usize, last: usize) -> Option<u64> {
        let (start, end) = self.measure_range(first, last)?;
        Some(
            self.tempo
                .micros_at(end)
                .saturating_sub(self.tempo.micros_at(start)),
        )
    }
}

/// The tempo in effect at `measure`: its own change, the last one before it, or the
/// song's initial tempo.
pub fn tempo_at(song: &Song, measure: MeasureIndex) -> u32 {
    (0..=measure.get())
        .rev()
        .find_map(|index| song.tempo_change(MeasureIndex::new(index)))
        .unwrap_or_else(|| song.tempo())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use gscore_model::testing::{self, GUITAR_6};

    use super::*;
    use crate::PPQ;

    #[test]
    fn the_total_time_follows_the_tempo_and_the_bars() {
        let mut song = testing::song(&[GUITAR_6], 4).unwrap();
        // 4/4 at 120 BPM: a bar is 2 seconds.
        let timeline = Timeline::new(&song).unwrap();
        assert_eq!(timeline.measures.len(), 4);
        assert_eq!(timeline.micros(), 8_000_000);

        // Halving the tempo doubles every bar's length.
        let _change = song.set_tempo(60).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        assert_eq!(timeline.micros(), 16_000_000);
    }

    #[test]
    fn a_measure_range_is_measured_in_order() {
        let song = testing::song(&[GUITAR_6], 4).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        assert_eq!(timeline.measure_micros(0, 0), Some(2_000_000));
        assert_eq!(timeline.measure_micros(1, 2), Some(4_000_000));
        assert_eq!(timeline.measure_micros(2, 1), None);
        assert_eq!(timeline.measure_micros(0, 9), None);
    }

    #[test]
    fn a_fermata_holds_the_end_of_its_measure() {
        // Two 4/4 bars at 120 BPM, the first with an ordinary fermata (one quarter added).
        let song = testing::song_with_fermatas(2, &[(0, gscore_model::Fermata::Medium)]).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        let bar = u64::from(PPQ).saturating_mul(4);
        assert_eq!(timeline.measures[0].pause, u64::from(PPQ));
        // The pause does not move any tick: only the time taken to reach them grows.
        assert_eq!(timeline.measures[1].start, bar);
        assert_eq!(timeline.measures[1].end, bar.saturating_mul(2));
        assert_eq!(timeline.end, bar.saturating_mul(2));
        // Two 2-second bars plus a half-second hold.
        assert_eq!(timeline.micros(), 4_500_000);
        assert_eq!(timeline.measure_range(0, 0), Some((0, bar)));
        assert_eq!(timeline.measure_micros(1, 1), Some(2_000_000));
        // The first bar's time includes its hold (the measure's last beat is held).
        assert_eq!(timeline.measure_micros(0, 0), Some(2_500_000));

        let sequence = crate::Sequence::new(&song, None).unwrap();
        assert_eq!(sequence.measures[1].start, bar);
        assert_eq!(sequence.micros(), timeline.micros());
        // A very short fermata holds a sixteenth of a quarter.
        let song =
            testing::song_with_fermatas(1, &[(0, gscore_model::Fermata::VeryShort)]).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        assert_eq!(timeline.measures[0].pause, u64::from(PPQ) / 4);
    }

    #[test]
    fn a_beat_lies_where_the_sequence_plays_it_and_the_tempo_follows_changes() {
        let mut song = testing::song(&[GUITAR_6], 4).unwrap();
        let _change = song.set_tempo(90).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        let sequence = crate::Sequence::new(&song, None).unwrap();
        for measure in 0..4 {
            let at = BeatPosition {
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(measure),
                voice: gscore_model::VoiceIndex::new(0),
                beat: gscore_model::BeatIndex::new(0),
            };
            assert_eq!(timeline.tick_of(&song, at), sequence.tick_of(&song, at));
        }
        assert_eq!(timeline.bpm_at(0), 90);
        assert_eq!(timeline.bpm_at(timeline.end), 90);
    }
}
