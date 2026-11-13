//! GuitarScore tests: a track's second staff (a piano's bass) is read with its own tuning
//! and written back unchanged; until 2026-11-13 saving dropped it.

// The XML edit's offsets come from `find` on the same text.
#![allow(clippy::arithmetic_side_effects)]

use super::display_tests::{MULTI_STAFF, with_gpif};
use crate::io::Song;
use crate::io::gpif_export::SongGpifExportOps;
use crate::model::Measure;

/// The stand-in tuning of the second staff (a five-string bass, lowest first, as GPIF
/// lists it), distinct from the first staff's.
const BASS_PITCHES: &str = "23 28 33 38 43";

/// `multi_staff.gp` with a second staff on the first track: its own bass tuning, and the
/// first staff's bars once more as its bars (so it has notes).
fn two_staves() -> Vec<u8> {
    with_gpif(MULTI_STAFF, |gpif| {
        let staff_start = gpif.find("<Staff>").expect("a staff");
        let staff_end = gpif[staff_start..].find("</Staff>").expect("its end") + staff_start;
        let end = staff_end + "</Staff>".len();
        let staff = &gpif[staff_start..end];
        let pitches_start = staff.find("<Pitches>").expect("a tuning") + "<Pitches>".len();
        let pitches_end = staff.find("</Pitches>").expect("its end");
        let bass = format!(
            "{}{BASS_PITCHES}{}",
            &staff[..pitches_start],
            &staff[pitches_end..]
        );
        let mut edited = format!("{}{bass}", &gpif[..end]);
        let mut rest = &gpif[end..];
        // Every master bar's list: the first track's bar twice (both staves), then the rest.
        while let Some(at) = rest.find("<Bars>") {
            let (before, after) = rest.split_at(at + "<Bars>".len());
            edited.push_str(before);
            if after.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
                let list_end = after.find("</Bars>").expect("the list's end");
                let mut ids = after[..list_end].split_whitespace();
                let first = ids.next().expect("a bar");
                let others: Vec<&str> = ids.collect();
                edited.push_str(&format!("{first} {first} {}", others.join(" ")));
                rest = &after[list_end..];
            } else {
                rest = after;
            }
        }
        edited.push_str(rest);
        edited
    })
}

fn read(bytes: &[u8]) -> Song {
    let mut song = Song::default();
    song.read_gp(bytes).expect("reads");
    song
}

/// Every note of `measures` as (measure, string, fret), in order.
fn notes(measures: &[Measure]) -> Vec<(usize, i8, i16)> {
    measures
        .iter()
        .enumerate()
        .flat_map(|(index, measure)| {
            measure
                .voices
                .iter()
                .flat_map(|voice| &voice.beats)
                .flat_map(move |beat| {
                    beat.notes
                        .iter()
                        .map(move |note| (index, note.string, note.value))
                })
        })
        .collect()
}

#[test]
fn a_second_staff_is_read_with_its_own_tuning() {
    let original = read(MULTI_STAFF);
    let song = read(&two_staves());
    assert_eq!(song.tracks.len(), original.tracks.len());
    let track = &song.tracks[0];
    assert_eq!(track.staves.len(), 1, "one staff after the first");
    let staff = &track.staves[0];
    assert_eq!(staff.measures.len(), song.measure_headers.len());
    // The bass tuning, highest string first as the model keeps it.
    let pitches: Vec<i8> = staff.strings.iter().map(|&(_, pitch)| pitch).collect();
    assert_eq!(pitches, vec![43, 38, 33, 28, 23]);
    assert!(staff.has_tuning);
    // Its bars are the first staff's (by the edit above): the same notes, by string from
    // the top of its own five strings.
    assert!(!notes(&staff.measures).is_empty());
    assert_eq!(notes(&staff.measures).len(), notes(&track.measures).len());
    assert_eq!(notes(&track.measures), notes(&original.tracks[0].measures));
    // The tracks after it are untouched.
    for index in 1..song.tracks.len() {
        assert_eq!(
            notes(&song.tracks[index].measures),
            notes(&original.tracks[index].measures)
        );
        assert!(song.tracks[index].staves.is_empty());
    }
}

#[test]
fn a_second_staff_survives_saving() {
    let song = read(&two_staves());
    let reread = read(&song.write_gp().expect("writes"));
    let (track, again) = (&song.tracks[0], &reread.tracks[0]);
    assert_eq!(again.staves.len(), 1);
    assert_eq!(again.staves[0].strings, track.staves[0].strings);
    assert_eq!(
        notes(&again.staves[0].measures),
        notes(&track.staves[0].measures)
    );
    assert_eq!(notes(&again.measures), notes(&track.measures));
    let clefs = |measures: &[Measure]| -> Vec<_> {
        measures
            .iter()
            .map(|measure| measure.clef.clone())
            .collect()
    };
    assert_eq!(
        clefs(&again.staves[0].measures),
        clefs(&track.staves[0].measures)
    );
    for index in 1..song.tracks.len() {
        assert_eq!(
            notes(&reread.tracks[index].measures),
            notes(&song.tracks[index].measures),
            "track {index}"
        );
    }
}

#[test]
fn a_one_staff_song_is_written_as_before() {
    // No `<Staves>` for tracks with one staff: the GP6-style track properties stay.
    let song = read(MULTI_STAFF);
    let written = song.write_gpif_xml().expect("writes");
    assert!(!written.contains("<Staves>"));
}
