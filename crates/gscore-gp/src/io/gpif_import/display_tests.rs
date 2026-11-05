//! GuitarScore tests for the additions: the master volume and pan, and which staves each
//! track shows (`Content/PartConfiguration`), read from Guitar Pro 7 and written back.

use crate::io::Song;

/// A Guitar Pro 7 file, embedded at build time (the crate does no file IO).
const STEEL_GUITAR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/basic/steel_guitar_dadgad.gp"
));

fn steel_guitar() -> Song {
    let mut song = Song::default();
    song.read_gp(STEEL_GUITAR).expect("reads");
    song
}

#[test]
fn guitar_pro_7_master_mix_and_staves_are_read() {
    let song = steel_guitar();
    let (volume, pan) = song.master_volume_pan.expect("the file has a master mix");
    assert!((volume - 0.76).abs() < 1e-6 && (pan - 0.5).abs() < 1e-6);
    assert!(song.tracks[0].settings.notation && song.tracks[0].settings.tablature);
}

#[test]
fn master_mix_and_staves_survive_saving() {
    let mut song = steel_guitar();
    song.master_volume_pan = Some((0.25, 0.75));
    song.tracks[0].settings.notation = false;
    let mut reread = Song::default();
    reread
        .read_gp(&song.write_gp().expect("writes"))
        .expect("reads back");
    assert_eq!(reread.master_volume_pan, Some((0.25, 0.75)));
    assert!(!reread.tracks[0].settings.notation);
    assert!(reread.tracks[0].settings.tablature);
}

#[test]
fn reverb_and_chorus_sends_survive_saving_and_are_absent_when_unset() {
    let mut song = steel_guitar();
    let index = song.tracks[0].channel_index;
    assert_eq!(song.channels[index].reverb, 0, "GP7 has no send of its own");
    let untouched = song.write_gp().expect("writes");

    song.channels[index].reverb = 40;
    song.channels[index].chorus = 12;
    let mut reread = Song::default();
    reread
        .read_gp(&song.write_gp().expect("writes"))
        .expect("reads back");
    let channel = reread.channels[reread.tracks[0].channel_index];
    assert_eq!((channel.reverb, channel.chorus), (40, 12));

    // An untouched song stores no GuitarScore sends at all.
    let mut plain = Song::default();
    plain.read_gp(&untouched).expect("reads back");
    let channel = plain.channels[plain.tracks[0].channel_index];
    assert_eq!((channel.reverb, channel.chorus), (0, 0));
}

/// The multi-track test file, embedded at build time.
const MULTI_STAFF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/basic/multi_staff.gp"
));

/// `gp` with `edit` applied to its `Content/score.gpif`.
fn with_gpif(gp: &[u8], edit: impl Fn(&str) -> String) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(gp)).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("an entry");
        let name = entry.name().expect("a name").to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("readable");
        if name == "Content/score.gpif" {
            bytes = edit(&String::from_utf8(bytes).expect("text")).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .expect("writable");
        out.write_all(&bytes).expect("writable");
    }
    out.finish().expect("finished").into_inner()
}

/// A track with two staves (a piano's or synth's treble and bass) takes two places in each
/// master bar's list of bars: the tracks after it still read their own bars.
#[test]
fn a_track_with_two_staves_does_not_shift_the_tracks_after_it() {
    // The first track gets a second staff, whose bars are empty (-1) in every master bar.
    let two_staves = with_gpif(MULTI_STAFF, |gpif| {
        let staff_start = gpif.find("<Staff>").expect("a staff");
        let staff_end = gpif[staff_start..].find("</Staff>").expect("its end") + staff_start;
        let staff = &gpif[staff_start..staff_end + "</Staff>".len()];
        let mut edited = String::new();
        edited.push_str(&gpif[..staff_end + "</Staff>".len()]);
        edited.push_str(staff);
        let mut rest = &gpif[staff_end + "</Staff>".len()..];
        // Every master bar's list: the first track's bar, then the new staff's (none).
        while let Some(at) = rest.find("<Bars>") {
            let (before, after) = rest.split_at(at + "<Bars>".len());
            edited.push_str(before);
            let is_list = after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '-');
            if is_list {
                let end = after.find("</Bars>").expect("the list's end");
                let mut ids = after[..end].split_whitespace();
                let first = ids.next().expect("a bar");
                let others: Vec<&str> = ids.collect();
                edited.push_str(&format!("{first} -1 {}", others.join(" ")));
                rest = &after[end..];
            } else {
                rest = after;
            }
        }
        edited.push_str(rest);
        edited
    });
    let mut original = Song::default();
    original.read_gp(MULTI_STAFF).expect("reads");
    let mut song = Song::default();
    song.read_gp(&two_staves).expect("reads");
    assert_eq!(song.tracks.len(), original.tracks.len());
    let notes = |song: &Song, track: usize| -> Vec<usize> {
        song.tracks[track]
            .measures
            .iter()
            .map(|measure| {
                measure
                    .voices
                    .iter()
                    .flat_map(|voice| &voice.beats)
                    .map(|beat| beat.notes.len())
                    .sum()
            })
            .collect()
    };
    for track in 0..original.tracks.len() {
        assert_eq!(
            notes(&song, track),
            notes(&original, track),
            "track {track}"
        );
    }
}
