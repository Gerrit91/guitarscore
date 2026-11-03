//! How note and beat effects change what is played: ghost notes and accents change the
//! velocity, staccato and palm mute shorten the note, let ring holds it to the next note on
//! its string, tremolo picking repeats it, a brush stroke spreads the chord, and vibrato
//! shakes the pitch.

use gscore_model::{AccentKind, NoteView, StrokeDirection, Vibrato};

use crate::pitch::{PitchShape, Wave};
use crate::sounding::Sounding;
use crate::tempo::PPQ;

/// A quarter note in ticks.
const QUARTER: u64 = PPQ as u64;
/// Velocity shares (percent) of a ghost note, a palm-muted note and the two accents.
const GHOST_PERCENT: u32 = 55;
const PALM_MUTE_PERCENT: u32 = 85;
const ACCENT_PERCENT: u32 = 125;
const HEAVY_ACCENT_PERCENT: u32 = 145;
/// A palm-muted note stops after an eighth note at most.
const PALM_MUTE_TICKS: u64 = QUARTER / 2;
/// A staccato note sounds half its length, at most an eighth note.
const STACCATO_TICKS: u64 = QUARTER / 2;
/// A let-ring note holds at most this long past its own end when no note follows.
const LET_RING_TICKS: u64 = 4 * QUARTER;
/// Tremolo picking repeats the note every 32nd note.
const TREMOLO_TICKS: u64 = QUARTER / 8;
/// A brush stroke spreads the chord over this long (a 32nd note, as Guitar Pro's default).
const BRUSH_TICKS: u64 = QUARTER / 8;
/// Vibrato: how far the pitch swings (cents) and one swing's length, slight and wide.
const SLIGHT_DEPTH_CENTS: u16 = 50;
const WIDE_DEPTH_CENTS: u16 = 100;
const SLIGHT_PERIOD: u64 = QUARTER / 3;
const WIDE_PERIOD: u64 = QUARTER / 2;

/// `velocity` scaled to `percent`, kept within MIDI's 1 to 127.
fn scale(velocity: u8, percent: u32) -> u8 {
    let scaled = u32::from(velocity).saturating_mul(percent) / 100;
    u8::try_from(scaled.clamp(1, 127)).unwrap_or(127)
}

/// Applies the note's own effects to its velocity and length.
pub(crate) fn shape_note(sounding: &mut Sounding, note: &NoteView<'_>) {
    let length = sounding.end.saturating_sub(sounding.start);
    if note.is_ghost() {
        sounding.velocity = scale(sounding.velocity, GHOST_PERCENT);
    }
    match note.accent() {
        Some(AccentKind::Heavy) => {
            sounding.velocity = scale(sounding.velocity, HEAVY_ACCENT_PERCENT);
        }
        Some(_) => sounding.velocity = scale(sounding.velocity, ACCENT_PERCENT),
        None => {}
    }
    if note.is_palm_muted() {
        sounding.velocity = scale(sounding.velocity, PALM_MUTE_PERCENT);
        sounding.end = sounding.start.saturating_add(length.min(PALM_MUTE_TICKS));
        sounding.damp = true;
    }
    if note.is_staccato() {
        sounding.end = sounding
            .start
            .saturating_add((length / 2).clamp(1, STACCATO_TICKS));
        sounding.damp = true;
    }
}

/// Spreads a chord's onsets for a brush stroke: a down stroke from the lowest string (the
/// highest string number), an up stroke from the highest.
pub(crate) fn brush(chord: &mut [Sounding], direction: StrokeDirection) {
    let count = chord.len();
    if count < 2 {
        return;
    }
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|&index| chord.get(index).map_or(0, |note| note.string));
    if direction == StrokeDirection::Down {
        order.reverse();
    }
    let gaps = u64::try_from(count.saturating_sub(1)).unwrap_or(1).max(1);
    for (position, index) in order.into_iter().enumerate() {
        let Some(note) = chord.get_mut(index) else {
            continue;
        };
        let delay = BRUSH_TICKS
            .saturating_mul(u64::try_from(position).unwrap_or(0))
            .checked_div(gaps)
            .unwrap_or(0);
        // Never past the note's end.
        let delay = delay.min(note.end.saturating_sub(note.start).saturating_sub(1));
        note.start = note.start.saturating_add(delay);
    }
}

