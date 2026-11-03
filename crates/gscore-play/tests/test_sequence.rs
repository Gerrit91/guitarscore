//! Sequencing the owner's test files, compared with Guitar Pro's own MIDI export of the same
//! songs (`testdata/basic/*.mid`), plus repeats, ties, bends and slides on borrowed channels,
//! and the `.mid` writer.

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

use gscore_model::testing::{self, GUITAR_6, TestNote};
use gscore_model::{
    BeatIndex, BeatPosition, Feel, Fret, MeasureIndex, NoteValue, SlideKind, Song, StringNumber,
    TrackIndex, VoiceIndex,
};
use gscore_play::{Channel, MidiEvent, PPQ, Sequence, Timeline, to_smf};

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
fn bends_and_slides_in_a_real_song_use_borrowed_channels() {
    let sequence = Sequence::new(&open("basic/ornaments.gp"), None).unwrap();
    assert_eq!(sequence.degraded_pitch_bends, 0);
    let base = sequence.tracks[0].channel;
    let bends = pitch_bends(&sequence);
    assert!(bends.len() > 10, "ornaments has bends and slides");
    assert!(bends.iter().all(|(_, channel, _)| *channel != base));
    assert!(
        note_ons(&sequence)
            .iter()
            .any(|&(_, channel, _)| channel != base),
        "the bent and sliding notes move to their own channels"
    );
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
fn beats_map_to_the_tick_of_their_first_pass() {
    let at = |measure: usize, beat: usize| BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    };
    // Measures 1-2 repeat; every measure holds four quarter rests (960 ticks each).
    let song = testing::song_with_repeats(3, &[(0, true, -1, 0), (1, false, 1, 0)]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.tick_of(&song, at(0, 0)), Some(0));
    assert_eq!(sequence.tick_of(&song, at(1, 2)), Some(3840 + 2 * 960));
    // After the repeat: the second pass is skipped, measure 3 starts at 15360.
    assert_eq!(sequence.tick_of(&song, at(2, 1)), Some(15_360 + 960));
    // Past the last beat (a cursor in an empty measure): at most the measure's end.
    assert_eq!(sequence.tick_of(&song, at(2, 99)), Some(19_200));
    assert_eq!(sequence.tick_of(&song, at(3, 0)), None, "no such measure");

    // Mixed lengths: an eighth, then a quarter, then the third beat.
    let song = testing::song_with_notes(&[
        (8, &[TestNote::new(1, 0)][..]),
        (4, &[TestNote::new(1, 2)][..]),
        (4, &[TestNote::new(1, 3)][..]),
    ])
    .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.tick_of(&song, at(0, 2)), Some(480 + 960));
    // A track that does not exist still finds its measure's start.
    let mut other = at(0, 2);
    other.track = TrackIndex::new(7);
    assert_eq!(sequence.tick_of(&song, other), Some(0));
}

#[test]
fn a_tick_maps_back_to_the_beat_playing_there() {
    let at = |measure: usize, beat: usize| BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    };
    // Measures 1-2 repeat; every measure holds four quarter rests (960 ticks each).
    let song = testing::song_with_repeats(3, &[(0, true, -1, 0), (1, false, 1, 0)]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    // The repeat: measure 1 plays at 0 and again at 7680.
    assert_eq!(sequence.beat_at(&song, at(0, 0), 0), Some(at(0, 0)));
    assert_eq!(sequence.beat_at(&song, at(0, 0), 959), Some(at(0, 0)));
    assert_eq!(sequence.beat_at(&song, at(0, 0), 960), Some(at(0, 1)));
    assert_eq!(sequence.beat_at(&song, at(0, 0), 7680), Some(at(0, 0)));
    // Past the last beat: the last beat of the last measure.
    assert_eq!(sequence.beat_at(&song, at(0, 0), u64::MAX), Some(at(2, 3)));
    // A track that does not exist has no beats.
    let mut missing = at(0, 0);
    missing.track = TrackIndex::new(7);
    assert_eq!(sequence.beat_at(&song, missing, 0), None);
    // A measure without the cursor's voice (Guitar Pro stores an empty bar without any)
    // still takes the cursor: its first beat, so the cursor follows over empty bars.
    let mut voice = at(0, 0);
    voice.voice = VoiceIndex::new(3);
    let mut expected = at(2, 0);
    expected.voice = VoiceIndex::new(3);
    assert_eq!(
        sequence.beat_at(&song, voice, 15_360 + 1000),
        Some(expected)
    );
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

/// Note starts as `(tick, channel, key)`.
fn note_ons(sequence: &Sequence) -> Vec<(u64, Channel, u8)> {
    sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::NoteOn { channel, key, .. } => Some((event.tick, channel, key)),
            _ => None,
        })
        .collect()
}

