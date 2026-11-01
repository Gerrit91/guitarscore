//! Pitch bends for bends and slides: the shape over one note and the MIDI messages that
//! play it.
//!
//! A note that bends or slides borrows a free MIDI channel (AGENTS.md section 5, decision
//! 10), so the bend does not move the rest of the chord or track. The borrowed channel gets
//! a pitch bend range of [`BEND_RANGE_SEMITONES`] through RPN 0,0 before the note starts;
//! without that, the range would depend on the synthesizer's default.

use crate::event::{Channel, MidiEvent, PITCH_BEND_CENTER, controller};

/// Pitch bend range set on a borrowed channel with RPN 0,0, in semitones.
pub(crate) const BEND_RANGE_SEMITONES: u8 = 12;
/// Quarter tones in a semitone.
const QUARTER_TONES_PER_SEMITONE: i32 = 2;
/// Largest pitch offset the configured range can represent, in quarter tones (12 semitones).
const MAX_OFFSET_QUARTER_TONES: i32 = 24;
/// How far an "out" slide moves when no target note is known: a whole step.
const OUT_SLIDE_QUARTER_TONES: i32 = 4;
/// How many messages approximate a pitch ramp.
const RAMP_STEPS: u64 = 16;

/// A pitch movement over one note: from `start` to `target` quarter tones over `ramp` ticks,
/// then held until the note ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PitchShape {
    start: i32,
    target: i32,
    ramp: u64,
}

impl PitchShape {
    /// A bend up to `quarter_tones` that reaches its target within `ramp_limit` ticks.
    pub(crate) fn up(quarter_tones: i16, length: u64, ramp_limit: u64) -> Self {
        PitchShape {
            start: 0,
            target: i32::from(quarter_tones),
            ramp: ramp_ticks(length, ramp_limit),
        }
    }

    /// A bend that starts at `quarter_tones` and releases back to the unbent pitch.
    pub(crate) fn release(quarter_tones: i16, length: u64) -> Self {
        PitchShape {
            start: i32::from(quarter_tones),
            target: 0,
            ramp: ramp_ticks(length, length),
        }
    }

    /// A slide from this note's pitch to a note `delta_semitones` away.
    pub(crate) fn slide_to(delta_semitones: i32, length: u64) -> Self {
        PitchShape {
            start: 0,
            target: delta_semitones.saturating_mul(QUARTER_TONES_PER_SEMITONE),
            ramp: ramp_ticks(length, length),
        }
    }

    /// A slide in from a note `delta_semitones` away.
    pub(crate) fn slide_in(delta_semitones: i32, length: u64, ramp_limit: u64) -> Self {
        PitchShape {
            start: delta_semitones.saturating_mul(QUARTER_TONES_PER_SEMITONE),
            target: 0,
            ramp: ramp_ticks(length, ramp_limit),
        }
    }

    /// A slide out with no target note, up or down by a whole step.
    pub(crate) fn slide_out(up: bool, length: u64) -> Self {
        PitchShape {
            start: 0,
            target: if up {
                OUT_SLIDE_QUARTER_TONES
            } else {
                OUT_SLIDE_QUARTER_TONES.saturating_neg()
            },
            ramp: ramp_ticks(length, length),
        }
    }

    /// The bend messages for this shape, as `(tick, message)`, including the initial value
    /// and the ramp but not the reset after the note.
    pub(crate) fn events(&self, channel: Channel, start_tick: u64) -> Vec<(u64, MidiEvent)> {
        let mut events =
            Vec::with_capacity(usize::try_from(RAMP_STEPS.saturating_add(1)).unwrap_or(0));
        events.push((
            start_tick,
            MidiEvent::PitchBend {
                channel,
                value: pitch_bend_value(self.start),
            },
        ));
        if self.ramp == 0 || self.start == self.target {
            return events;
        }
        let steps = RAMP_STEPS.min(self.ramp).max(1);
        for step in 1..=steps {
            let tick = start_tick.saturating_add(
                self.ramp
                    .saturating_mul(step)
                    .checked_div(steps)
                    .unwrap_or(0),
            );
            let delta = i64::from(self.target).saturating_sub(i64::from(self.start));
            let offset = delta
                .saturating_mul(i64::try_from(step).unwrap_or(i64::MAX))
                .checked_div(i64::try_from(steps).unwrap_or(1))
                .unwrap_or(0);
            let value = i64::from(self.start).saturating_add(offset);
            events.push((
                tick,
                MidiEvent::PitchBend {
                    channel,
                    value: pitch_bend_value_i64(value),
                },
            ));
        }
        events
    }
}

/// How many ticks a ramp lasts: at most `limit`, and one tick before the note ends.
fn ramp_ticks(length: u64, limit: u64) -> u64 {
    length.saturating_sub(1).min(limit)
}

/// The 14-bit bend value for `quarter_tones`, clamped to the configured range.
pub(crate) fn pitch_bend_value(quarter_tones: i32) -> u16 {
    pitch_bend_value_i64(i64::from(quarter_tones))
}

