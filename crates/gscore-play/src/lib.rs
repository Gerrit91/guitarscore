//! Playback sequencing: turns the model into a [`Performance`] (every note with its string,
//! fret, timing and techniques, for any engine), derives timed MIDI events from it, allocates
//! MIDI channels, writes Standard MIDI Files, and defines the [`Player`] trait that output
//! backends (`gscore-audio`) implement. No device crates and no clock here; backends pass
//! the time into the [`Scheduler`]. A [`Sequence`] carries the performance it was derived
//! from, so a backend can play the MIDI events, the notes, or both.
//!
//! Played: notes of all voices with exact timing, ties, dead notes (short and soft), grace
//! notes (about 75 ms, just before their beat or on it; they take no time of their own),
//! the tracks' instruments, volume and pan, tempo changes, repeats and alternative endings,
//! swing/triplet, dotted and Scottish feel, bends, slides and vibrato (pitch bends on a
//! borrowed channel, so the rest of the chord stays put; the model stores only the highest
//! point of a bend, so bend curves are simplified), ghost notes and accents (velocity),
//! staccato and palm mute (shorter), let ring, tremolo picking at its speed, trills (the
//! note alternating with the trilled fret), the whammy bar, pick and brush strokes and
//! arpeggios (the chord struck string by string), rasgueados (three quick strikes), dead
//! slapped beats (muted hits on every string), tapping, slapping and popping (velocity),
//! left-hand taps and grace hammer-ons (softer, like legato), pick slides (a quick muted
//! scrape), fades in, out and swells (an expression ramp on a borrowed channel), and
//! hammer-ons, pull-offs and legato (the reached note is not picked: softer).
//! Repeats, alternative endings and navigation signs (D.C., D.S., coda, fine) are followed,
//! and fermatas hold the end of their measure.
//! [`Sequence::beat_excerpt`] plays one beat alone, for hearing an edit.

#![warn(clippy::arithmetic_side_effects)]

mod build;
mod channels;
mod click;
mod curve;
mod drums;
mod effects;
mod event;
mod excerpt;
mod graces;
mod midi;
mod order;
mod performance;
mod pitch;
mod player;
mod sequence;
mod smf;
mod sounding;
mod strokes;
mod swing;
mod tempo;
mod timeline;
mod wire;
mod wire_notes;

pub use channels::ChannelAllocator;
pub use click::{
    CLICK_SOUND_CONTROLLER, CLICK_TRACK, Click, ClickSound, MAX_COUNT_IN_BARS, count_in, lead_in,
};
pub use event::{Channel, MidiEvent, controller};
pub use order::playback_order;
// The model's types in a performance's public fields, so engines need not depend on the
// model.
pub use gscore_model::{Fade, HarmonicKind, InstrumentKind, StrokeDirection};
pub use performance::{Attack, Performance, PerformanceTrack, PlayedNote, StringKind, TrackEngine};
pub use pitch::PitchShape;
pub use player::{PlayError, PlayState, Player, Scheduler, TEMPO_SCALE_RANGE};
pub use sequence::{MeasureSpan, Sequence, SequenceTrack, TimedEvent, sound_events};
pub use smf::{SmfError, to_smf};
pub use tempo::{PPQ, TempoMap, micros_per_quarter};
pub use timeline::{Timeline, tempo_at};
pub use wire::{decode_sequence, encode_sequence};

// The integration tests sequence the owner's files with `gscore-io`; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;