/// Pitch bends as `(tick, channel, value)`.
fn pitch_bends(sequence: &Sequence) -> Vec<(u64, Channel, u16)> {
    sequence
        .events
        .iter()
        .filter_map(|event| match event.event {
            MidiEvent::PitchBend { channel, value } => Some((event.tick, channel, value)),
            _ => None,
        })
        .collect()
}

/// A whole-step bend (2 semitones) with the six-semitone range.
const WHOLE_STEP: u16 = 8192 + 2730;

#[test]
fn a_bend_borrows_a_channel_and_moves_the_pitch() {
    let song = testing::song_with_notes(&[(1, &[TestNote::new(6, 3).with_bend(4)])]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.degraded_pitch_bends, 0);

    let base = sequence.tracks[0].channel;
    let ons = note_ons(&sequence);
    assert_eq!(ons.len(), 1);
    let (start, channel, key) = ons[0];
    assert_eq!((start, key), (0, 43));
    assert_ne!(channel, base, "the bent note gets its own channel");
    assert!(sequence.channels().contains(&channel));

    // The borrowed channel gets the track's setup and the bend range.
    assert!(sequence.events.iter().any(|event| {
        event.tick == 0
            && matches!(event.event, MidiEvent::ProgramChange { channel: c, .. } if c == channel)
    }));
    assert!(sequence.events.iter().any(|event| {
        event.tick == 0
            && matches!(
                event.event,
                MidiEvent::Control {
                    channel: c,
                    controller: 101,
                    ..
                } if c == channel
            )
    }));

    let bends = pitch_bends(&sequence);
    assert!(
        bends
            .iter()
            .any(|(tick, c, value)| { *c == channel && *tick > 0 && *tick < 480 && *value > 8192 })
    );
    assert!(
        bends
            .iter()
            .any(|(_, c, value)| *c == channel && *value == WHOLE_STEP),
        "the bend reaches a whole step"
    );
    assert!(
        bends
            .iter()
            .any(|(tick, c, value)| *c == channel && *tick == sequence.end && *value == 8192),
        "the bend is reset after the note"
    );
}

#[test]
fn a_release_bend_starts_bent_and_returns_to_center() {
    let song =
        testing::song_with_notes(&[(1, &[TestNote::new(6, 3).with_release_bend(4)])]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let channel = note_ons(&sequence)[0].1;
    assert_ne!(channel, base);

    let bends = pitch_bends(&sequence);
    let starts: Vec<u16> = bends
        .iter()
        .filter(|(tick, c, _)| *tick == 0 && *c == channel)
        .map(|(_, _, value)| *value)
        .collect();
    assert!(starts.contains(&WHOLE_STEP), "starts bent: {starts:?}");
    assert!(
        bends
            .iter()
            .any(|(tick, c, value)| { *c == channel && *tick == sequence.end && *value == 8192 })
    );
}

#[test]
fn a_chord_bends_each_note_on_its_own_channel() {
    let song = testing::song_with_notes(&[(
        1,
        &[
            TestNote::new(6, 3).with_bend(4),
            TestNote::new(5, 3).with_bend(4),
        ],
    )])
    .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.degraded_pitch_bends, 0);
    let base = sequence.tracks[0].channel;
    let channels: Vec<Channel> = note_ons(&sequence).iter().map(|&(_, c, _)| c).collect();
    assert_eq!(channels.len(), 2);
    assert_ne!(channels[0], channels[1]);
    assert!(channels.iter().all(|&channel| channel != base));
    assert!(sequence.channels().contains(&channels[0]));
    assert!(sequence.channels().contains(&channels[1]));
}

#[test]
fn a_shift_slide_bends_to_the_next_note() {
    let song = testing::song_with_notes(&[
        (
            2,
            &[TestNote::new(6, 3).with_slides(&[SlideKind::ShiftSlideTo])],
        ),
        (2, &[TestNote::new(6, 5)]),
    ])
    .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.degraded_pitch_bends, 0);
    let base = sequence.tracks[0].channel;
    let ons = note_ons(&sequence);
    assert_eq!(ons.len(), 2);
    let (first_tick, slide_channel, first_key) = ons[0];
    let (second_tick, second_channel, second_key) = ons[1];
    assert_eq!((first_tick, first_key), (0, 43));
    assert_eq!((second_tick, second_key), (1920, 45));
    assert_ne!(slide_channel, base);
    assert_eq!(second_channel, base, "only the sliding note borrows");

    let bends = pitch_bends(&sequence);
    assert!(bends.iter().any(|(tick, c, value)| {
        *c == slide_channel && *tick > 0 && *tick < second_tick && *value == WHOLE_STEP
    }));
    assert!(
        bends.iter().any(|(tick, c, value)| {
            *c == slide_channel && *tick == second_tick && *value == 8192
        })
    );
}

