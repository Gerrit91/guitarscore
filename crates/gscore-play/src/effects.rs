//! How note and beat effects change what is played: ghost notes and accents change the
//! velocity, staccato and palm mute shorten the note, let ring holds it to the next note on
//! its string, tremolo picking repeats it, and vibrato shakes the pitch. A stroke spreads
//! a chord ([`spread`]; the beat effects themselves are in `strokes`).

use gscore_model::{AccentKind, NoteView, StrokeDirection, TremoloSpeed, TrillSpeed, Vibrato};

use crate::performance::Attack;
use crate::pitch::{PitchShape, Wave};
use crate::sounding::Sounding;
use crate::tempo::PPQ;

/// A quarter note in ticks.
const QUARTER: u64 = PPQ as u64;
/// Velocity shares (percent) of a ghost note and the two accents.
const GHOST_PERCENT: u32 = 55;
const ACCENT_PERCENT: u32 = 125;
const HEAVY_ACCENT_PERCENT: u32 = 145;
/// A staccato note sounds half its length, at most an eighth note.
const STACCATO_TICKS: u64 = QUARTER / 2;
/// A let-ring note holds at most this long past its own end when no note follows.
const LET_RING_TICKS: u64 = 4 * QUARTER;
/// How long one repeated pick or trill note lasts: a 64th, 32nd or 16th (a trill's or
/// tremolo's note value).
const SIXTY_FOURTH: u64 = QUARTER / 16;
const THIRTY_SECOND: u64 = QUARTER / 8;
const SIXTEENTH: u64 = QUARTER / 4;
/// A pick slide runs over an eighth note at most.
const PICK_SLIDE_TICKS: u64 = QUARTER / 2;
/// How many notes a pick slide scrapes through.
const PICK_SLIDE_NOTES: u64 = 6;
/// A rasgueado has three quick strikes; each is this far after the one before.
const RASGUEADO_STRIKES: u64 = 3;
const RASGUEADO_STEP: u64 = SIXTY_FOURTH;
/// Velocity shares of the later rasgueado strikes (percent).
const RASGUEADO_SECOND_PERCENT: u32 = 85;
const RASGUEADO_THIRD_PERCENT: u32 = 70;
/// Vibrato: how far the pitch swings (cents) and one swing's length, slight and wide.
const SLIGHT_DEPTH_CENTS: u16 = 50;
const WIDE_DEPTH_CENTS: u16 = 100;
const SLIGHT_PERIOD: u64 = QUARTER / 3;
const WIDE_PERIOD: u64 = QUARTER / 2;

/// `velocity` scaled to `percent`, kept within MIDI's 1 to 127.
pub(crate) fn scale(velocity: u8, percent: u32) -> u8 {
    let scaled = u32::from(velocity).saturating_mul(percent) / 100;
    u8::try_from(scaled.clamp(1, 127)).unwrap_or(127)
}

/// Applies the note's own effects to its velocity and length. A palm mute changes neither
/// here: the modelled strings play it at full strength and its written length (the palm
/// damps the string), and the SoundFont path shortens it ([`crate::midi`]).
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
    if note.is_staccato() {
        sounding.end = sounding
            .start
            .saturating_add((length / 2).clamp(1, STACCATO_TICKS));
        sounding.damp = true;
    }
}

/// Spreads a chord's onsets over `total` ticks for a pick or brush stroke: a down stroke
/// from the lowest string (the highest string number), an up stroke from the highest.
pub(crate) fn spread(chord: &mut [Sounding], direction: StrokeDirection, total: u64) {
    spread_with_offset(chord, direction, total, false);
}

/// Like [`spread`], but with `before` the whole roll is moved earlier so that its last
/// note lands on the beat (the spread "falls" on the beat) instead of starting on it.
pub(crate) fn spread_with_offset(
    chord: &mut [Sounding],
    direction: StrokeDirection,
    total: u64,
    before: bool,
) {
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
    // A roll ending on the beat needs room before it: at the very start of the song it is
    // squeezed into what there is, so the last string still lands on the beat.
    let total = if before {
        chord
            .iter()
            .map(|note| note.start)
            .min()
            .map_or(0, |start| total.min(start))
    } else {
        total
    };
    for (position, index) in order.into_iter().enumerate() {
        let Some(note) = chord.get_mut(index) else {
            continue;
        };
        let delay = total
            .saturating_mul(u64::try_from(position).unwrap_or(0))
            .checked_div(gaps)
            .unwrap_or(0);
        note.start = if before {
            // Starting earlier only lengthens the note, so nothing is clamped here.
            note.start.saturating_sub(total).saturating_add(delay)
        } else {
            // Never past the note's end.
            let delay = delay.min(note.end.saturating_sub(note.start).saturating_sub(1));
            note.start.saturating_add(delay)
        };
    }
}

