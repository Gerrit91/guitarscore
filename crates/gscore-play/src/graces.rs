//! Grace notes in playback: quick notes just before their beat (an acciaccatura) or on it
//! (an appoggiatura, which delays the main note on its string).
//!
//! A grace note sounds for its written value (its speed: a 64th, 32nd or 16th), but never
//! shorter than a floor for that speed: a 32nd at a fast tempo lasts about 40 ms, which a
//! SoundFont's attack barely gets through. The previous note on its string stops when it
//! starts (see [`crate::effects::one_note_per_string`]), as on a guitar, so it is not
//! drowned out.

use std::collections::HashMap;

use gscore_model::{BeatView, NoteKind, TrackView};

use crate::event::Channel;
use crate::sounding::Sounding;
use crate::tempo::PPQ;

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
    /// The written value: 64, 32 or 16 (another value counts as a 32nd).
    pub(crate) value: u16,
}

/// How long a grace note of written `value` (64, 32 or 16) sounds at `bpm`: its value,
/// but at least 40, 60 or 90 ms, so that the speeds stay apart and audible at any tempo.
pub(crate) fn length(bpm: u32, value: u16) -> u64 {
    let (written, floor_millis): (u64, u64) = match value {
        64 => (u64::from(PPQ) / 16, 40),
        16 => (u64::from(PPQ) / 4, 90),
        _ => (u64::from(PPQ) / 8, 60),
    };
    let floor = floor_millis
        .saturating_mul(u64::from(bpm))
        .saturating_mul(u64::from(PPQ))
        / 60_000;
    written.max(floor)
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
    for grace_beat in grace_beats {
        let value = grace_beat.note_value().map_or(32, |value| value.value());
        for note in grace_beat.notes() {
            match (view.sounding_pitch(&note), note.string()) {
                (Ok(pitch), Ok(string)) => graces.push(Grace {
                    key: pitch.get(),
                    velocity: soften(note.velocity(), note.kind() == NoteKind::Dead),
                    string: string.get(),
                    on_beat: note.grace().is_some_and(|grace| grace.on_beat),
                    value,
                }),
                _ => *skipped = skipped.saturating_add(1),
            }
        }
    }
    for note in beat.notes() {
        let Some(grace) = note.grace() else {
            continue;
        };
        let string = note.string().ok();
        let pitch = string
            .and_then(|string| view.pitch_at(string, grace.fret).ok())
            .or_else(|| view.sounding_pitch(&note).ok());
        match pitch {
            Some(pitch) => graces.push(Grace {
                key: pitch.get(),
                velocity: soften(note.velocity(), grace.dead),
                string: string.map_or(0, |string| string.get()),
                on_beat: grace.on_beat,
                value: u16::from(grace.speed.value()),
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

/// The sounding notes of `graces` for a beat at `start` at `bpm`, and how far the main
/// note on each string moves later (behind the grace notes played on the beat). Grace notes
/// before the beat end at it; at the start of the song, where there is no time before, they
/// are played on the beat too.
pub(crate) fn place(
    graces: &[Grace],
    start: u64,
    bpm: u32,
    track: usize,
    channel: Channel,
) -> (Vec<Sounding>, HashMap<u8, u64>) {
    let span = graces
        .iter()
        .filter(|grace| !grace.on_beat)
        .map(|grace| length(bpm, grace.value))
        .fold(0_u64, u64::saturating_add);
    let mut placed = Vec::with_capacity(graces.len());
    let mut on_beat: Vec<&Grace> = Vec::new();
    match start.checked_sub(span) {
        Some(mut at) => {
            for grace in graces.iter().filter(|grace| !grace.on_beat) {
                let length = length(bpm, grace.value);
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
        let length = length(bpm, grace.value);
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

    const PPQ64: u64 = PPQ as u64;

    fn grace(string: u8, on_beat: bool) -> Grace {
        Grace {
            key: 60,
            velocity: 90,
            string,
            on_beat,
            value: 32,
        }
    }

    #[test]
    fn a_grace_note_lasts_its_value_but_never_too_short() {
        // At 60 BPM every written value is long enough.
        assert_eq!(length(60, 64), PPQ64 / 16);
        assert_eq!(length(60, 32), PPQ64 / 8);
        assert_eq!(length(60, 16), PPQ64 / 4);
        // At 200 BPM a 32nd would be 37 ms: the floor of 60 ms holds.
        assert_eq!(length(200, 32), 60 * 200 * PPQ64 / 60_000);
        // The speeds stay in order at any tempo.
        for bpm in [40, 90, 120, 170, 240, 320] {
            assert!(length(bpm, 64) < length(bpm, 32), "{bpm}");
            assert!(length(bpm, 32) < length(bpm, 16), "{bpm}");
        }
    }

    #[test]
    fn graces_before_the_beat_end_at_it_in_order() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(1, false), grace(2, false)], 960, 60, 0, channel);
        let spans: Vec<(u64, u64)> = notes.iter().map(|n| (n.start, n.end)).collect();
        assert_eq!(spans, [(720, 840), (840, 960)]);
        assert!(delays.is_empty());
        assert!(
            notes.iter().all(|n| n.onset == 960),
            "they belong to their beat"
        );
    }

    #[test]
    fn an_appoggiatura_takes_the_beat_and_delays_its_string() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(3, true)], 960, 60, 0, channel);
        assert_eq!((notes[0].start, notes[0].end), (960, 1080));
        assert_eq!(delays.get(&3), Some(&120));
    }

    #[test]
    fn at_the_song_start_graces_move_onto_the_beat() {
        let channel = Channel::new(0).unwrap();
        let (notes, delays) = place(&[grace(1, false)], 0, 60, 0, channel);
        assert_eq!((notes[0].start, notes[0].end), (0, 120));
        assert_eq!(delays.get(&1), Some(&120), "the main note follows");
    }
}
