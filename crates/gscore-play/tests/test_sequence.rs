//! Sequencing the owner's test files, compared with Guitar Pro's own MIDI export of the same
//! songs (`testdata/basic/*.mid`), plus repeats, ties and the `.mid` writer.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{Song, TrackIndex};
use gscore_play::{MidiEvent, PPQ, Sequence, to_smf};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn open(relative: &str) -> Song {
    let bytes = std::fs::read(testdata(relative)).unwrap();
    gscore_io::import_guitar_pro_from_bytes(&bytes)
        .unwrap()
        .song
}

/// A parsed Standard MIDI File: resolution and, per track, `(tick, status, data)`.
struct Smf {
    ppq: u16,
    tracks: Vec<Vec<(u64, u8, Vec<u8>)>>,
}

/// A small SMF reader for the tests (running status, meta and sysex events skipped).
fn parse_smf(bytes: &[u8]) -> Smf {
    assert_eq!(&bytes[..4], b"MThd");
    let count = u16::from_be_bytes([bytes[10], bytes[11]]);
    let ppq = u16::from_be_bytes([bytes[12], bytes[13]]);
    let mut pos = 14;
    let mut tracks = Vec::new();
    let vlq = |data: &[u8], i: &mut usize| {
        let mut value = 0_u64;
        loop {
            let byte = data[*i];
            *i += 1;
            value = (value << 7) | u64::from(byte & 0x7F);
            if byte < 0x80 {
                return value;
            }
        }
    };
    for _ in 0..count {
        assert_eq!(&bytes[pos..pos + 4], b"MTrk");
        let len = u32::from_be_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let data = &bytes[pos + 8..pos + 8 + len];
        pos += 8 + len;
        let (mut i, mut tick, mut status) = (0, 0_u64, 0_u8);
        let mut events = Vec::new();
        while i < data.len() {
            tick += vlq(data, &mut i);
            if data[i] == 0xFF {
                i += 2;
                let len = vlq(data, &mut i) as usize;
                i += len;
                continue;
            }
            if data[i] == 0xF0 || data[i] == 0xF7 {
                i += 1;
                let len = vlq(data, &mut i) as usize;
                i += len;
                continue;
            }
            if data[i] & 0x80 != 0 {
                status = data[i];
                i += 1;
            }
            let n = if matches!(status & 0xF0, 0xC0 | 0xD0) {
                1
            } else {
                2
            };
            events.push((tick, status, data[i..i + n].to_vec()));
            i += n;
        }
        tracks.push(events);
    }
    Smf { ppq, tracks }
}

/// Note starts as `(time in 1/480 quarter notes, key)`, sorted, from a parsed file.
fn note_starts(smf: &Smf) -> Vec<(u64, u8)> {
    let mut starts: Vec<(u64, u8)> = smf
        .tracks
        .iter()
        .flatten()
        .filter(|(_, status, data)| status & 0xF0 == 0x90 && data[1] > 0)
        .map(|(tick, _, data)| (tick * 480 / u64::from(smf.ppq), data[0]))
        .collect();
    starts.sort_unstable();
    starts
}

fn our_note_starts(sequence: &Sequence) -> Vec<(u64, u8)> {
    let mut starts: Vec<(u64, u8)> = sequence
        .events
        .iter()
        .filter_map(|e| match e.event {
            MidiEvent::NoteOn { key, .. } => Some((e.tick * 480 / u64::from(PPQ), key)),
            _ => None,
        })
        .collect();
    starts.sort_unstable();
    starts
}

/// The same notes, allowing one tick (of 480 per quarter) of rounding: Guitar Pro
/// truncates tuplet positions where we round.
fn assert_same_notes(ours: &[(u64, u8)], theirs: &[(u64, u8)], what: &str) {
    assert_eq!(ours.len(), theirs.len(), "{what}: {ours:?}\n{theirs:?}");
    for (a, b) in ours.iter().zip(theirs) {
        assert!(
            a.1 == b.1 && a.0.abs_diff(b.0) <= 1,
            "{what}: {a:?} vs {b:?}"
        );
    }
}

#[test]
fn notes_and_programs_match_guitar_pros_midi_export() {
    // Including ornaments: ties are held, harmonics sound at their harmonic pitch.
    for name in ["steel_guitar_dadgad", "note_lengths", "ornaments"] {
        let song = open(&format!("basic/{name}.gp"));
        let sequence = Sequence::new(&song, None).unwrap();
        let reference = parse_smf(&std::fs::read(testdata(&format!("basic/{name}.mid"))).unwrap());
        assert_same_notes(&our_note_starts(&sequence), &note_starts(&reference), name);
        let program = reference.tracks[1]
            .iter()
            .find(|(_, status, _)| status & 0xF0 == 0xC0)
            .map(|(_, _, data)| data[0]);
        assert_eq!(Some(sequence.tracks[0].sound.program), program, "{name}");
        assert_eq!(sequence.skipped_notes, 0);
    }
}

