//! Grace notes in playback: quick notes just before their beat (an acciaccatura) or on it
//! (an appoggiatura, which delays the main note on its string).
//!
//! A grace note sounds for a fixed short time rather than its written value: a 32nd at a
//! fast tempo lasts about 40 ms, which a SoundFont's attack barely gets through. The
//! previous note on its string stops when it starts (see
//! [`crate::effects::one_note_per_string`]), as on a guitar, so it is not drowned out.

use std::collections::HashMap;

use gscore_model::{BeatView, Fret, NoteKind, TrackView};

use crate::event::Channel;
use crate::sounding::Sounding;
use crate::tempo::PPQ;

/// The shortest a grace note sounds: a 32nd.
const MIN_TICKS: u64 = PPQ as u64 / 8;
/// The longest a grace note sounds: a 16th.
const MAX_TICKS: u64 = PPQ as u64 / 4;
/// How long a grace note should sound, where the tempo allows it.
const TARGET_MILLIS: u64 = 75;
/// Loudness of a dead grace note against its velocity, in percent.
const DEAD_VELOCITY_PERCENT: u16 = 60;

/// A grace note before it is placed in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grace {
    pub(crate) key: u8,
    pub(crate) velocity: u8,
    /// The string; 0 when the note has no valid string.
    pub(crate) string: u8,
    pub(crate) on_beat: bool,
}

/// How long one grace note sounds at `bpm`: about [`TARGET_MILLIS`], between a 32nd and a
/// 16th.
pub(crate) fn length(bpm: u32) -> u64 {
    let ticks = TARGET_MILLIS
        .saturating_mul(u64::from(bpm))
        .saturating_mul(u64::from(PPQ))
        / 60_000;
    ticks.clamp(MIN_TICKS, MAX_TICKS)
}

/// The grace notes of `beat`: the notes of the grace beats before it (GP6/GP7), then the
/// grace notes attached to its own notes (GP3-5). Counts notes whose pitch cannot be
/// worked out in `skipped`.
pub(crate) fn gather(
    view: &TrackView<'_>,
    grace_beats: &[BeatView<'_>],
    beat: &BeatView<'_>,
    skipped: &mut usize,
) -> Vec<Grace> {
    let mut graces = Vec::new();
    for note in grace_beats.iter().flat_map(BeatView::notes) {
        match (view.sounding_pitch(&note), note.string()) {
            (Ok(pitch), Ok(string)) => graces.push(Grace {
                key: pitch.get(),
                velocity: soften(note.velocity(), note.kind() == NoteKind::Dead),
                string: string.get(),
                on_beat: note.grace().is_some_and(|grace| grace.on_beat),
            }),
            _ => *skipped = skipped.saturating_add(1),
        }
    }
    for note in beat.notes() {
        let Some(grace) = note.grace() else {
            continue;
        };
        let string = note.string().ok();
        let pitch = string
            .and_then(|string| {
                u8::try_from(grace.fret)
                    .ok()
                    .and_then(|fret| Fret::new(fret).ok())
                    .and_then(|fret| view.pitch_at(string, fret).ok())
            })
            .or_else(|| view.sounding_pitch(&note).ok());
        match pitch {
            Some(pitch) => graces.push(Grace {
                key: pitch.get(),
                velocity: soften(note.velocity(), grace.dead || grace.fret < 0),
                string: string.map_or(0, |string| string.get()),
                on_beat: grace.on_beat,
            }),
            None => *skipped = skipped.saturating_add(1),
        }
    }
    graces
}

fn soften(velocity: u8, dead: bool) -> u8 {
    let velocity = if dead {
        u16::from(velocity) * DEAD_VELOCITY_PERCENT / 100
    } else {
        u16::from(velocity)
    };
    u8::try_from(velocity).unwrap_or(u8::MAX).max(1)
}

/// The sounding notes of `graces` for a beat at `start`, each `length` long, and how far
/// the main note on each string moves later (behind the grace notes played on the beat).
/// Grace notes before the beat end at it; at the start of the song, where there is no time
/// before, they are played on the beat too.
pub(crate) fn place(
    graces: &[Grace],
    start: u64,
    length: u64,
    track: usize,
    channel: Channel,
) -> (Vec<Sounding>, HashMap<u8, u64>) {
    let before_count = graces.iter().filter(|grace| !grace.on_beat).count();
    let span = u64::try_from(before_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(length);
    let room = start.checked_sub(span);
    let mut placed = Vec::with_capacity(graces.len());
    let mut on_beat: Vec<&Grace> = Vec::new();
    match room {
        Some(mut at) => {
            for grace in graces.iter().filter(|grace| !grace.on_beat) {
                placed.push(sounding(grace, at, length, start, track, channel));
                at = at.saturating_add(length);
            }
            on_beat.extend(graces.iter().filter(|grace| grace.on_beat));
        }
        None => on_beat.extend(graces),
    }
    let mut delays: HashMap<u8, u64> = HashMap::new();
    let mut at = start;
    for grace in on_beat {
        placed.push(sounding(grace, at, length, start, track, channel));
        at = at.saturating_add(length);
        delays.insert(grace.string, at.saturating_sub(start));
    }
    (placed, delays)
}

fn sounding(
    grace: &Grace,
    start: u64,
    length: u64,
    onset: u64,
    track: usize,
    channel: Channel,
) -> Sounding {
    Sounding {
        track,
        channel,
        play_channel: channel,
        key: grace.key,
        velocity: grace.velocity,
        start,
        end: start.saturating_add(length.max(1)),
        string: grace.string,
        slide: None,
        bend: None,
        shape: None,
        vibrato: None,
        let_ring: false,
        tremolo: false,
        // Grace notes belong to their beat: an excerpt of the beat plays them too.
        onset,
        damp: false,
        bend_curve: None,
        whammy: None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use super::*;

    fn grace(string: u8, on_beat: bool) -> Grace {
        Grace {
            key: 60,
            velocity: 90,
            string,
            on_beat,
        }
    }

    #[test]
    fn a_grace_note_lasts_about_the_same_time_at_any_tempo() {
        assert_eq!(length(60), MIN_TICKS, "slow: a 32nd is long enough");
        let fast = length(170);
        assert!(fast > MIN_TICKS && fast <= MAX_TICKS, "{fast}");
        assert_eq!(length(400), MAX_TICKS, "never longer than a 16th");
    }

    #[test]
    fn graces_before_the_beat_end_at_it_in_order() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(1, false), grace(2, false)], 960, 100, 0, channel);
        let spans: Vec<(u64, u64)> = notes.iter().map(|n| (n.start, n.end)).collect();
        assert_eq!(spans, [(760, 860), (860, 960)]);
        assert!(delays.is_empty());
        assert!(
            notes.iter().all(|n| n.onset == 960),
            "they belong to their beat"
        );
    }

    #[test]
    fn an_appoggiatura_takes_the_beat_and_delays_its_string() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(3, true)], 960, 100, 0, channel);
        assert_eq!((notes[0].start, notes[0].end), (960, 1060));
        assert_eq!(delays.get(&3), Some(&100));
    }

    #[test]
    fn at_the_song_start_graces_move_onto_the_beat() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(1, false)], 0, 100, 0, channel);
        assert_eq!((notes[0].start, notes[0].end), (0, 100));
        assert_eq!(delays.get(&1), Some(&100), "the main note follows");
    }
}
