//! Playback sequencing: turns the model into timed MIDI events, allocates MIDI channels,
//! writes Standard MIDI Files, and defines the [`Player`] trait that output backends
//! (`gscore-audio`) implement. No device crates and no clock here; backends pass the time
//! into the [`Scheduler`].
//!
//! Played: notes of all voices with exact timing, ties, dead notes (short and soft), grace
//! notes (about 75 ms, just before their beat or on it; they take no time of their own),
//! the tracks' instruments, volume and pan, tempo changes, repeats and alternative endings,
//! swing/triplet, dotted and Scottish feel, bends, slides and vibrato (pitch bends on a
//! borrowed channel, so the rest of the chord stays put; the model stores only the highest
//! point of a bend, so bend curves are simplified), ghost notes and accents (velocity),
//! staccato and palm mute (shorter), let ring, tremolo picking and brush strokes.
//! [`Sequence::beat_excerpt`] plays one beat alone, for hearing an edit.
//! Not yet: trills, whammy bar, fade-in, fermatas and directions (D.C., D.S., coda).

#![warn(clippy::arithmetic_side_effects)]

mod build;
mod channels;
mod click;
mod curve;
mod effects;
mod event;
mod excerpt;
mod graces;
mod order;
mod pitch;
mod player;
mod sequence;
mod smf;
mod sounding;
mod swing;
mod tempo;
mod timeline;

pub use channels::ChannelAllocator;
pub use click::count_in;
pub use event::{Channel, MidiEvent, controller};
pub use order::playback_order;
pub use player::{PlayError, PlayState, Player, Scheduler, TEMPO_SCALE_RANGE};
pub use sequence::{MeasureSpan, Sequence, SequenceTrack, TimedEvent, sound_events};
pub use smf::{SmfError, to_smf};
pub use tempo::{PPQ, TempoMap, micros_per_quarter};
pub use timeline::{Timeline, tempo_at};

// The integration tests sequence the owner's files with `gscore-io`; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;
