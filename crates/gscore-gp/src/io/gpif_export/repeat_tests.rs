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
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    let kept = crate::model::GpifBeatKept {
        hairpin: Some("Crescendo".to_owned()),
        wah: Some("Open".to_owned()),
        variation: Some("2".to_owned()),
        whammy_extend: true,
        flat_whammy: true,
        rasgueado: Some("amii_1".to_owned()),
        rest_dynamic: None,
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

/// A tempo change inside a bar (a written ritardando) survives a GP7 round trip on its
/// beat, and one at a bar's very end becomes the next bar's tempo.
#[test]
fn tempo_changes_inside_a_bar_round_trip_through_gpif() {
    use crate::io::gpif_import::tempo::{beat_tempo, mid_bar_tempos};
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    // The second bar holds several beats: the second one changes the tempo.
    let beats = &mut song.tracks[0].measures[1].voices[0].beats;
    assert!(beats.len() > 1, "the bar has several beats");
    beats[1].effect.mix_table_change = Some(crate::model::MixTableChange {
        tempo: Some(crate::model::MixTableItem {
            value: 93,
            ..Default::default()
        }),
        ..Default::default()
    });
    let written = mid_bar_tempos(&song);
    assert_eq!(written.len(), 1, "{written:?}");

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let beats = &reread.tracks[0].measures[1].voices[0].beats;
    assert_eq!(beat_tempo(&beats[1]), Some(93));
    assert_eq!(beat_tempo(&beats[0]), None);
    assert_eq!(mid_bar_tempos(&reread), written);
}

/// Typed lyrics on a track that is not the song's lyric track (backing vocals) survive a
/// GP7 round trip on that track.
#[test]
fn other_tracks_typed_lyrics_round_trip_through_gpif() {
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/multi_staff.gp"
    ))
    .expect("read");
    song.lyrics.track_choice = 0;
    song.lyrics.lines = vec![(0, 1, "la la".to_owned())];
    song.tracks[1].gpif_lyrics = vec![(0, 3, "oh oh".to_owned()), (1, 5, "yeah".to_owned())];

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(reread.lyrics.track_choice, 0);
    assert_eq!(reread.lyrics.lines, vec![(0, 1, "la la".to_owned())]);
    assert_eq!(reread.tracks[1].gpif_lyrics, song.tracks[1].gpif_lyrics);
}

/// A track's sound changing during the song (Guitar Pro's `Sound` automations) survives a
/// GP7 round trip on its bar, and the bar-0 sound every file writes is kept as well.
#[test]
fn track_sound_changes_round_trip_through_gpif() {
    use crate::model::SoundChange;
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    assert!(
        !song.tracks[0].measures[0].sound_changes.is_empty(),
        "the file starts with a sound"
    );
    song.tracks[0].measures[1].sound_changes = vec![SoundChange {
        position: 0.5,
        value: "Stringed/Electric Guitars/Clean Guitar;Clean;Factory".to_owned(),
    }];

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(
        reread.tracks[0].measures[0].sound_changes,
        song.tracks[0].measures[0].sound_changes
    );
    assert_eq!(
        reread.tracks[0].measures[1].sound_changes,
        song.tracks[0].measures[1].sound_changes
    );
}

/// The page header and footer templates, the words-and-music credit, Guitar Pro 8's audio
/// track list and its score views survive a GP7 round trip.
#[test]
fn song_templates_and_raw_sections_round_trip_through_gpif() {
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    assert!(
        song.gpif_audio_tracks.is_some(),
        "the file has audio tracks"
    );
    song.words_and_music = "Words & Music".to_owned();
    song.first_page_header = "<html><body>%TITLE%</body></html>".to_owned();
    song.first_page_footer = "<html><body>Page %page%</body></html>".to_owned();
    song.page_header = "<html><body>%ARTIST%</body></html>".to_owned();
    song.page_footer = "<html><body>Page %page%/%pages%</body></html>".to_owned();
    song.gpif_score_views =
        Some("<ScoreViews><ScoreView id=\"1\"/><ScoreView id=\"2\"/></ScoreViews>".to_owned());

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(reread.words_and_music, song.words_and_music);
    assert_eq!(reread.first_page_header, song.first_page_header);
    assert_eq!(reread.first_page_footer, song.first_page_footer);
    assert_eq!(reread.page_header, song.page_header);
    assert_eq!(reread.page_footer, song.page_footer);
    assert_eq!(reread.gpif_score_views, song.gpif_score_views);
    assert_eq!(reread.gpif_audio_tracks, song.gpif_audio_tracks);
}

