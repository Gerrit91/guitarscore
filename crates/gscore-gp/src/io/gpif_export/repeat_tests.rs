//! Repeat counts between GPIF (plays) and the model (repeats).

use super::SongGpifExportOps;
use crate::io::Song;

/// A plain repeat (one jump back) is `count="2"` in GPIF, which counts plays; reading
/// it back gives one repeat again. Both directions used to pass the number through,
/// so GP7 repeats played once too often and GP5 <-> GP7 conversion shifted the count.
#[test]
fn repeat_counts_convert_between_repeats_and_plays() {
    let mut song = Song::default();
    song.tracks.push(Default::default());
    for index in 0..2 {
        let header = crate::model::MeasureHeader {
            number: index + 1,
            ..Default::default()
        };
        song.measure_headers.push(header);
        song.tracks[0].measures.push(crate::model::Measure {
            header_index: usize::from(index),
            ..Default::default()
        });
    }
    song.measure_headers[0].repeat_open = true;
    song.measure_headers[1].repeat_close = 1;

    let xml = song.write_gpif_xml().expect("written");
    assert!(xml.contains("end=\"true\" count=\"2\""), "{xml}");

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert!(reread.measure_headers[0].repeat_open);
    assert_eq!(reread.measure_headers[1].repeat_close, 1);
}