/// [`pitch_bend_value`] for an intermediate value of a ramp.
fn pitch_bend_value_i64(quarter_tones: i64) -> u16 {
    let maximum = i64::from(MAX_OFFSET_QUARTER_TONES);
    let offset = quarter_tones.clamp(maximum.saturating_neg(), maximum);
    let value = i64::from(PITCH_BEND_CENTER).saturating_add(
        offset
            .saturating_mul(i64::from(PITCH_BEND_CENTER))
            .checked_div(maximum)
            .unwrap_or(0),
    );
    u16::try_from(value.clamp(0, 0x3FFF)).unwrap_or(PITCH_BEND_CENTER)
}

/// The RPN messages that set a borrowed channel's pitch bend range, and a center bend.
pub(crate) fn bend_range_setup(channel: Channel) -> [MidiEvent; 7] {
    [
        MidiEvent::Control {
            channel,
            controller: controller::RPN_MSB,
            value: 0,
        },
        MidiEvent::Control {
            channel,
            controller: controller::RPN_LSB,
            value: 0,
        },
        MidiEvent::Control {
            channel,
            controller: controller::DATA_ENTRY_MSB,
            value: BEND_RANGE_SEMITONES,
        },
        MidiEvent::Control {
            channel,
            controller: controller::DATA_ENTRY_LSB,
            value: 0,
        },
        // Leave RPN selection so later controllers are not taken as RPN data.
        MidiEvent::Control {
            channel,
            controller: controller::RPN_MSB,
            value: 127,
        },
        MidiEvent::Control {
            channel,
            controller: controller::RPN_LSB,
            value: 127,
        },
        MidiEvent::PitchBend {
            channel,
            value: PITCH_BEND_CENTER,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> Channel {
        Channel::new(0).unwrap()
    }

    #[test]
    fn bend_values_span_the_configured_range() {
        assert_eq!(pitch_bend_value(0), PITCH_BEND_CENTER);
        assert_eq!(pitch_bend_value(MAX_OFFSET_QUARTER_TONES), 0x3FFF);
        assert_eq!(pitch_bend_value(-MAX_OFFSET_QUARTER_TONES), 0);
        assert_eq!(pitch_bend_value(i32::MAX), 0x3FFF);
        assert_eq!(pitch_bend_value(i32::MIN), 0);
        // A whole step (4 quarter tones) is a sixth of the range.
        assert_eq!(pitch_bend_value(4), 8192 + 1365);
    }

    #[test]
    fn a_bend_ramps_up_to_its_target_and_holds() {
        let shape = PitchShape::up(4, 400, 100);
        let events = shape.events(channel(), 1000);
        assert_eq!(events.len(), usize::try_from(RAMP_STEPS).unwrap() + 1);
        assert_eq!(events[0].0, 1000);
        assert_eq!(
            events[0].1,
            MidiEvent::PitchBend {
                channel: channel(),
                value: PITCH_BEND_CENTER
            }
        );
        let last = events.last().unwrap();
        assert_eq!(last.0, 1100, "the ramp lasts at most 100 ticks");
        assert_eq!(
            last.1,
            MidiEvent::PitchBend {
                channel: channel(),
                value: pitch_bend_value(4)
            }
        );
        let values: Vec<u16> = events
            .iter()
            .filter_map(|(_, event)| match event {
                MidiEvent::PitchBend { value, .. } => Some(*value),
                _ => None,
            })
            .collect();
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_release_starts_bent_and_returns_to_center() {
        let shape = PitchShape::release(4, 500);
        let events = shape.events(channel(), 0);
        assert_eq!(
            events[0].1,
            MidiEvent::PitchBend {
                channel: channel(),
                value: pitch_bend_value(4)
            }
        );
        let last = events.last().unwrap();
        assert_eq!(last.0, 499, "the ramp ends just before the note");
        assert_eq!(
            last.1,
            MidiEvent::PitchBend {
                channel: channel(),
                value: PITCH_BEND_CENTER
            }
        );
    }

    #[test]
    fn short_notes_and_equal_ends_produce_one_event() {
        let shape = PitchShape::up(4, 1, 100);
        assert_eq!(shape.events(channel(), 7).len(), 1);
        assert_eq!(PitchShape::slide_to(0, 100).events(channel(), 0).len(), 1);
    }

    #[test]
    fn the_range_setup_selects_rpn_zero_and_resets_it() {
        let setup = bend_range_setup(channel());
        assert_eq!(setup.len(), 7);
        assert_eq!(
            setup[0],
            MidiEvent::Control {
                channel: channel(),
                controller: controller::RPN_MSB,
                value: 0
            }
        );
        assert_eq!(
            setup[2],
            MidiEvent::Control {
                channel: channel(),
                controller: controller::DATA_ENTRY_MSB,
                value: BEND_RANGE_SEMITONES
            }
        );
        assert_eq!(
            setup[6],
            MidiEvent::PitchBend {
                channel: channel(),
                value: PITCH_BEND_CENTER
            }
        );
    }
}