/// Guitar Pro's master effect chain is written back, including its bypassed automations;
/// a master volume change later in the song stays, and the bar-0 value follows the model.
#[test]
fn master_effects_and_volume_changes_round_trip_through_gpif() {
    use crate::model::MasterAutomation;
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    assert!(
        !song.gpif_master_effects.is_empty(),
        "the file has master effects"
    );
    let Some(volume_pan) = song
        .gpif_master_effects
        .iter_mut()
        .find(|effect| effect.id == crate::io::gpif_import::MASTER_VOLUME_PAN)
    else {
        panic!("the file has a volume-and-pan effect");
    };
    volume_pan.bypassed_automations = Some("DSPParam_00".to_owned());
    volume_pan.automations.push(MasterAutomation {
        automation_type: "DSPParam_00".to_owned(),
        linear: false,
        bar: 0,
        position: 0.0,
        visible: true,
        text: None,
        value: "0.9".to_owned(),
    });
    volume_pan.automations.push(MasterAutomation {
        automation_type: "DSPParam_00".to_owned(),
        linear: false,
        bar: 2,
        position: 0.0,
        visible: true,
        text: None,
        value: "0.4".to_owned(),
    });
    // A change to the model's master volume must reach the written parameters.
    song.master_volume_pan = Some((0.25, 0.5));

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(reread.master_volume_pan, Some((0.25, 0.5)));
    let effect = reread
        .gpif_master_effects
        .iter()
        .find(|effect| effect.id == crate::io::gpif_import::MASTER_VOLUME_PAN)
        .expect("the effect is written");
    assert_eq!(effect.bypassed_automations.as_deref(), Some("DSPParam_00"));
    let later = effect
        .automations
        .iter()
        .find(|automation| automation.bar == 2)
        .expect("the mid-song change is written");
    assert_eq!(later.value, "0.4");
    let start = effect
        .automations
        .iter()
        .find(|automation| automation.bar == 0 && automation.automation_type == "DSPParam_00")
        .expect("the starting volume is written");
    assert_eq!(start.value, "0.25");
}

/// An empty `<SystemsLayout>` (a track laid out by Guitar Pro's default) survives, and so
/// does the tempo's name on the bar-0 automation.
#[test]
fn empty_systems_layout_and_tempo_text_round_trip_through_gpif() {
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    song.tracks[0].gpif_systems_layout = Some(String::new());
    song.tracks[0].gpif_systems_default = Some("3".to_owned());
    for measure in &mut song.tracks[0].measures {
        measure.line_break = crate::model::LineBreak::None;
    }
    song.gpif_tempo_text = Some("Allegro".to_owned());

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(reread.tracks[0].gpif_systems_layout.as_deref(), Some(""));
    assert_eq!(reread.tracks[0].gpif_systems_default.as_deref(), Some("3"));
    assert_eq!(reread.gpif_tempo_text.as_deref(), Some("Allegro"));
}

/// A note's own tie origin marker, its Guitar Pro properties, a fermata's length and a
/// tempo at the very end of a bar survive a GP7 round trip.
#[test]
fn note_tie_origin_properties_and_bar_marks_round_trip_through_gpif() {
    use crate::model::{NoteType, XProperty};
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    let note = &mut song.tracks[0].measures[0].voices[0].beats[0].notes[0];
    note.kind = NoteType::Normal;
    note.tie_origin = true;
    note.x_properties = vec![XProperty {
        id: "688062467".to_owned(),
        kind: "Int".to_owned(),
        value: "129".to_owned(),
    }];
    song.measure_headers[0].fermatas =
        vec![("Medium".to_owned(), "0/1".to_owned(), "0.5".to_owned())];
    song.measure_headers[0].end_tempos = vec![(1.0, 220)];

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let note = &reread.tracks[0].measures[0].voices[0].beats[0].notes[0];
    assert!(note.tie_origin, "the file's own tie origin is kept");
    assert_eq!(
        note.x_properties,
        song.tracks[0].measures[0].voices[0].beats[0].notes[0].x_properties
    );
    assert_eq!(
        reread.measure_headers[0].fermatas,
        song.measure_headers[0].fermatas
    );
    assert_eq!(
        reread.measure_headers[1].tempo, 220,
        "the next bar takes the end tempo"
    );
}

