//! Regression tests for vibrato and legato, which GP7/GPX and GP3-5 store differently.

use std::collections::HashMap;

use crate::io::gpif::{Beat as GpifBeat, BeatPropertiesWrapper, BeatProperty, LegatoInfo};

use super::effect_tests::note_with;

/// GPIF names the vibrato kind; the model used to keep only "some vibrato", so a wide
/// vibrato came back as slight after a save.
#[test]
fn wide_vibrato_survives_reading_and_writing() {
    use crate::io::Song;

    let mut note = note_with("Fret");
    if let Some(property) = note.properties.properties.first_mut() {
        property.fret = Some(7);
    }
    note.vibrato = Some("Wide".to_owned());
    let song_note = super::convert_note(&note, 80, false, false);
    assert!(song_note.effect.vibrato);
    assert!(song_note.effect.wide_vibrato);

    let mut song = Song {
        tracks: vec![crate::model::Track::default()],
        measure_headers: vec![Default::default()],
        ..Song::default()
    };
    let mut beat = crate::model::Beat::default();
    beat.notes.push(crate::model::Note {
        string: 1,
        value: 7,
        kind: crate::model::NoteType::Normal,
        ..Default::default()
    });
    beat.notes[0].effect.vibrato = true;
    beat.notes[0].effect.wide_vibrato = true;
    song.tracks[0].measures = vec![crate::model::Measure {
        voices: vec![crate::model::Voice {
            beats: vec![beat],
            ..Default::default()
        }],
        ..Default::default()
    }];
    let bytes = song.write_gp().expect("write");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    let effect = &reread.tracks[0].measures[0].voices[0].beats[0].notes[0].effect;
    assert!(effect.vibrato);
    assert!(effect.wide_vibrato);
}

/// Guitar Pro writes a beat's own vibrato as the `VibratoWTremBar` property; the
/// reader used to ignore it, so the wavy line over notes 11 and 13 in `ornaments` was
/// missing. The strength (Slight/Wide) must survive a save.
#[test]
fn beat_vibrato_survives_reading_and_writing() {
    use crate::io::Song;

    let property = BeatProperty {
        name: "VibratoWTremBar".to_owned(),
        direction: None,
        enable: None,
        float: None,
        flags: None,
        strength: Some("Wide".to_owned()),
        duration: None,
        text: None,
    };
    let beat = GpifBeat {
        id: 0,
        notes: None,
        rhythm: None,
        dynamic: None,
        grace_notes: None,
        fadding: None,
        tremolo: None,
        whammy: None,
        wah: None,
        legato: None,
        free_text: None,
        lyrics: None,
        ottavia: None,
        chord: None,
        arpeggio: None,
        dead_slapped: None,
        properties: Some(BeatPropertiesWrapper {
            properties: vec![property],
        }),
    };
    let mut velocity = 0;
    let song_beat = super::convert_beat(&beat, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert!(song_beat.effect.vibrato);
    assert!(song_beat.effect.wide_vibrato);

    let mut song = Song {
        tracks: vec![crate::model::Track::default()],
        measure_headers: vec![Default::default()],
        ..Song::default()
    };
    let mut beat = crate::model::Beat::default();
    beat.effect.vibrato = true;
    beat.effect.wide_vibrato = true;
    song.tracks[0].measures = vec![crate::model::Measure {
        voices: vec![crate::model::Voice {
            beats: vec![beat],
            ..Default::default()
        }],
        ..Default::default()
    }];
    let bytes = song.write_gp().expect("write");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    let effect = &reread.tracks[0].measures[0].voices[0].beats[0].effect;
    assert!(effect.vibrato);
    assert!(effect.wide_vibrato);
}

/// A beat-level legato run is not a slide or a hammer-on; GPIF stores it in
/// `<Legato>`, which the reader and writer used to drop.
#[test]
fn beat_legato_survives_reading_and_writing() {
    use crate::io::Song;
    use crate::model::LegatoEffect;

    let beat = GpifBeat {
        id: 0,
        notes: None,
        rhythm: None,
        dynamic: None,
        grace_notes: None,
        fadding: None,
        tremolo: None,
        whammy: None,
        wah: None,
        legato: Some(LegatoInfo {
            origin: "true".to_owned(),
            destination: "false".to_owned(),
        }),
        free_text: None,
        lyrics: None,
        ottavia: None,
        chord: None,
        arpeggio: None,
        dead_slapped: None,
        properties: None,
    };
    let mut velocity = 0;
    let song_beat = super::convert_beat(&beat, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert_eq!(
        song_beat.effect.legato,
        LegatoEffect {
            origin: true,
            destination: false,
        }
    );

    let mut song = Song {
        tracks: vec![crate::model::Track::default()],
        measure_headers: vec![Default::default()],
        ..Song::default()
    };
    let mut beat = crate::model::Beat::default();
    beat.effect.legato = LegatoEffect {
        origin: true,
        destination: true,
    };
    song.tracks[0].measures = vec![crate::model::Measure {
        voices: vec![crate::model::Voice {
            beats: vec![beat],
            ..Default::default()
        }],
        ..Default::default()
    }];
    let bytes = song.write_gp().expect("write");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    assert_eq!(
        reread.tracks[0].measures[0].voices[0].beats[0]
            .effect
            .legato,
        LegatoEffect {
            origin: true,
            destination: true,
        }
    );
}