#[test]
fn tied_notes_are_held_and_dead_notes_are_short() {
    // Ornaments, measure 1: a half note tied to a half note; measure 2: a dead note.
    let sequence = Sequence::new(&open("basic/ornaments.gp"), None).unwrap();
    let whole = 4 * u64::from(PPQ);
    let notes: Vec<(u64, MidiEvent)> = sequence
        .events
        .iter()
        .filter(|e| e.event.is_note() && e.tick <= whole + whole / 8)
        .map(|e| (e.tick, e.event))
        .collect();
    let channel = sequence.tracks[0].channel;
    let off = |key| MidiEvent::NoteOff { channel, key };
    assert!(matches!(notes[0], (0, MidiEvent::NoteOn { key: 40, .. })));
    assert_eq!(notes[1], (whole, off(40)), "held through the tie");
    assert!(matches!(notes[2], (t, MidiEvent::NoteOn { key: 40, .. }) if t == whole));
    assert_eq!(
        notes[3],
        (whole + whole / 32, off(40)),
        "dead notes are short"
    );
}

#[test]
fn every_note_on_has_a_later_note_off() {
    for file in [
        "basic/steel_guitar_dadgad.gp5",
        "basic/note_lengths.gpx",
        "basic/ornaments.gp",
    ] {
        let sequence = Sequence::new(&open(file), None).unwrap();
        let mut open_notes = std::collections::HashMap::new();
        let mut last_tick = 0;
        for event in &sequence.events {
            assert!(event.tick >= last_tick, "{file}: events are sorted");
            last_tick = event.tick;
            match event.event {
                MidiEvent::NoteOn { channel, key, .. } => {
                    *open_notes.entry((channel, key)).or_insert(0) += 1;
                }
                MidiEvent::NoteOff { channel, key } => {
                    *open_notes.entry((channel, key)).or_insert(0) -= 1;
                }
                _ => {}
            }
        }
        assert!(
            open_notes.values().all(|&n| n == 0),
            "{file}: {open_notes:?}"
        );
        assert!(last_tick <= sequence.end, "{file}");
    }
}

#[test]
fn repeats_play_measures_again_in_time() {
    // |: 1 2 :| 3 in 4/4: five bars of 3840 ticks.
    let song = testing::song_with_repeats(3, &[(0, true, -1, 0), (1, false, 1, 0)]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let measures: Vec<(usize, u64)> = sequence
        .measures
        .iter()
        .map(|m| (m.measure.get(), m.start))
        .collect();
    assert_eq!(
        measures,
        [(0, 0), (1, 3840), (0, 7680), (1, 11_520), (2, 15_360)]
    );
    assert_eq!(sequence.end, 19_200);
    assert_eq!(sequence.measure_at(8000).unwrap().measure.get(), 0);
    assert_eq!(sequence.measure_at(u64::MAX).unwrap().measure.get(), 2);
}

#[test]
fn tracks_get_their_own_channels_and_drums_channel_ten() {
    let song = testing::song(&[GUITAR_6, GUITAR_6, GUITAR_6], 1).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let channels: Vec<u8> = sequence.tracks.iter().map(|t| t.channel.get()).collect();
    assert_eq!(channels, [0, 1, 2]);
    let only_second = Sequence::new(&song, Some(&[TrackIndex::new(1)])).unwrap();
    assert_eq!(only_second.tracks.len(), 1);
    assert!(Sequence::new(&song, Some(&[TrackIndex::new(7)])).is_err());
}

#[test]
fn the_midi_file_reads_back_with_the_same_notes() {
    let song = open("basic/note_lengths.gp");
    let sequence = Sequence::new(&song, None).unwrap();
    let bytes = to_smf(&sequence, song.title()).unwrap();
    let smf = parse_smf(&bytes);
    assert_eq!(u32::from(smf.ppq), PPQ);
    assert_eq!(smf.tracks.len(), 2, "conductor plus one track");
    assert_eq!(note_starts(&smf), our_note_starts(&sequence));
}

#[test]
fn songs_without_tracks_or_measures_give_empty_sequences() {
    let empty = Sequence::new(&testing::empty(), None).unwrap();
    assert!(empty.events.is_empty() && empty.measures.is_empty());
    assert!(to_smf(&empty, "").is_ok());
    let no_measures = Sequence::new(&testing::song(&[GUITAR_6], 0).unwrap(), None).unwrap();
    assert_eq!(no_measures.end, 0);
}