/// A `Bended` marker with no movement is written back (a zero bend), while the model
/// treats it as no bend for playback and drawing.
#[test]
fn a_flat_bend_marker_round_trips_through_gpif() {
    use crate::model::{BendEffect, BendPoint, BendType};
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    let note = &mut song.tracks[0].measures[0].voices[0].beats[0].notes[0];
    note.effect.bend = Some(BendEffect {
        kind: BendType::Bend,
        points: vec![BendPoint {
            position: 0,
            value: 0,
            vibrato: false,
        }],
        ..BendEffect::default()
    });

    let xml = song.write_gpif_xml().expect("written");
    assert!(xml.contains("<Property name=\"Bended\">"), "{xml}");
    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let bend = reread.tracks[0].measures[0].voices[0].beats[0].notes[0]
        .effect
        .bend
        .as_ref()
        .expect("the zero bend is kept");
    assert!(bend.points.iter().all(|point| point.value == 0));
}

/// A staff's block, which GuitarScore does not model, survives: the tuning's name, its
/// instrument and flats, a partial capo, the fret count and the whole chord library.
#[test]
fn the_staff_block_round_trips_through_gpif() {
    let mut song = Song::default();
    song.read_gp(include_bytes!("../../../../../testdata/basic/staff.gp"))
        .expect("read");
    let kept = song.tracks[0].staff_kept.clone();
    assert_eq!(kept.name.as_deref(), Some("Standard"));
    assert_eq!(kept.tuning_instrument.as_deref(), Some("Guitar"));
    assert!(
        kept.tuning_flat_child,
        "the test file's tuning has a Flat child"
    );
    assert!(
        !kept.chord_libraries.is_empty(),
        "the test file has a chord library"
    );
    assert_eq!(song.tracks[0].fret_count, 24);

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(reread.tracks[0].staff_kept, kept);
    assert_eq!(reread.tracks[0].fret_count, 24);
}

/// A grace beat carrying Guitar Pro's `<Variation>` is not folded into the next note, so
/// the variation survives.
#[test]
fn a_grace_beat_with_a_variation_is_not_folded() {
    use crate::model::{Duration, GpifBeatKept};
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    let voice = &mut song.tracks[0].measures[0].voices[0];
    let main = voice.beats[0].clone();
    let mut grace = crate::model::Beat {
        grace: true,
        duration: Duration {
            value: 32,
            ..Duration::default()
        },
        ..crate::model::Beat::default()
    };
    grace.kept = GpifBeatKept {
        variation: Some("2".to_owned()),
        ..GpifBeatKept::default()
    };
    grace.notes = main
        .notes
        .iter()
        .map(|note| crate::model::Note {
            kind: crate::model::NoteType::Normal,
            value: 3,
            ..note.clone()
        })
        .collect();
    voice.beats.insert(0, grace);

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    let beats = &reread.tracks[0].measures[0].voices[0].beats;
    assert!(beats[0].grace, "the grace beat stays a beat");
    assert_eq!(beats[0].kept.variation.as_deref(), Some("2"));
}