/// A string sounds one note at a time: a note still ringing when the next one on its
/// string (of the same track) starts stops there. This matters where notes overlap on
/// purpose, such as a grace note struck before the beat while the previous note would
/// still ring into it.
pub(crate) fn one_note_per_string(notes: &mut [Sounding]) {
    let mut order: Vec<usize> = (0..notes.len())
        .filter(|&index| notes.get(index).is_some_and(|note| note.string != 0))
        .collect();
    order.sort_by_key(|&index| {
        notes
            .get(index)
            .map_or((0, 0, 0), |note| (note.track, note.string, note.start))
    });
    for pair in order.windows(2) {
        let [this, next] = pair else {
            continue;
        };
        let Some(&next) = notes.get(*next) else {
            continue;
        };
        if let Some(note) = notes.get_mut(*this)
            && (note.track, note.string) == (next.track, next.string)
            && note.start < next.start
            && note.end > next.start
        {
            note.end = next.start;
        }
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

/// The duration of one repeated pick at `speed`.
fn tremolo_ticks(speed: TremoloSpeed) -> u64 {
    match speed {
        TremoloSpeed::Eighth => QUARTER / 2,
        TremoloSpeed::Sixteenth => SIXTEENTH,
        TremoloSpeed::ThirtySecond => THIRTY_SECOND,
    }
}

/// The duration of one trill note at `speed`.
fn trill_ticks(speed: TrillSpeed) -> u64 {
    match speed {
        TrillSpeed::Sixteenth => SIXTEENTH,
        TrillSpeed::ThirtySecond => THIRTY_SECOND,
        TrillSpeed::SixtyFourth => SIXTY_FOURTH,
    }
}

/// Replaces tremolo-picked notes by repeated notes at their speed over their length.
pub(crate) fn tremolo_picking(notes: Vec<Sounding>) -> Vec<Sounding> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        let Some(speed) = note.tremolo else {
            out.push(note);
            continue;
        };
        let step = tremolo_ticks(speed);
        if note.end.saturating_sub(note.start) <= step {
            out.push(note);
            continue;
        }
        let mut start = note.start;
        while start < note.end {
            let end = start.saturating_add(step).min(note.end);
            out.push(Sounding {
                start,
                end,
                tremolo: None,
                // Only the first pick carries a slide, a bend or a fade.
                fade: note.fade.filter(|_| start == note.start),
                slide: if start == note.start {
                    note.slide
                } else {
                    None
                },
                bend: if start == note.start { note.bend } else { None },
                bend_curve: if start == note.start {
                    note.bend_curve
                } else {
                    None
                },
                ..note
            });
            start = end;
        }
    }
    out
}

/// Replaces trilled notes by notes alternating with the trill key at the trill's speed.
pub(crate) fn trills(notes: Vec<Sounding>) -> Vec<Sounding> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        let Some((other_key, other_fret, speed)) = note.trill else {
            out.push(note);
            continue;
        };
        let step = trill_ticks(speed);
        if note.end.saturating_sub(note.start) <= step {
            out.push(note);
            continue;
        }
        let mut start = note.start;
        let mut alternate = false;
        while start < note.end {
            let end = start.saturating_add(step).min(note.end);
            out.push(Sounding {
                start,
                end,
                key: if alternate { other_key } else { note.key },
                fret: if alternate {
                    Some(other_fret)
                } else {
                    note.fret
                },
                // The first note is struck; the rest are hammered on and pulled off.
                attack: if start == note.start {
                    note.attack
                } else {
                    Attack::Legato
                },
                trill: None,
                // Only the first note carries the note's slide, bend or fade.
                fade: note.fade.filter(|_| start == note.start),
                slide: if start == note.start {
                    note.slide
                } else {
                    None
                },
                bend: if start == note.start { note.bend } else { None },
                bend_curve: if start == note.start {
                    note.bend_curve
                } else {
                    None
                },
                ..note
            });
            alternate = !alternate;
            start = end;
        }
    }
    out
}

/// Replaces a pick-slide note by a quick run of muted notes scraping to the next note on
/// its string (or a fixed span when there is none), in the slide's direction.
pub(crate) fn pick_slides(notes: Vec<Sounding>) -> Vec<Sounding> {
    // The next note on each (track, string), by start, to know where the scrape goes.
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by_key(|&index| {
        notes
            .get(index)
            .map_or((0, 0, 0), |note| (note.track, note.string, note.start))
    });
    let mut next: Vec<Option<(u8, u64)>> = vec![None; notes.len()];
    for pair in order.windows(2) {
        let [this, next_index] = pair else {
            continue;
        };
        let (Some(&this_note), Some(&next_note)) = (notes.get(*this), notes.get(*next_index))
        else {
            continue;
        };
        if (this_note.track, this_note.string) == (next_note.track, next_note.string)
            && let Some(slot) = next.get_mut(*this)
        {
            *slot = Some((next_note.key, next_note.start));
        }
    }
    let mut out = Vec::with_capacity(notes.len());
    for (index, note) in notes.iter().enumerate() {
        let Some(direction) = note.pick_slide else {
            out.push(*note);
            continue;
        };
        let (target, next_start) = next.get(index).copied().flatten().unwrap_or_else(|| {
            // No next note: scrape a whole octave in the slide's direction.
            let target = match direction {
                StrokeDirection::Up => note.key.saturating_add(12),
                StrokeDirection::Down => note.key.saturating_sub(12),
            };
            (target, note.start.saturating_add(PICK_SLIDE_TICKS))
        });
        // The scrape runs over an eighth at most, but stops at the next note.
        let run_end = note
            .start
            .saturating_add(PICK_SLIDE_TICKS)
            .min(next_start)
            .max(note.start.saturating_add(1));
        let step = run_end
            .saturating_sub(note.start)
            .checked_div(PICK_SLIDE_NOTES.max(1))
            .unwrap_or(1)
            .max(1);
        let last = PICK_SLIDE_NOTES.saturating_sub(1).max(1);
        let from = i32::from(note.key);
        let to = i32::from(target);
        let span = to.saturating_sub(from);
        for step_index in 0..PICK_SLIDE_NOTES {
            let offset = i32::try_from(step_index).unwrap_or(0);
            let divisor = i32::try_from(last).unwrap_or(1);
            let key = from.saturating_add(
                span.saturating_mul(offset)
                    .checked_div(divisor)
                    .unwrap_or(0),
            );
            let key = u8::try_from(key.clamp(0, 127)).unwrap_or(note.key);
            let start = note
                .start
                .saturating_add(step.saturating_mul(step_index))
                .min(run_end.saturating_sub(1));
            out.push(Sounding {
                start,
                end: start.saturating_add(step).min(run_end),
                key,
                fret: None,
                attack: Attack::Scrape,
                velocity: scale(note.velocity, 85),
                pick_slide: None,
                damp: true,
                ..*note
            });
        }
    }
    out
}

