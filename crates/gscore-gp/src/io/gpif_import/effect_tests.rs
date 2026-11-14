//! Regression tests for effects that GP7/GPX and GP3-5 store differently: hammers,
//! harmonics, bends and whammy.

use std::collections::HashMap;

use crate::io::gpif::{
    Beat as GpifBeat, BeatPropertiesWrapper, BeatProperty, EnableTag, Note, NoteProperties,
    Property, Whammy,
};

pub(super) fn note_with(property: &str) -> Note {
    Note {
        id: 0,
        properties: NoteProperties {
            properties: vec![Property {
                name: property.to_owned(),
                fret: None,
                string: None,
                pitch: None,
                number: None,
                enable: Some(EnableTag),
                float: None,
                flags: None,
                hfret: None,
                htype: None,
                pitches: None,
                label: None,
                text: None,
                direction: None,
                items: None,
                element: None,
                variation: None,
            }],
        },
        tie: None,
        vibrato: None,
        let_ring: None,
        anti_accent: None,
        accent: None,
        trill: None,
        ornament: None,
        left_fingering: None,
        right_fingering: None,
    }
}

/// A GPIF beat with nothing set.
fn beat() -> GpifBeat {
    GpifBeat {
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
        properties: None,
        x_properties: None,
    }
}

/// GuitarScore additions: a rolled chord, a dead slapped beat, the three fades and the
/// beat text all come from GPIF elements the old reader dropped.
#[test]
fn advanced_beat_effects_are_imported() {
    let mut rolled = beat();
    rolled.arpeggio = Some("Down".to_owned());
    rolled.dead_slapped = Some(EnableTag);
    rolled.free_text = Some("Dead slapped".to_owned());
    rolled.fadding = Some("FadeOut".to_owned());
    let mut velocity = 0;
    let song_beat = super::convert_beat(&rolled, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert_eq!(
        song_beat.effect.arpeggio,
        crate::model::BeatStrokeDirection::Down
    );
    assert!(song_beat.effect.dead_slapped);
    assert_eq!(song_beat.text, "Dead slapped");
    assert!(song_beat.effect.fade_out);

    let mut swell = beat();
    swell.fadding = Some("VolumeSwell".to_owned());
    let song_beat = super::convert_beat(&swell, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert!(song_beat.effect.volume_swell);

    let mut fade_in = beat();
    fade_in.fadding = Some("FadeIn".to_owned());
    let song_beat = super::convert_beat(&fade_in, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert!(song_beat.effect.fade_in);
}

/// GPIF writes a tremolo picking speed as the beat's `<Tremolo>`.
#[test]
fn tremolo_picking_speed_is_read_from_the_beat() {
    assert_eq!(super::beats::tremolo_speed("1/2"), Some(8));
    assert_eq!(super::beats::tremolo_speed("1/4"), Some(16));
    assert_eq!(super::beats::tremolo_speed("1/8"), Some(32));
    assert_eq!(super::beats::tremolo_speed("nonsense"), None);
}

/// Tapping, string numbers and fingering are note properties the old reader dropped.
#[test]
fn advanced_note_effects_are_imported() {
    let tapped = super::convert_note(&note_with("LeftHandTapped"), 80, false, false);
    assert!(tapped.effect.left_hand_tapped);
    let numbered = super::convert_note(&note_with("ShowStringNumber"), 80, false, false);
    assert!(numbered.effect.show_string_number);

    let mut fingered = note_with("LeftHandTapped");
    fingered.left_fingering = Some("C".to_owned());
    fingered.right_fingering = Some("P".to_owned());
    let song_note = super::convert_note(&fingered, 80, false, false);
    assert_eq!(
        song_note.effect.left_hand_finger,
        crate::model::Fingering::Little
    );
    assert_eq!(
        song_note.effect.right_hand_finger,
        crate::model::Fingering::Thumb
    );
}

/// GPIF slide flags 64 and 128 are pick slides, not ordinary slides.
#[test]
fn pick_slide_flags_are_imported() {
    let slide = |flags: i32| {
        let mut note = note_with("Slide");
        if let Some(property) = note.properties.properties.first_mut() {
            property.enable = None;
            property.flags = Some(flags);
        }
        super::convert_note(&note, 80, false, false)
    };
    assert!(
        slide(64)
            .effect
            .slides
            .contains(&crate::model::SlideType::PickSlideDown)
    );
    assert!(
        slide(128)
            .effect
            .slides
            .contains(&crate::model::SlideType::PickSlideUp)
    );
}

/// GPIF stores a trill's other note as a MIDI pitch; it becomes a fret on import.
#[test]
fn trill_pitch_becomes_a_fret() {
    use crate::model::{Beat, Measure, Note, NoteType, TrillEffect, Voice};
    let mut note = Note {
        string: 3,
        value: 2,
        kind: NoteType::Normal,
        ..Note::default()
    };
    note.effect.trill = Some(TrillEffect {
        fret: 47,
        duration: Default::default(),
    });
    let mut track = crate::model::Track {
        strings: vec![(3, 43)],
        ..crate::model::Track::default()
    };
    track.measures = vec![Measure {
        voices: vec![Voice {
            beats: vec![Beat {
                notes: vec![note],
                ..Beat::default()
            }],
            ..Voice::default()
        }],
        ..Measure::default()
    }];
    super::convert_trills(std::slice::from_mut(&mut track), false);
    assert_eq!(
        track.measures[0].voices[0].beats[0].notes[0]
            .effect
            .trill
            .as_ref()
            .map(|trill| trill.fret),
        Some(4),
        "47 on a G string is fret 4"
    );
}

/// A grace note with `HopoOrigin` has a hammer transition, so the main note is not picked.
#[test]
fn a_grace_note_keeps_its_hammer_transition() {
    let song_note = super::convert_note(&note_with("HopoOrigin"), 80, true, false);
    let grace = song_note.effect.grace.expect("grace note imported");
    assert_eq!(
        grace.transition,
        crate::model::GraceEffectTransition::Hammer
    );
}

/// GPIF has both `HopoOrigin` and `HopoDestination`; the model's single `hammer` flag
/// means "origin", as in GP3-5, so a dump prints one `h` per hammer/pull pair.
#[test]
fn hammer_is_kept_on_the_origin_only() {
    let origin = super::convert_note(&note_with("HopoOrigin"), 80, false, false);
    assert!(origin.effect.hammer);
    let destination = super::convert_note(&note_with("HopoDestination"), 80, false, false);
    assert!(!destination.effect.hammer);
}

/// Guitar Pro writes `HarmonicFret` before `HarmonicType`; the reader used to drop the
/// fret because it applied it while the harmonic did not exist yet.
#[test]
fn harmonic_fret_is_read_before_the_harmonic_type() {
    let mut note = note_with("HarmonicFret");
    if let Some(property) = note.properties.properties.first_mut() {
        property.hfret = Some(12.0);
    }
    note.properties.properties.push(Property {
        name: "HarmonicType".to_owned(),
        fret: None,
        string: None,
        pitch: None,
        number: None,
        enable: None,
        float: None,
        flags: None,
        hfret: None,
        htype: Some("Pinch".to_owned()),
        pitches: None,
        label: None,
        text: None,
        direction: None,
        items: None,
        element: None,
        variation: None,
    });
    let song_note = super::convert_note(&note, 80, false, false);
    let harmonic = song_note.effect.harmonic.expect("harmonic imported");
    assert_eq!(harmonic.kind, crate::model::HarmonicType::Pinch);
    assert_eq!(harmonic.fret, Some(12));
}

/// GP7 keeps the dive bomb in `<Whammy>`, not `<Tremolo>`; the reader used to drop it.
#[test]
fn whammy_bar_is_imported() {
    let beat = GpifBeat {
        id: 0,
        notes: None,
        rhythm: None,
        dynamic: None,
        grace_notes: None,
        fadding: None,
        tremolo: None,
        whammy: Some(Whammy {
            origin_value: 0.0,
            middle_value: None,
            destination_value: -300.0,
            middle_offset: None,
            destination_offset: None,
        }),
        wah: None,
        legato: None,
        free_text: None,
        lyrics: None,
        ottavia: None,
        chord: None,
        arpeggio: None,
        dead_slapped: None,
        properties: None,
        x_properties: None,
    };
    let mut velocity = 0;
    let song_beat = super::convert_beat(&beat, &HashMap::new(), &HashMap::new(), &mut velocity);
    let tremolo = song_beat.effect.tremolo_bar.expect("whammy imported");
    assert_eq!(tremolo.points.last().map(|point| point.value), Some(-12));
}

/// A bend that goes up and is released again (origin = destination = 0) used to be read
/// as no bend at all, because the reader ignored the middle value.
#[test]
fn bend_and_release_survives_reading_and_writing() {
    use crate::io::Song;
    use crate::model::BendType;
    let bend = super::build_note_bend(0.0, Some(50.0), 0.0, Some(50.0), Some(100.0))
        .expect("a bend-and-release moves");
    assert_eq!(bend.kind, BendType::BendRelease);
    let values: Vec<i8> = bend.points.iter().map(|p| p.value).collect();
    assert_eq!(values, [0, 2, 0]);
    assert_eq!(
        super::build_note_bend(0.0, Some(25.0), 50.0, None, None).map(|b| b.kind),
        Some(BendType::Bend)
    );

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
    beat.notes[0].effect.bend = Some(bend);
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
    let bend = reread.tracks[0].measures[0].voices[0].beats[0].notes[0]
        .effect
        .bend
        .clone()
        .expect("the bend is still there");
    assert_eq!(bend.kind, BendType::BendRelease);
    let values: Vec<i8> = bend.points.iter().map(|p| p.value).collect();
    assert_eq!(values, [0, 2, 0]);
}

/// A dip goes down and comes back (origin = destination = 0); the middle used to be
/// averaged to 0 and the writer then dropped the whole effect.
#[test]
fn whammy_dip_survives_writing() {
    use crate::io::Song;
    let dip = super::build_whammy_effect(0.0, Some(-100.0), 0.0, Some(50.0), Some(100.0))
        .expect("a dip moves");
    let values: Vec<i8> = dip.points.iter().map(|p| p.value).collect();
    assert_eq!(values, [0, -4, 0]);
    assert!(super::build_whammy_effect(0.0, Some(0.0), 0.0, None, None).is_none());

    let mut song = Song {
        tracks: vec![crate::model::Track::default()],
        measure_headers: vec![Default::default()],
        ..Song::default()
    };
    let mut beat = crate::model::Beat::default();
    beat.effect.tremolo_bar = Some(dip);
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
    let tremolo = reread.tracks[0].measures[0].voices[0].beats[0]
        .effect
        .tremolo_bar
        .clone()
        .expect("the dip is still there");
    let values: Vec<i8> = tremolo.points.iter().map(|p| p.value).collect();
    assert_eq!(values, [0, -4, 0]);
}

/// GP6 (GPX) writes the same effect as three beat properties, on every beat (0 = none).
#[test]
fn gp6_whammy_properties_are_imported() {
    fn whammy_beat(destination: f64) -> GpifBeat {
        let property = |name: &str, value: f64| BeatProperty {
            name: name.to_owned(),
            direction: None,
            enable: None,
            float: Some(value),
            flags: None,
            strength: None,
            duration: None,
            text: None,
        };
        GpifBeat {
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
                properties: vec![
                    property("WhammyBarOriginValue", 0.0),
                    property("WhammyBarDestinationValue", destination),
                ],
            }),
            x_properties: None,
        }
    }

    let mut velocity = 0;
    let dive = whammy_beat(-300.0);
    let song_beat = super::convert_beat(&dive, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert!(song_beat.effect.tremolo_bar.is_some());

    let none = whammy_beat(0.0);
    let song_beat = super::convert_beat(&none, &HashMap::new(), &HashMap::new(), &mut velocity);
    assert!(song_beat.effect.tremolo_bar.is_none());
}
