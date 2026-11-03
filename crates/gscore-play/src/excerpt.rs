//! A one-beat excerpt of a song: the notes of one beat (all voices of its track that
//! start there) with every effect, as the song plays them, moved to tick 0. The app plays
//! it to let a palette click be heard.

use gscore_model::{BeatPosition, ModelError, Song};

use crate::build;
use crate::sequence::{Sequence, rank};
use crate::tempo::{PPQ, TempoMap};

/// The excerpt never lasts longer than this (a let-ring or long note is cut): two whole
/// notes.
const MAX_TICKS: u64 = 8 * PPQ as u64;

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
        let collected = build::collect(song, &[at.track], &measures, Some(&keep))?;
        if collected.notes.is_empty() {
            return Ok(None);
        }
        let mut notes = collected.notes;
        for note in &mut notes {
            note.start = note.start.saturating_sub(start);
            note.end = note.end.saturating_sub(start).min(MAX_TICKS);
        }
        let mut events = collected.setup;
        build::emit(&notes, &collected.tracks, &mut events);
        events.retain(|event| event.tick <= MAX_TICKS);
        events.sort_by_key(|event| (event.tick, rank(&event.event)));
        let end = events.last().map_or(0, |event| event.tick);
        let bpm = crate::timeline::tempo_at(song, at.measure);
        Ok(Some(Sequence {
            tracks: collected.tracks,
            events,
            tempo: TempoMap::new(bpm),
            measures: Vec::new(),
            end,
            skipped_notes: collected.skipped_notes,
            degraded_pitch_bends: collected.degraded_pitch_bends,
        }))
    }
}
