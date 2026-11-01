//! Playback sequencing: turns the model into timed MIDI events, allocates MIDI channels,
//! writes Standard MIDI Files, and defines the [`Player`] trait that output backends
//! (`gscore-audio`) implement. No device crates and no clock here; backends pass the time
//! into the [`Scheduler`].
//!
//! Played: notes of all voices with exact timing, ties, dead notes (short and soft), the
//! tracks' instruments, volume and pan, tempo changes, repeats and alternative endings.
//! Not yet: bends, slides, vibrato, let ring, palm mute, grace notes, directions (D.C.,
//! D.S., coda), swing.

#![warn(clippy::arithmetic_side_effects)]

mod channels;
mod event;
mod order;
mod player;
mod sequence;
mod smf;
mod tempo;

pub use channels::ChannelAllocator;
pub use event::{Channel, MidiEvent, controller};
pub use order::playback_order;
pub use player::{PlayError, PlayState, Player, Scheduler, TEMPO_SCALE_RANGE};
pub use sequence::{MeasureSpan, Sequence, SequenceTrack, TimedEvent};
pub use smf::{SmfError, to_smf};
pub use tempo::{PPQ, TempoMap, micros_per_quarter};

// The integration tests sequence the owner's files with `gscore-io`; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;
