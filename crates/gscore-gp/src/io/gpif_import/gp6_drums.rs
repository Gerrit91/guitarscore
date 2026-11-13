//! GuitarScore addition: GP6 drum notes. Guitar Pro 6 stores a drum note as an element
//! of its drum kit (which drum) and a variation (how it is struck), with no fret; Guitar
//! Pro 7, and this crate, as the articulation number (General MIDI-like, 35 for the
//! kick). Until 2026-11-13 GP6 drum notes were read as fret 0: silent and drawn nowhere.
//!
//! The table is the one other Guitar Pro readers use for GP6's drum kit (elements 0-16,
//! variations 0-2). Checked against `testdata/basic/multi_staff.gpx` and its GP7 twin
//! `multi_staff.gp`, which Guitar Pro saved from the same song: every element and
//! variation that file uses gives the articulation the GP7 file has (kick, snare and side
//! stick, three hi-hats, two crashes, splash, ride bell, two toms).

/// The articulation of each element's variations (a missing variation plays the first).
const ARTICULATIONS: [[i16; 3]; 17] = [
    [35, 35, 35],    // 0: kick
    [38, 91, 37],    // 1: snare (hit, rim shot, side stick)
    [99, 100, 99],   // 2: cowbell low
    [56, 100, 56],   // 3: cowbell medium
    [102, 103, 102], // 4: cowbell high
    [43, 43, 43],    // 5: tom very low
    [45, 45, 45],    // 6: tom low
    [47, 47, 47],    // 7: tom medium
    [48, 48, 48],    // 8: tom high
    [50, 50, 50],    // 9: tom very high
    [42, 92, 46],    // 10: hi-hat (closed, half, open)
    [44, 44, 44],    // 11: pedal hi-hat
    [57, 98, 57],    // 12: crash medium
    [49, 97, 49],    // 13: crash high
    [55, 95, 55],    // 14: splash
    [51, 93, 127],   // 15: ride (middle, bell, edge)
    [52, 96, 52],    // 16: china
];

/// The string (GPIF's, 0-based) Guitar Pro 7 puts each element's notes on in its own
/// files (kick and ride 0, toms 1-2, snare 3, hi-hats 4, other cymbals 5), so a GP6 kit
/// spreads over the strings as a GP7 kit does: a later hit only cuts a ringing one of
/// the same kind.
const STRINGS: [i8; 17] = [0, 3, 2, 2, 2, 1, 1, 2, 2, 2, 4, 4, 5, 5, 5, 0, 5];

/// The articulation and GPIF string of element `element` struck as `variation`, or
/// `None` for an element GP6's kit does not have.
pub(super) fn articulation(element: i32, variation: i32) -> Option<(i16, i8)> {
    let index = usize::try_from(element).ok()?;
    let variations = ARTICULATIONS.get(index)?;
    let articulation = usize::try_from(variation)
        .ok()
        .and_then(|variation| variations.get(variation))
        .unwrap_or(&variations[0]);
    Some((*articulation, *STRINGS.get(index)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kit_elements_give_their_articulations() {
        assert_eq!(articulation(0, 0), Some((35, 0)));
        assert_eq!(articulation(1, 2), Some((37, 3)));
        assert_eq!(articulation(10, 2), Some((46, 4)));
        assert_eq!(articulation(15, 1), Some((93, 0)));
        // A variation the element lacks plays its first; an unknown element nothing.
        assert_eq!(articulation(5, 7), Some((43, 1)));
        assert_eq!(articulation(17, 0), None);
        assert_eq!(articulation(-1, 0), None);
    }

    /// The test song as Guitar Pro 6 and Guitar Pro 7 saved it.
    const GP6: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/basic/multi_staff.gpx"
    ));
    const GP7: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/basic/multi_staff.gp"
    ));

    /// The drum track's notes (articulations), in order.
    fn drums(bytes: &[u8], gp6: bool) -> Vec<i16> {
        let mut song = crate::io::Song::default();
        if gp6 {
            song.read_gpx(bytes).expect("reads");
        } else {
            song.read_gp(bytes).expect("reads");
        }
        let kit = song
            .tracks
            .iter()
            .find(|track| track.percussion_track)
            .expect("a drum track");
        kit.measures
            .iter()
            .flat_map(|measure| &measure.voices)
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
            .map(|note| note.value)
            .collect()
    }

    #[test]
    fn a_gp6_drum_track_plays_what_guitar_pro_7_saved_of_it() {
        let (gp6, gp7) = (drums(GP6, true), drums(GP7, false));
        assert_eq!(gp6.len(), gp7.len());
        assert!(
            gp6.iter().all(|&value| value > 0),
            "every drum note has a sound"
        );
        // One kick was changed (35 to 36) in the GP7 file after it was saved from GP6.
        let differing = gp6.iter().zip(&gp7).filter(|(a, b)| a != b).count();
        assert!(differing <= 1, "{differing} notes differ: {gp6:?} {gp7:?}");
    }

    #[test]
    fn a_drum_staff_is_written_with_the_percussion_clef() {
        use crate::io::gpif_export::SongGpifExportOps;
        let mut song = crate::io::Song::default();
        song.read_gp(GP7).expect("reads");
        let xml = song.write_gpif_xml().expect("writes");
        assert!(xml.contains("<Clef>Neutral</Clef>"));
    }
}
