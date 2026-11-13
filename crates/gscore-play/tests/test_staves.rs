//! A track's staves all play: a piano's treble and bass, each at its own staff's pitches,
//! held over each other (a note on one staff never stops a note on the other).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use gscore_model::testing;
use gscore_play::{MidiEvent, PPQ, Performance, Sequence};

#[test]
fn both_staves_of_a_piano_sound_at_their_own_pitches() {
    let song = testing::song_with_two_staves().unwrap();
    let performance = Performance::new(&song, None).unwrap();
    let mut piano: Vec<_> = performance
        .notes
        .iter()
        .filter(|note| note.track == 0)
        .collect();
    piano.sort_by_key(|note| note.key);
    // String 1 open on each staff: G2 on the bass staff, E4 on the treble staff.
    let keys: Vec<u8> = piano.iter().map(|note| note.key).collect();
    assert_eq!(keys, vec![43, 64]);
    // Both last the whole measure: neither stops the other, though both are on string 1
    // of their staff.
    let whole = 4 * u64::from(PPQ);
    for note in &piano {
        assert_eq!((note.start, note.end), (0, whole), "{note:?}");
    }
    assert_ne!(piano[0].string, piano[1].string);
}

#[test]
fn the_midi_sequence_plays_both_staves() {
    let song = testing::song_with_two_staves().unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let mut keys: Vec<u8> = sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOn {
                channel: on,
                key,
                velocity,
            } if on == channel && velocity > 0 => Some(key),
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec![43, 64]);
}