/// Holds let-ring notes until the next note on the same string of the same track starts
/// (at most [`LET_RING_TICKS`] past their own end).
pub(crate) fn let_ring(notes: &mut [Sounding]) {
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by_key(|&index| {
        notes
            .get(index)
            .map_or((0, 0, 0), |note| (note.track, note.string, note.start))
    });
    for pair in order.windows(2) {
        let [this, next] = pair else {
            continue;
        };
        let next_start = notes
            .get(*next)
            .zip(notes.get(*this))
            .filter(|(next, this)| (next.track, next.string) == (this.track, this.string))
            .map(|(next, _)| next.start);
        if let Some(note) = notes.get_mut(*this)
            && note.let_ring
        {
            let limit = note.end.saturating_add(LET_RING_TICKS);
            note.end = next_start
                .map_or(limit, |start| start.min(limit))
                .max(note.end);
        }
    }
    // The last note on each string has no follower.
    if let Some(&last) = order.last()
        && let Some(note) = notes.get_mut(last)
        && note.let_ring
    {
        note.end = note.end.saturating_add(LET_RING_TICKS);
    }
}

/// Replaces tremolo-picked notes by repeated 32nd notes over their length.
pub(crate) fn tremolo_picking(notes: Vec<Sounding>) -> Vec<Sounding> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        if !note.tremolo || note.end.saturating_sub(note.start) <= TREMOLO_TICKS {
            out.push(note);
            continue;
        }
        let mut start = note.start;
        while start < note.end {
            let end = start.saturating_add(TREMOLO_TICKS).min(note.end);
            out.push(Sounding {
                start,
                end,
                tremolo: false,
                // Only the first pick carries a slide or bend.
                slide: if start == note.start {
                    note.slide
                } else {
                    None
                },
                bend: if start == note.start { note.bend } else { None },
                ..note
            });
            start = end;
        }
    }
    out
}

/// Adds the vibrato to each shaking note's pitch shape (a steady one if it has none).
pub(crate) fn apply_vibrato(notes: &mut [Sounding]) {
    for note in notes {
        let Some(vibrato) = note.vibrato else {
            continue;
        };
        let (depth_cents, period) = match vibrato {
            Vibrato::Wide => (WIDE_DEPTH_CENTS, WIDE_PERIOD),
            _ => (SLIGHT_DEPTH_CENTS, SLIGHT_PERIOD),
        };
        let wave = Wave {
            depth_cents,
            period,
            length: note.end.saturating_sub(note.start),
        };
        note.shape = Some(
            note.shape
                .unwrap_or_else(PitchShape::steady)
                .with_vibrato(wave),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Channel;

    fn note(string: u8, start: u64, end: u64) -> Sounding {
        let channel = Channel::new(0).unwrap();
        Sounding {
            track: 0,
            channel,
            play_channel: channel,
            key: 60,
            velocity: 100,
            start,
            end,
            string,
            slide: None,
            bend: None,
            shape: None,
            vibrato: None,
            let_ring: false,
            tremolo: false,
            onset: start,
            damp: false,
        }
    }

    #[test]
    fn velocities_stay_in_midi_range() {
        assert_eq!(scale(100, GHOST_PERCENT), 55);
        assert_eq!(scale(120, HEAVY_ACCENT_PERCENT), 127);
        assert_eq!(scale(1, GHOST_PERCENT), 1);
    }

    #[test]
    fn a_down_brush_starts_on_the_lowest_string() {
        let mut chord = [note(1, 0, 960), note(6, 0, 960), note(3, 0, 960)];
        brush(&mut chord, StrokeDirection::Down);
        assert_eq!(chord[1].start, 0);
        assert!(chord[2].start > 0 && chord[2].start < chord[0].start);
        assert_eq!(chord[0].start, BRUSH_TICKS);
    }

    #[test]
    fn let_ring_holds_to_the_next_note_on_the_string() {
        let mut notes = [note(1, 0, 240), note(2, 0, 240), note(1, 960, 1200)];
        notes[0].let_ring = true;
        notes[1].let_ring = true;
        let_ring(&mut notes);
        assert_eq!(notes[0].end, 960);
        assert_eq!(notes[1].end, 240 + LET_RING_TICKS);
    }

    #[test]
    fn tremolo_picking_repeats_the_note() {
        let mut long = note(1, 0, QUARTER);
        long.tremolo = true;
        let picks = tremolo_picking(vec![long]);
        assert_eq!(picks.len(), 8);
        assert_eq!(picks[7].end, QUARTER);
    }

    #[test]
    fn vibrato_gives_a_note_a_shaking_shape() {
        let mut notes = [note(1, 0, QUARTER)];
        notes[0].vibrato = Some(Vibrato::Wide);
        apply_vibrato(&mut notes);
        let events = notes[0].shape.unwrap().events(notes[0].channel, 0);
        assert!(events.len() > 10);
    }
}
