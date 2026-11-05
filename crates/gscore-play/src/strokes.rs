//! How beat effects and legato are played: a pick stroke strikes the chord quickly in its
//! direction (an upstroke a little softer), a brush stroke spreads it wider, tapping is
//! softer and slapping and popping louder, a fade-in swells from silence over the note,
//! and a note reached by a hammer-on, pull-off or beat-level legato is not picked (softer).

use gscore_model::{BeatView, Fade, SlapKind, StrokeDirection};

use crate::effects::{scale, spread};
use crate::event::{Channel, MidiEvent, controller};
use crate::sounding::Sounding;
use crate::tempo::PPQ;

/// A quarter note in ticks.
const QUARTER: u64 = PPQ as u64;
/// A pick stroke strikes a chord's strings within a 128th note; a brush stroke spreads
/// them over a 32nd (Guitar Pro's default), an arpeggio (rolled chord) over an eighth.
const PICK_STROKE_TICKS: u64 = QUARTER / 32;
const BRUSH_TICKS: u64 = QUARTER / 8;
const ARPEGGIO_TICKS: u64 = QUARTER / 2;
/// Velocity shares (percent): an upstroke, tapping, slapping, popping, and a note reached
/// by legato instead of a pick.
const UPSTROKE_PERCENT: u32 = 90;
const TAPPING_PERCENT: u32 = 85;
const SLAPPING_PERCENT: u32 = 115;
const POPPING_PERCENT: u32 = 125;
const LEGATO_PERCENT: u32 = 80;
/// A fade-in rises in this many expression steps.
const FADE_STEPS: u64 = 16;

/// Applies the beat's effects to its notes (`chord`, all struck at the beat's start).
pub(crate) fn shape_beat(chord: &mut [Sounding], beat: &BeatView<'_>) {
    let percent = match beat.slap() {
        Some(SlapKind::Tapping) => Some(TAPPING_PERCENT),
        Some(SlapKind::Slapping) => Some(SLAPPING_PERCENT),
        Some(SlapKind::Popping) => Some(POPPING_PERCENT),
        None => None,
    };
    let upstroke = beat.pick_stroke() == Some(StrokeDirection::Up);
    let fade = beat.fade();
    for note in chord.iter_mut() {
        if let Some(percent) = percent {
            note.velocity = scale(note.velocity, percent);
        }
        if upstroke {
            note.velocity = scale(note.velocity, UPSTROKE_PERCENT);
        }
        note.fade = note.fade.or(fade);
        note.rasgueado |= beat.has_rasgueado();
    }
    // An arpeggio rolls the chord widest; then a brush stroke; then a pick stroke.
    if let Some(direction) = beat.arpeggio() {
        spread(chord, direction, ARPEGGIO_TICKS);
    } else {
        match (beat.brush_stroke(), beat.pick_stroke()) {
            (Some(direction), _) => spread(chord, direction, BRUSH_TICKS),
            (None, Some(direction)) => spread(chord, direction, PICK_STROKE_TICKS),
            (None, None) => {}
        }
    }
}

/// A note reached by a hammer-on, pull-off or legato: no pick attack, so softer.
pub(crate) fn legato(note: &mut Sounding) {
    note.velocity = scale(note.velocity, LEGATO_PERCENT);
}

/// The expression steps of a fade on the note's own (borrowed) channel: in from silence,
/// out to silence, or a swell that rises and falls again.
pub(crate) fn fade_events(note: &Sounding, channel: Channel) -> Vec<(u64, MidiEvent)> {
    let length = note.end.saturating_sub(note.start).max(1);
    let Some(fade) = note.fade else {
        return Vec::new();
    };
    let steps = if fade == Fade::Swell {
        FADE_STEPS.saturating_mul(2)
    } else {
        FADE_STEPS
    };
    (0..=steps)
        .map(|step| {
            let tick = note.start.saturating_add(
                length
                    .saturating_mul(step)
                    .checked_div(steps.max(1))
                    .unwrap_or(0),
            );
            let falling = fade == Fade::Out || (fade == Fade::Swell && step > FADE_STEPS);
            let share = if falling {
                steps.saturating_sub(step)
            } else {
                step.min(FADE_STEPS)
            };
            let value = u8::try_from(share.saturating_mul(127) / FADE_STEPS).unwrap_or(127);
            (
                tick,
                MidiEvent::Control {
                    channel,
                    controller: controller::EXPRESSION,
                    value,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use crate::effects::tests::note;

    #[test]
    fn a_pick_stroke_is_quicker_than_a_brush() {
        let mut picked = [note(1, 0, 960), note(6, 0, 960)];
        spread(&mut picked, StrokeDirection::Up, PICK_STROKE_TICKS);
        assert_eq!(
            picked[0].start, 0,
            "an upstroke starts on the highest string"
        );
        assert_eq!(
            picked[1].start, PICK_STROKE_TICKS,
            "quicker than a brush (a 32nd)"
        );
    }

    #[test]
    fn a_fade_in_rises_from_silence_over_the_note() {
        let channel = Channel::new(3).unwrap();
        let mut faded = note(1, 100, 1060);
        faded.fade = Some(Fade::In);
        let events = fade_events(&faded, channel);
        let values: Vec<(u64, u8)> = events
            .iter()
            .map(|(tick, event)| match event {
                MidiEvent::Control { value, .. } => (*tick, *value),
                _ => (0, 0),
            })
            .collect();
        assert_eq!(values.first(), Some(&(100, 0)));
        assert_eq!(values.last(), Some(&(1060, 127)));
        assert!(values.windows(2).all(|pair| pair[0].1 <= pair[1].1));
    }
}