/// The beats' chord diagrams are numbered as the file numbered them, so the staff's kept
/// libraries (which refer to those ids) stay right; a diagram the file did not have gets
/// the next id, and the staff's diagram collection is then written anew with it.
#[test]
fn chord_diagrams_keep_the_files_ids_and_a_new_one_is_added() {
    use crate::model::ChordDiagram;
    let diagram = |name: &str, fret: i8| ChordDiagram {
        name: name.to_owned(),
        string_count: 6,
        fret_count: 5,
        base_fret: 0,
        frets: vec![(0, fret)],
    };
    let (a, b, c) = (diagram("A", 0), diagram("B", 2), diagram("C", 3));
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    let library = String::from(
        "<Property name=\"DiagramCollection\"><Items>\
         <Item id=\"5\" name=\"A\"><Diagram stringCount=\"6\" fretCount=\"5\" baseFret=\"0\"><Fret string=\"0\" fret=\"0\"/></Diagram></Item>\
         <Item id=\"7\" name=\"B\"><Diagram stringCount=\"6\" fretCount=\"5\" baseFret=\"0\"><Fret string=\"0\" fret=\"2\"/></Diagram></Item>\
         </Items></Property>",
    );
    {
        let track = &mut song.tracks[0];
        track.gpif_diagram_ids = vec![(5, a.clone()), (7, b.clone())];
        track.staff_kept.chord_libraries = vec![library.clone()];
        track.measures[0].voices[0].beats[0].chord_diagram = Some(b.clone());
    }
    let chords = |song: &Song| -> Vec<ChordDiagram> {
        song.tracks[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.voices[0].beats)
            .filter_map(|beat| beat.chord_diagram.clone())
            .collect()
    };

    // Unchanged: the file's ids and its library as written.
    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(chords(&reread), vec![b.clone()]);
    assert_eq!(reread.tracks[0].staff_kept.chord_libraries, vec![library]);
    assert_eq!(
        reread.tracks[0].gpif_diagram_ids,
        vec![(5, a.clone()), (7, b.clone())]
    );

    // A chord added in GuitarScore: it gets id 8 and both beats read back right.
    song.tracks[0].measures[1].voices[0].beats[0].chord_diagram = Some(c.clone());
    // Guitar Pro 7 reads the staff's collection: it must hold the new diagram, and the
    // beats must name the ids it has.
    let xml = song.write_gpif_xml().expect("written");
    let staff = &xml[xml.find("<Staves>").expect("a staff block")..];
    let staff = &staff[..staff.find("</Staves>").expect("its end")];
    assert!(staff.contains("<Item id=\"8\" name=\"C\">"), "{staff}");
    assert!(staff.contains("<Item id=\"7\" name=\"B\">"));
    assert!(xml.contains("<Chord>7</Chord>") && xml.contains("<Chord>8</Chord>"));
    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(chords(&reread), vec![b.clone(), c.clone()]);
    assert_eq!(
        reread.tracks[0].gpif_diagram_ids,
        vec![(5, a), (7, b), (8, c)]
    );
}

/// What a GP3-5 song keeps that GP7 has no place for, or lost on the way, survives a GP7
/// round trip: notes ending in empty lines, a note louder than its beat's dynamic, and a
/// section's color.
#[test]
fn notes_lines_note_velocities_and_section_colors_round_trip_through_gpif() {
    let mut song = Song::default();
    song.read_gp(include_bytes!(
        "../../../../../testdata/basic/note_lengths.gp"
    ))
    .expect("read");
    song.notice = vec![
        String::new(),
        "First line".to_owned(),
        String::new(),
        String::new(),
    ];
    song.measure_headers[0].marker = Some(crate::model::Marker {
        title: "Intro".to_owned(),
        color: 0x00_FF_00_00,
        ..Default::default()
    });
    // A chord whose second note is softer than the first (the beat's dynamic).
    let beat = first_beat(&mut song);
    let mut second = beat.notes[0].clone();
    second.string = if beat.notes[0].string == 1 { 2 } else { 1 };
    second.velocity = beat.notes[0].velocity.saturating_sub(16);
    beat.notes.push(second);
    let velocities: Vec<i16> = beat.notes.iter().map(|note| note.velocity).collect();

    let bytes = song.write_gp().expect("written");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("read back");
    assert_eq!(
        reread.notice, song.notice,
        "every line, the empty last ones too"
    );
    let marker = reread.measure_headers[0]
        .marker
        .as_ref()
        .expect("a section");
    assert_eq!(
        (marker.title.as_str(), marker.color),
        ("Intro", 0x00_FF_00_00)
    );
    let reread_velocities: Vec<i16> = first_beat(&mut reread)
        .notes
        .iter()
        .map(|note| note.velocity)
        .collect();
    assert_eq!(
        reread_velocities, velocities,
        "each note keeps its own dynamic"
    );
}

/// A GP5 marker's title is an int-byte-size string: read without its length byte, and
/// written so Guitar Pro reads it the same way.
#[test]
fn a_gp5_marker_title_has_no_length_character() {
    let mut song = empty_song(1);
    song.measure_headers[0].marker = Some(crate::model::Marker {
        title: "Intro".to_owned(),
        ..Default::default()
    });
    let bytes = song.write((5, 1, 0), None).expect("written");
    // As Guitar Pro stores it: the size (length plus one), the length byte, the text.
    let stored = [6_u8, 0, 0, 0, 5, b'I', b'n', b't', b'r', b'o'];
    assert!(
        bytes.windows(stored.len()).any(|window| window == stored),
        "written as an int-byte-size string"
    );
    let mut reread = Song::default();
    reread.read_gp5(&bytes).expect("read back");
    assert_eq!(
        reread.measure_headers[0]
            .marker
            .as_ref()
            .map(|m| m.title.as_str()),
        Some("Intro")
    );
}
