//! How beat effects and legato are played: a pick stroke strikes the chord quickly in its
//! direction (an upstroke a little softer), a brush stroke spreads it wider, tapping is
//! softer and slapping and popping louder, a fade-in swells from silence over the note,
//! and a note reached by a hammer-on, pull-off or beat-level legato is not picked (softer).

use gscore_model::{BeatView, Fade, SlapKind, Stroke, StrokeDirection, StrokeKind};

use crate::effects::{scale, spread, spread_with_offset};
use crate::event::{Channel, MidiEvent, controller};
use crate::performance::Attack;
use crate::sounding::Sounding;
use crate::tempo::PPQ;

/// A quarter note in ticks.
const QUARTER: u64 = PPQ as u64;
/// A pick stroke strikes a chord's strings within a 128th note.
const PICK_STROKE_TICKS: u64 = QUARTER / 32;
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
    let attack = match beat.slap() {
        Some(SlapKind::Tapping) => Some(Attack::Tapped),
        Some(SlapKind::Slapping) => Some(Attack::Slapped),
        Some(SlapKind::Popping) => Some(Attack::Popped),
        None => None,
    };
    let upstroke = beat.pick_stroke() == Some(StrokeDirection::Up);
    let fade = beat.fade();
    let stroke = beat
        .stroke(StrokeKind::Arpeggio)
        .or_else(|| beat.stroke(StrokeKind::Brush))
        .map(|stroke| stroke.direction)
        .or_else(|| beat.pick_stroke());
    for note in chord.iter_mut() {
        if let Some(percent) = percent {
            note.velocity = scale(note.velocity, percent);
        }
        if let Some(attack) = attack
            && note.attack == Attack::Picked
        {
            note.attack = attack;
        }
        note.stroke = note.stroke.or(stroke);
        if upstroke {
            note.velocity = scale(note.velocity, UPSTROKE_PERCENT);
        }
        note.fade = note.fade.or(fade);
        note.rasgueado |= beat.has_rasgueado();
    }
    // An arpeggio rolls the chord; then a brush stroke; then a pick stroke.
    if let Some(arpeggio) = beat.stroke(StrokeKind::Arpeggio) {
        spread_stroke(chord, arpeggio);
    } else if let Some(brush) = beat.stroke(StrokeKind::Brush) {
        spread_stroke(chord, brush);
    } else if let Some(direction) = beat.pick_stroke() {
        spread(chord, direction, PICK_STROKE_TICKS);
    }
}

/// Spreads a brush or arpeggio over its own speed, either starting on the beat or ending
/// on it.
fn spread_stroke(chord: &mut [Sounding], stroke: Stroke) {
    let total = QUARTER
        .checked_mul(4)
        .and_then(|quarter| quarter.checked_div(u64::from(stroke.speed.max(1))))
        .unwrap_or(QUARTER);
    spread_with_offset(chord, stroke.direction, total, !stroke.on_beat);
}

/// A note reached by a hammer-on, pull-off, legato or a tap of the fretting hand
/// (`attack`): no pick attack, so softer.
pub(crate) fn legato(note: &mut Sounding, attack: Attack) {
    note.velocity = scale(note.velocity, LEGATO_PERCENT);
    note.attack = attack;
}

/// The expression steps of a fade over a note from `start` to `end` on its own (borrowed)
/// channel: in from silence, out to silence, or a swell that rises and falls again.
pub(crate) fn fade_events(
    fade: Fade,
    start: u64,
    end: u64,
    channel: Channel,
) -> Vec<(u64, MidiEvent)> {
    let length = end.saturating_sub(start).max(1);
    let steps = if fade == Fade::Swell {
        FADE_STEPS.saturating_mul(2)
    } else {
        FADE_STEPS
    };
    (0..=steps)
        .map(|step| {
            let tick = start.saturating_add(
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
        let events = fade_events(Fade::In, 100, 1060, channel);
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
