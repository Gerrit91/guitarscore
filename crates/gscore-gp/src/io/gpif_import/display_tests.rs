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