#[test]
fn a_slide_in_starts_from_the_previous_pitch() {
    let song = testing::song_with_notes(&[
        (2, &[TestNote::new(6, 3)]),
        (
            2,
            &[TestNote::new(6, 5).with_slides(&[SlideKind::IntoFromBelow])],
        ),
    ])
    .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let base = sequence.tracks[0].channel;
    let ons = note_ons(&sequence);
    assert_eq!(ons.len(), 2);
    let (start, channel, key) = ons[1];
    assert_eq!((start, key), (1920, 45));
    assert_ne!(channel, base);

    let bends = pitch_bends(&sequence);
    let starts: Vec<u16> = bends
        .iter()
        .filter(|(tick, c, _)| *tick == start && *c == channel)
        .map(|(_, _, value)| *value)
        .collect();
    assert!(
        starts.contains(&(8192 - 2730)),
        "starts a whole step below: {starts:?}"
    );
    assert!(bends.iter().any(|(tick, c, value)| {
        *c == channel && *tick > start && *tick < sequence.end && *value == 8192
    }));
}

#[test]
fn bends_degrade_when_every_channel_is_in_use() {
    let bent = [TestNote::new(6, 3).with_bend(4)];
    let song = testing::song_with_notes_on_tracks(15, &[(1, &bent)]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.tracks.len(), 15);
    assert_eq!(sequence.degraded_pitch_bends, 15);
    let channels: Vec<Channel> = note_ons(&sequence).iter().map(|&(_, c, _)| c).collect();
    for (track, channel) in sequence.tracks.iter().zip(channels) {
        assert_eq!(
            track.channel, channel,
            "a degraded note stays on its track channel"
        );
    }
    assert!(pitch_bends(&sequence).is_empty());
}

