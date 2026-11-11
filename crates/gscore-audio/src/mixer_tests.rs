//! The mixer's own bookkeeping: which strip a channel's controllers reach.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::sync::Arc;

use gscore_model::testing;
use gscore_play::{Channel, MidiEvent, Sequence, controller};

use super::Mixer;

#[test]
fn a_borrowed_channels_setup_never_moves_another_strip() {
    // Two tracks; a bent note of the first borrows channel 6, and the second's next bend
    // gets the same channel. That borrower's setup (its pan) comes before its note: it
    // used to land on the first track's strip, which then sat hard on the other side.
    let song = testing::song_with_notes_on_tracks(2, &[(1, &[])]).unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap());
    let mut mixer = Mixer::new(48_000);
    mixer.follow(&sequence);
    let borrowed = Channel::new(6).unwrap();
    let before = (mixer.strips[0].pan, mixer.strips[1].pan);
    assert!(mixer.take(
        &MidiEvent::NoteOn {
            channel: borrowed,
            key: 60,
            velocity: 100
        },
        Some(0)
    ));
    let pan = |value| MidiEvent::Control {
        channel: borrowed,
        controller: controller::PAN,
        value,
    };
    assert!(
        !mixer.take(&pan(0), Some(1)),
        "a strip's controller, not the synth's"
    );
    assert_eq!((mixer.strips[0].pan, mixer.strips[1].pan), before);
    // A track's own channel still sets its strip.
    let home = sequence.tracks[1].channel;
    let _ = mixer.take(
        &MidiEvent::Control {
            channel: home,
            controller: controller::PAN,
            value: 10,
        },
        Some(1),
    );
    assert_eq!(mixer.strips[1].pan, 10);
}
