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

/// Every feel survives a GP7 round trip through GPIF's `<TripletFeel>`.
#[test]
fn feels_round_trip_through_gpif() {
    let mut song = Song::default();
    song.tracks.push(Default::default());
    let feels = [
        crate::model::TripletFeel::DottedEighth,
        crate::model::TripletFeel::Eighth,
        crate::model::TripletFeel::ScottishSixteenth,
        crate::model::TripletFeel::None,
    ];
    for index in 0..4 {
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
    for (header, feel) in song.measure_headers.iter_mut().zip(&feels) {
        header.triplet_feel = feel.clone();
    }

    let xml = song.write_gpif_xml().expect("written");
    assert!(
        xml.contains("<TripletFeel>Dotted8th</TripletFeel>"),
        "{xml}"
    );
    assert!(
        xml.contains("<TripletFeel>Triplet8th</TripletFeel>"),
        "{xml}"
    );
    assert_eq!(
        xml.matches("<TripletFeel>").count(),
        3,
        "straight writes none"
    );

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let reread_feels: Vec<_> = reread
        .measure_headers
        .iter()
        .map(|header| header.triplet_feel.clone())
        .collect();
    assert_eq!(reread_feels, feels);
}
