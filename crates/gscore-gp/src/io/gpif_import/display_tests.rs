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
