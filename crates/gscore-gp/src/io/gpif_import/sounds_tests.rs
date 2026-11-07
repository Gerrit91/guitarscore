//! Guitar Pro's own RSE sound choice (`<Sounds>` and `<AudioEngineState>`) survives a
//! load-save cycle. GuitarScore does not edit these parts yet, so they are kept verbatim.

use crate::io::Song;
use crate::io::gpif_export::SongGpifExportOps;

/// A Guitar Pro 7 file with RSE sounds, embedded at build time (the crate does no file IO).
const STAFF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../testdata/basic/staff.gp"
));

fn staff() -> Song {
    let mut song = Song::default();
    song.read_gp(STAFF).expect("reads");
    song
}

/// The verbatim elements of every track, in order.
fn kept(song: &Song) -> Vec<(Option<String>, Option<String>)> {
    song.tracks
        .iter()
        .map(|track| (track.gpif_sounds.clone(), track.gpif_audio_engine.clone()))
        .collect()
}

#[test]
fn guitar_pro_7_sounds_are_read() {
    let song = staff();
    let sounds: Vec<&str> = song
        .tracks
        .iter()
        .filter_map(|track| track.gpif_sounds.as_deref())
        .collect();
    assert!(!sounds.is_empty(), "the file has an RSE sound");
    assert!(
        sounds
            .iter()
            .any(|sounds| sounds.contains("Stringed/Electric Guitars")),
        "{sounds:?}"
    );
    // The whole element, including the parts GuitarScore does not understand.
    assert!(
        sounds
            .iter()
            .any(|sounds| sounds.contains("<SoundbankPatch>"))
    );
    assert!(
        song.tracks
            .iter()
            .any(|track| track.gpif_audio_engine.as_deref()
                == Some("<AudioEngineState>RSE</AudioEngineState>"))
    );
}

#[test]
fn guitar_pro_7_sounds_survive_saving() {
    let song = staff();
    let before = kept(&song);
    let bytes = song.write_gp().expect("writes");
    let mut back = Song::default();
    back.read_gp(&bytes).expect("reads back");
    assert_eq!(kept(&back), before);
    // The written XML holds the element exactly once per track that had one.
    let xml = back.write_gpif_xml().expect("writes the XML");
    let expected = before.iter().filter(|(sounds, _)| sounds.is_some()).count();
    assert_eq!(xml.matches("<Sounds>").count(), expected);
}