#[test]
fn the_midi_file_contains_pitch_bends_and_the_range_setup() {
    let song = testing::song_with_notes(&[(1, &[TestNote::new(6, 3).with_bend(4)])]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let bytes = to_smf(&sequence, "bend").unwrap();
    let smf = parse_smf(&bytes);
    let mut bends = 0;
    let mut range_setup = 0;
    for track in &smf.tracks {
        for (_, status, data) in track {
            if status & 0xF0 == 0xE0 {
                bends += 1;
            }
            if status & 0xF0 == 0xB0 && data.first() == Some(&101) && data.get(1) == Some(&0) {
                range_setup += 1;
            }
        }
    }
    assert!(bends > 0, "pitch bends are written");
    assert!(range_setup > 0, "the borrowed channel gets a bend range");
}

#[test]
fn a_slide_wider_than_the_bend_range_widens_its_channel_range() {
    // Fret 5 to fret 12 on the low E: seven semitones, beyond the usual six.
    let song = testing::song_with_notes(&[
        (
            2,
            &[TestNote::new(6, 5).with_slides(&[SlideKind::ShiftSlideTo])],
        ),
        (2, &[TestNote::new(6, 12)]),
    ])
    .unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let (_, slide_channel, _) = note_ons(&sequence)[0];
    let range: Vec<u8> = sequence
        .events
        .iter()
        .filter_map(|e| match e.event {
            MidiEvent::Control {
                channel,
                controller: 6,
                value,
            } if channel == slide_channel => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(range, [7], "RPN 0,0 data entry sets seven semitones");
    let bends = pitch_bends(&sequence);
    assert!(
        bends
            .iter()
            .any(|(_, c, value)| *c == slide_channel && *value == 0x3FFF),
        "the slide reaches the target pitch: {bends:?}"
    );
}

#[test]
fn muted_tracks_do_not_play_and_solo_silences_the_rest() {
    let notes = [TestNote::new(6, 5)];
    let mut song = testing::song_with_notes_on_tracks(3, &[(4, &notes)]).unwrap();
    let tracks = |sequence: &Sequence| -> Vec<TrackIndex> {
        sequence.tracks.iter().map(|track| track.track).collect()
    };

    assert_eq!(
        tracks(&Sequence::new(&song, None).unwrap()),
        [TrackIndex::new(0), TrackIndex::new(1), TrackIndex::new(2)]
    );
    let _ = song.set_track_mute(TrackIndex::new(1), true).unwrap();
    assert_eq!(
        tracks(&Sequence::new(&song, None).unwrap()),
        [TrackIndex::new(0), TrackIndex::new(2)]
    );

    let _ = song.set_track_solo(TrackIndex::new(2), true).unwrap();
    let soloed = Sequence::new(&song, None).unwrap();
    assert_eq!(tracks(&soloed), [TrackIndex::new(2)]);
    assert!(
        soloed
            .events
            .iter()
            .all(|event| !matches!(event.event, MidiEvent::NoteOn { .. }) || event.track == 0),
        "no note comes from a silenced track"
    );

    // A soloed but muted track stays silent.
    let _ = song.set_track_mute(TrackIndex::new(2), true).unwrap();
    assert!(Sequence::new(&song, None).unwrap().tracks.is_empty());
}

#[test]
fn a_sequence_reports_the_channel_each_track_plays_on() {
    let notes = [TestNote::new(6, 5)];
    let mut song = testing::song_with_notes_on_tracks(2, &[(4, &notes)]).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    for entry in &sequence.tracks {
        assert_eq!(sequence.channel_of(entry.track), Some(entry.channel));
        assert_eq!(entry.channel, sequence.tracks[entry.track.get()].channel);
    }
    assert_eq!(sequence.channel_of(TrackIndex::new(9)), None);

    let _ = song.set_track_mute(TrackIndex::new(0), true).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    assert_eq!(sequence.channel_of(TrackIndex::new(0)), None);
    assert!(sequence.channel_of(TrackIndex::new(1)).is_some());
}

#[test]
fn swing_feel_moves_the_off_beats() {
    let mut song = testing::song(&[GUITAR_6], 1).unwrap();
    let at = |beat: usize| BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    };
    let string = StringNumber::new(1).unwrap();
    let fret = Fret::new(3).unwrap();
    let set_length = |song: &mut Song, value: u16| {
        let length = NoteValue::new(value, 0, None).unwrap();
        for beat in 0..4 {
            let _ = song.set_duration(at(beat), length).unwrap();
            let _ = song.set_note(at(beat), string, fret).unwrap();
        }
    };
    let starts = |song: &Song| {
        let sequence = Sequence::new(song, None).unwrap();
        let mut ticks: Vec<u64> = sequence
            .events
            .iter()
            .filter_map(|event| match event.event {
                MidiEvent::NoteOn { .. } => Some(event.tick),
                _ => None,
            })
            .collect();
        ticks.sort_unstable();
        ticks
    };

    // Four eighth notes: straight, then two triplet pairs.
    set_length(&mut song, 8);
    assert_eq!(starts(&song), vec![0, 480, 960, 1440]);
    let _ = song.set_feel(MeasureIndex::new(0), Feel::Eighth).unwrap();
    assert_eq!(starts(&song), vec![0, 640, 960, 1600]);

    // The cursor and the time readout use the same warped positions.
    let sequence = Sequence::new(&song, None).unwrap();
    let timeline = Timeline::new(&song).unwrap();
    assert_eq!(sequence.tick_of(&song, at(1)), Some(640));
    assert_eq!(timeline.tick_of(&song, at(1)), Some(640));
    assert_eq!(sequence.beat_at(&song, at(1), 650), Some(at(1)));

    // A dotted feel uses a three-to-one pair.
    let _ = song
        .set_feel(MeasureIndex::new(0), Feel::DottedEighth)
        .unwrap();
    assert_eq!(starts(&song), vec![0, 720, 960, 1680]);

    // Sixteenths swing inside each eighth.
    set_length(&mut song, 16);
    let _ = song
        .set_feel(MeasureIndex::new(0), Feel::Sixteenth)
        .unwrap();
    assert_eq!(starts(&song), vec![0, 320, 480, 800]);
}

#[test]
fn the_setup_sends_each_tracks_reverb_and_chorus() {
    let mut song = testing::song(&[GUITAR_6], 1).unwrap();
    let track = TrackIndex::new(0);
    let _reverb = song.set_track_reverb(track, 64).unwrap();
    let _chorus = song.set_track_chorus(track, 20).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.channel_of(track).unwrap();
    let setup: Vec<MidiEvent> = sequence
        .events
        .iter()
        .take_while(|event| event.tick == 0)
        .map(|event| event.event)
        .collect();
    for (controller, value) in [
        (gscore_play::controller::REVERB, 64),
        (gscore_play::controller::CHORUS, 20),
    ] {
        assert!(
            setup.contains(&MidiEvent::Control {
                channel,
                controller,
                value
            }),
            "controller {controller} = {value} in {setup:?}"
        );
    }
}
