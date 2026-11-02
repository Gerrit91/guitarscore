//! Tests of the mix table reading and writing.

use crate::io::Song;
use crate::model::{Beat, MixTableChange, MixTableItem, Voice};

#[test]
fn tempo_changes_above_255_bpm_survive_gp5() {
    // The reader used to clamp tempo changes to 255 BPM (a u8), losing fast tempos.
    let mut song = Song::default();
    song.tracks.push(Default::default());
    song.measure_headers.push(Default::default());
    let beat = Beat {
        effect: crate::model::BeatEffects {
            mix_table_change: Some(MixTableChange {
                tempo: Some(MixTableItem {
                    value: 280,
                    ..MixTableItem::default()
                }),
                ..MixTableChange::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let measure = crate::model::Measure {
        voices: vec![Voice {
            beats: vec![beat],
            ..Voice::default()
        }],
        ..Default::default()
    };
    song.tracks[0].measures.push(measure);

    let bytes = song.write((5, 1, 0), None).expect("written");
    let mut reread = Song::default();
    reread.read_gp5(&bytes).expect("read back");
    let tempo = reread.tracks[0].measures[0].voices[0].beats[0]
        .effect
        .mix_table_change
        .as_ref()
        .and_then(|m| m.tempo.as_ref())
        .map(|t| t.value);
    assert_eq!(tempo, Some(280));
}
