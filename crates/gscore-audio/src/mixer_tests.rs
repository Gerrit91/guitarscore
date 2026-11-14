//! The mixer's own bookkeeping: which strip a channel's controllers reach.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::sync::Arc;

use gscore_model::testing;
use gscore_play::{Channel, MidiEvent, Sequence, controller};

use super::{Bus, Mixer};

/// Solo keeps the buses: the soloed track is the only one in the sequence, and its strip
/// still carries its reverb and compressor sends, which the mixer takes after every strip
/// (the buses themselves know nothing of solo), so they keep playing its sends.
#[test]
fn a_soloed_track_keeps_its_sends_in_the_buses() {
    use gscore_model::TrackIndex;

    let mut song = testing::song(&[testing::GUITAR_6, testing::GUITAR_6], 1).unwrap();
    let soloed = TrackIndex::new(0);
    let _ = song.set_track_reverb(soloed, 100).unwrap();
    let _ = song.set_track_solo(soloed, true).unwrap();
    // The sound the mixer takes the sends from, as the engine is given it.
    let sequence = Arc::new(Sequence::new(&song, None).unwrap());
    assert_eq!(sequence.tracks.len(), 1, "only the soloed track plays");
    assert_eq!(sequence.tracks[0].sound.reverb, 100);

    let mut mixer = Mixer::new(48_000);
    mixer.follow(&sequence);
    mixer.set_comp_send(0, 100);
    // The strip the engine plays the soloed track's channel on still has its reverb send
    // (the compressor send is the mixer's own, above), and the buses are separate strips
    // of their own:
    assert_eq!(mixer.strips[0].song_track, 0);
    assert_eq!(mixer.strips[0].reverb, 100);
    assert_eq!(mixer.comp_sends[0], 100);
    // A busy board is no reason to silence a bus: none of the three is muted by solo.
    assert!(Bus::ALL.iter().all(|bus| !mixer.buses[bus.index()].mix.1));
}

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
