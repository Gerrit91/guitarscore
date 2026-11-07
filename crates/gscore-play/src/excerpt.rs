//! A one-beat excerpt of a song: the notes of one beat (all voices of its track that
//! start there, and their grace notes) with every effect, as the song plays them, moved to tick 0. The app plays
//! it to let a palette click be heard.

use std::sync::Arc;

use gscore_model::{BeatPosition, ModelError, Song};

use crate::build;
use crate::event::{Channel, MidiEvent, controller};
use crate::performance::Performance;
use crate::sequence::TimedEvent;
use crate::sequence::{Sequence, rank};
use crate::tempo::{PPQ, TempoMap};

/// The excerpt never lasts longer than this (a let-ring or long note is cut): two whole
/// notes.
const MAX_TICKS: u64 = 8 * PPQ as u64;
/// How long the excerpt runs on after its last note-off, for the release: a whole note.
const RELEASE_TAIL: u64 = 4 * PPQ as u64;

impl Sequence {
    /// The beat at `at` alone, with its effects, from tick 0 at the tempo it is played at.
    /// `None` when the beat is not played (a muted track, a rest, a measure no pass
    /// reaches) or has no sounding note.
    pub fn beat_excerpt(song: &Song, at: BeatPosition) -> Result<Option<Sequence>, ModelError> {
        let Some(track) = song.track(at.track) else {
            return Ok(None);
        };
        if track.is_muted() {
            return Ok(None);
        }
        let (measures, _) = crate::timeline::measure_spans(song)?;
        let Some(start) = crate::sequence::beat_tick(&measures, song, at) else {
            return Ok(None);
        };
        // The beat's notes at the first pass of its measure (later passes repeat them).
        let keep = |note: &crate::sounding::Sounding| note.onset == start;
        let collected = build::collect(song, &[at.track], &measures, Some(&keep), Some(start))?;
        if collected.notes.is_empty() {
            return Ok(None);
        }
        let mut notes = collected.notes;
        // Grace notes before the beat belong to it: the excerpt starts with the first.
        let origin = notes.iter().map(|note| note.start).fold(start, u64::min);
        for note in &mut notes {
            note.start = note.start.saturating_sub(origin);
            note.end = note.end.saturating_sub(origin).min(MAX_TICKS);
            note.onset = note.onset.saturating_sub(origin);
        }
        let bpm = crate::timeline::tempo_at(song, at.measure);
        let mut performance = Performance {
            tracks: collected.tracks,
            notes,
            tempo: TempoMap::new(bpm),
            measures: Vec::new(),
            end: 0,
            skipped_notes: collected.skipped_notes,
            by_start: Vec::new(),
        };
        performance.index_by_start();
        let midi = crate::midi::derive(&performance);
        let mut events = midi.events;
        events.retain(|event| event.tick <= MAX_TICKS);
        // Run on after the last note-off while its release fades. The player centers every
        // bend when the sequence ends, so a bent release still ringing is faded out (its
        // borrowed channel's expression to 0) just before, instead of pulled back down.
        let end = events
            .iter()
            .map(|event| event.tick)
            .max()
            .unwrap_or(0)
            .saturating_add(RELEASE_TAIL);
        let base: Vec<Channel> = midi.tracks.iter().map(|track| track.channel).collect();
        let mut borrowed: Vec<Channel> = midi
            .channels
            .iter()
            .copied()
            .filter(|channel| !base.contains(channel))
            .collect();
        borrowed.sort_by_key(|channel| channel.get());
        borrowed.dedup();
        for channel in borrowed {
            for (tick, controller) in [
                (end.saturating_sub(1), controller::EXPRESSION),
                (end, controller::ALL_SOUND_OFF),
            ] {
                events.push(TimedEvent {
                    tick,
                    track: 0,
                    event: MidiEvent::Control {
                        channel,
                        controller,
                        value: 0,
                    },
                });
            }
        }
        events.sort_by_key(|event| (event.tick, rank(&event.event)));
        performance.end = end;
        Ok(Some(Sequence {
            tracks: midi.tracks,
            events,
            tempo: performance.tempo.clone(),
            measures: Vec::new(),
            end,
            skipped_notes: performance.skipped_notes,
            degraded_pitch_bends: midi.degraded_pitch_bends,
            performance: Arc::new(performance),
        }))
    }
}