/// Adds two more quick strikes to a rasgueado beat, softer than the first.
pub(crate) fn rasgueados(notes: Vec<Sounding>) -> Vec<Sounding> {
    let mut out = Vec::with_capacity(notes.len());
    let mut index = 0;
    while let Some(note) = notes.get(index) {
        if !note.rasgueado {
            out.push(*note);
            index = index.saturating_add(1);
            continue;
        }
        // The whole beat shares its onset.
        let onset = note.onset;
        let start = index;
        let mut end = index;
        while notes
            .get(end)
            .is_some_and(|other| other.rasgueado && other.onset == onset)
        {
            end = end.saturating_add(1);
        }
        let chord = notes.get(start..end).unwrap_or_default();
        for note in chord {
            // Each strike sounds until the next one on its string; the last one keeps the
            // note's end, so nothing rings into the next note (strings were already cut to
            // one note at a time). A note too short for the strikes is struck once.
            let room = note.end.saturating_sub(note.start);
            let strikes = if room > RASGUEADO_STEP.saturating_mul(RASGUEADO_STRIKES) {
                RASGUEADO_STRIKES
            } else {
                1
            };
            for strike in 0..strikes {
                let percent = match strike {
                    0 => 100,
                    1 => RASGUEADO_SECOND_PERCENT,
                    _ => RASGUEADO_THIRD_PERCENT,
                };
                let start = note
                    .start
                    .saturating_add(RASGUEADO_STEP.saturating_mul(strike));
                let end = if strike.saturating_add(1) < strikes {
                    start.saturating_add(RASGUEADO_STEP)
                } else {
                    note.end
                };
                out.push(Sounding {
                    start,
                    end,
                    velocity: scale(note.velocity, percent),
                    rasgueado: false,
                    ..*note
                });
            }
        }
        index = end;
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
pub(crate) mod tests {
    use super::*;
    use crate::event::Channel;

    /// A plain note for the tests.
    pub(crate) fn note(string: u8, start: u64, end: u64) -> Sounding {
        Sounding::new(0, string, 60, 100, start, end)
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
        spread(&mut chord, StrokeDirection::Down, 120);
        assert_eq!(chord[1].start, 0);
        assert!(chord[2].start > 0 && chord[2].start < chord[0].start);
        assert_eq!(chord[0].start, 120);
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
    fn a_note_stops_when_the_next_on_its_string_starts() {
        let mut notes = [note(1, 0, 960), note(1, 840, 960), note(2, 0, 960)];
        one_note_per_string(&mut notes);
        assert_eq!(notes[0].end, 840, "cut by the grace note on its string");
        assert_eq!(notes[1].end, 960);
        assert_eq!(notes[2].end, 960, "another string rings on");
    }

    #[test]
    fn tremolo_picking_repeats_the_note_at_its_speed() {
        let mut long = note(1, 0, QUARTER);
        long.tremolo = Some(TremoloSpeed::ThirtySecond);
        let picks = tremolo_picking(vec![long]);
        assert_eq!(picks.len(), 8);
        assert_eq!(picks[7].end, QUARTER);

        let mut eighth = note(1, 0, QUARTER);
        eighth.tremolo = Some(TremoloSpeed::Eighth);
        assert_eq!(tremolo_picking(vec![eighth]).len(), 2);
    }

    #[test]
    fn vibrato_gives_a_note_a_shaking_shape() {
        let mut notes = [note(1, 0, QUARTER)];
        notes[0].vibrato = Some(Vibrato::Wide);
        apply_vibrato(&mut notes);
        let events = notes[0].shape.unwrap().events(Channel::new(0).unwrap(), 0);
        assert!(events.len() > 10);
    }
}
