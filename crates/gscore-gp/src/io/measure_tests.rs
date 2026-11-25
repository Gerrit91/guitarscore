//! Reading GP3-5 measures.

use crate::io::Song;
use crate::model::{KeySignature, Measure, MeasureHeader, TimeSignature, Track, Voice};

/// GP3-5 store signatures only in the measure headers. Every track's measure must carry a
/// copy, as the GP6/GP7 import gives it: layout, playback and saving read the measure's
/// copy, which used to stay 4/4 in C major, so a 6/8 song showed and played as 4/4 and
/// saving it as GP7 reported every bar as changed.
#[test]
fn gp3_to_gp5_measures_carry_their_headers_signatures() {
    let signatures = [(3, 4), (6, 8)];
    let mut song = Song {
        tracks: vec![Track::default(), Track::default()],
        ..Default::default()
    };
    for (index, (numerator, denominator)) in signatures.into_iter().enumerate() {
        let mut header = MeasureHeader {
            number: u16::try_from(index + 1).unwrap(),
            key_signature: KeySignature {
                key: 2,
                is_minor: true,
            },
            ..Default::default()
        };
        header.time_signature.numerator = numerator;
        header.time_signature.denominator.value = denominator;
        song.measure_headers.push(header);
        for track in &mut song.tracks {
            track.measures.push(Measure {
                header_index: index,
                voices: vec![Voice::default(), Voice::default()],
                ..Default::default()
            });
        }
    }

    for version in [(3, 0, 0), (4, 0, 6), (5, 1, 0)] {
        let bytes = song.write(version, None).expect("written");
        let mut reread = Song::default();
        match version.0 {
            3 => reread.read_gp3(&bytes),
            4 => reread.read_gp4(&bytes),
            _ => reread.read_gp5(&bytes),
        }
        .expect("read back");
        for track in &reread.tracks {
            let read: Vec<(&TimeSignature, &KeySignature)> = track
                .measures
                .iter()
                .map(|m| (&m.time_signature, &m.key_signature))
                .collect();
            let headers: Vec<(&TimeSignature, &KeySignature)> = reread
                .measure_headers
                .iter()
                .map(|h| (&h.time_signature, &h.key_signature))
                .collect();
            assert_eq!(read, headers, "version {version:?}");
            let shown: Vec<_> = read
                .iter()
                .map(|(t, _)| (t.numerator, t.denominator.value))
                .collect();
            assert_eq!(shown, [(3, 4), (6, 8)], "version {version:?}");
        }
    }
}
