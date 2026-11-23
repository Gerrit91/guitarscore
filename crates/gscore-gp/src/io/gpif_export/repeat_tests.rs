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

/// A song of `count` empty 4/4 measures on one track.
fn empty_song(count: u16) -> Song {
    let mut song = Song::default();
    song.tracks.push(Default::default());
    for (index, number) in (0..count).zip(1..) {
        song.measure_headers.push(crate::model::MeasureHeader {
            number,
            ..Default::default()
        });
        song.tracks[0].measures.push(crate::model::Measure {
            header_index: usize::from(index),
            ..Default::default()
        });
    }
    song
}

/// A bar may hold several navigation signs; codas, segnos and fine are `<Target>`s and
/// D.C./D.S./to coda `<Jump>`s. Every sign used to be written as a jump, and only one sign
/// per bar was kept (two targets made the file fail to open).
#[test]
fn several_directions_in_one_bar_round_trip_through_gpif() {
    use crate::model::DirectionSign as D;
    let mut song = empty_song(3);
    song.measure_headers[0].directions = vec![D::Segno, D::Coda];
    song.measure_headers[1].directions = vec![D::Fine, D::DaCoda];
    song.measure_headers[2].directions = vec![D::DaSegnoAlCoda];

    let xml = song.write_gpif_xml().expect("written");
    assert!(
        xml.contains("<Directions><Target>Segno</Target><Target>Coda</Target></Directions>"),
        "{xml}"
    );
    assert!(
        xml.contains("<Directions><Target>Fine</Target><Jump>DaCoda</Jump></Directions>"),
        "{xml}"
    );

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let read: Vec<_> = reread
        .measure_headers
        .iter()
        .map(|header| header.directions.clone())
        .collect();
    assert_eq!(
        read,
        [
            vec![D::Segno, D::Coda],
            vec![D::Fine, D::DaCoda],
            vec![D::DaSegnoAlCoda]
        ]
    );
}

/// GP5 stores where each sign is; two signs at one measure are both read back.
#[test]
fn several_directions_in_one_measure_round_trip_through_gp5() {
    use crate::model::DirectionSign as D;
    let mut song = empty_song(2);
    song.measure_headers[1].directions = vec![D::Segno, D::Fine, D::DaCapoAlFine];
    let bytes = song.write((5, 1, 0), None).expect("written");
    let mut reread = Song::default();
    reread.read_gp5(&bytes).expect("read back");
    assert!(reread.measure_headers[0].directions.is_empty());
    assert_eq!(
        reread.measure_headers[1].directions,
        [D::Segno, D::Fine, D::DaCapoAlFine]
    );
}

/// The first beat of the first track.
fn first_beat(song: &mut Song) -> &mut crate::model::Beat {
    song.tracks[0].measures[0].voices[0]
        .beats
        .first_mut()
        .expect("a beat")
}

/// What a beat holds that nothing uses yet (hairpin, wah, variation, a held or flat
/// whammy, a rasgueado's pattern) and a section's letter survive a GP7 round trip.
#[test]
fn kept_beat_elements_and_section_letters_round_trip_through_gpif() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/basic/note_lengths.gp");
    let mut song = Song::default();
    song.read_gp(&std::fs::read(path).expect("the test file"))
        .expect("read");
    let kept = crate::model::GpifBeatKept {
        hairpin: Some("Crescendo".to_owned()),
        wah: Some("Open".to_owned()),
        variation: Some("2".to_owned()),
        whammy_extend: true,
        flat_whammy: true,
        rasgueado: Some("amii_1".to_owned()),
    };
    first_beat(&mut song).kept = kept.clone();
    first_beat(&mut song).effect.has_rasgueado = true;
    song.measure_headers[0].marker = Some(crate::model::Marker {
        title: "Intro".to_owned(),
        letter: Some("A".to_owned()),
        ..Default::default()
    });

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(first_beat(&mut reread).kept, kept);
    let marker = reread.measure_headers[0]
        .marker
        .clone()
        .expect("the section");
    assert_eq!(
        (marker.title.as_str(), marker.letter.as_deref()),
        ("Intro", Some("A"))
    );
}
