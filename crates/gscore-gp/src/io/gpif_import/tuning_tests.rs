//! GuitarScore regression tests for the tuning fixes (see docs/guitarpro.md).

use crate::io::Song;
use crate::io::gpif::Gpif;
use crate::io::gpif_import::SongGpifOps;

/// A minimal GPIF document with one track whose tuning is on the staff, as GP7 writes it.
fn gpif_with_staff_tuning(pitches: &str) -> Gpif {
    let xml = format!(
        "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks><Track id=\"0\">\
         <Name>Bass</Name><Staves><Staff><Properties>\
         <Property name=\"Tuning\"><Pitches>{pitches}</Pitches></Property>\
         </Properties></Staff></Staves></Track></Tracks><MasterBars></MasterBars>\
         <Bars></Bars><Voices></Voices><Beats></Beats><Notes></Notes>\
         <Rhythms></Rhythms></GPIF>"
    );
    quick_xml::de::from_str(&xml).expect("test GPIF parses")
}

#[test]
fn missing_tuning_is_marked_and_not_written() {
    let xml = "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks><Track id=\"0\">\
               <Name>Singer</Name></Track></Tracks><MasterBars></MasterBars><Bars></Bars>\
               <Voices></Voices><Beats></Beats><Notes></Notes><Rhythms></Rhythms></GPIF>";
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(xml).expect("test GPIF parses"));
    assert!(!song.tracks[0].has_tuning);
    // Placeholder strings stay, because code indexes them.
    assert_eq!(song.tracks[0].strings.len(), 6);

    let bytes = song.write_gp().expect("write");
    let gpif = crate::io::gpx::read_gp_with_audio(&bytes)
        .expect("read written file")
        .0;
    let written_tuning = gpif.tracks.tracks[0]
        .properties
        .iter()
        .flat_map(|p| &p.properties)
        .any(|p| p.name == "Tuning");
    assert!(!written_tuning, "a tuning was invented on export");

    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    assert!(!reread.tracks[0].has_tuning);
}

#[test]
fn instrument_set_is_read_and_drum_kits_are_percussion() {
    let xml = "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks>\
               <Track id=\"0\"><Name>Fabi</Name><InstrumentSet><Name>Drums</Name>\
               <Type>drumKit</Type><LineCount>5</LineCount><Elements><Element>\
               <Name>Kick</Name><Type>kick</Type></Element></Elements></InstrumentSet>\
               </Track>\
               <Track id=\"1\"><Name>Singer</Name><InstrumentSet><Name>Voice</Name>\
               <Type>voice</Type></InstrumentSet></Track>\
               </Tracks><MasterBars></MasterBars><Bars></Bars><Voices></Voices>\
               <Beats></Beats><Notes></Notes><Rhythms></Rhythms></GPIF>";
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(xml).expect("test GPIF parses"));
    let drums = &song.tracks[0];
    assert_eq!(drums.instrument_type.as_deref(), Some("drumKit"));
    assert_eq!(drums.instrument_name.as_deref(), Some("Drums"));
    assert!(drums.percussion_track);
    let voice = &song.tracks[1];
    assert_eq!(voice.instrument_type.as_deref(), Some("voice"));
    assert!(!voice.percussion_track);
}

#[test]
fn tuning_label_is_read_when_set() {
    let xml = "<GPIF><Score></Score><MasterTrack></MasterTrack><Tracks><Track id=\"0\">\
               <Name>G</Name><Staves><Staff><Properties><Property name=\"Tuning\">\
               <Pitches>36 43 48 53 57 62</Pitches><Label><![CDATA[My Drop C]]></Label>\
               </Property></Properties></Staff></Staves></Track></Tracks>\
               <MasterBars></MasterBars><Bars></Bars><Voices></Voices><Beats></Beats>\
               <Notes></Notes><Rhythms></Rhythms></GPIF>";
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(xml).expect("test GPIF parses"));
    assert_eq!(song.tracks[0].tuning_label.as_deref(), Some("My Drop C"));

    let mut unlabeled = Song::default();
    unlabeled.read_gpif(&gpif_with_staff_tuning("28 33 38 43"));
    assert_eq!(unlabeled.tracks[0].tuning_label, None);
}

#[test]
fn score_metadata_maps_like_the_gp5_reader() {
    let xml = "<GPIF><Score><Title>T</Title><Words>W</Words><Music>M</Music>\
               <Tabber>Tab</Tabber><Instructions>I</Instructions></Score>\
               <MasterTrack></MasterTrack><Tracks></Tracks><MasterBars></MasterBars>\
               <Bars></Bars><Voices></Voices><Beats></Beats><Notes></Notes>\
               <Rhythms></Rhythms></GPIF>";
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(xml).expect("test GPIF parses"));
    assert_eq!(
        (
            song.words.as_str(),
            song.author.as_str(),
            song.writer.as_str()
        ),
        ("W", "M", "Tab")
    );
    assert_eq!(song.instructions, "I");

    let both = xml.replace(
        "<Words>W</Words><Music>M</Music>",
        "<WordsAndMusic>WM</WordsAndMusic>",
    );
    let mut song = Song::default();
    song.read_gpif(&quick_xml::de::from_str(&both).expect("test GPIF parses"));
    assert_eq!((song.words.as_str(), song.author.as_str()), ("WM", "WM"));
}

#[test]
fn note_strings_are_flipped_and_round_trip() {
    use crate::io::Measure;
    use crate::io::MeasureHeader;
    use crate::io::Note;
    use crate::io::{Beat, Voice};

    let mut song = Song::default();
    song.read_gpif(&gpif_with_staff_tuning("28 33 38 43"));
    // GPIF string 0 (lowest) must come in as legacy string 4 (lowest of 4).
    let mut tracks = song.tracks.clone();
    tracks[0].measures = vec![Measure {
        voices: vec![Voice {
            beats: vec![Beat {
                notes: vec![Note {
                    string: 1,
                    ..Note::default()
                }],
                ..Beat::default()
            }],
            ..Voice::default()
        }],
        ..Measure::default()
    }];
    super::flip_note_strings(&mut tracks);
    assert_eq!(tracks[0].measures[0].voices[0].beats[0].notes[0].string, 4);
    super::flip_note_strings(&mut tracks);
    assert_eq!(tracks[0].measures[0].voices[0].beats[0].notes[0].string, 1);

    // Out-of-range strings are left alone.
    tracks[0].measures[0].voices[0].beats[0].notes[0].string = 9;
    super::flip_note_strings(&mut tracks);
    assert_eq!(tracks[0].measures[0].voices[0].beats[0].notes[0].string, 9);

    // Export and re-import keep the legacy string number.
    song.measure_headers = vec![MeasureHeader::default()];
    song.tracks[0].measures = tracks[0].measures.clone();
    song.tracks[0].measures[0].voices[0].beats[0].notes[0].string = 4;
    let bytes = song.write_gp().expect("write");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    let strings: Vec<i8> = reread.tracks[0].measures[0].voices[0].beats[0]
        .notes
        .iter()
        .map(|n| n.string)
        .collect();
    assert_eq!(strings, [4]);
}

#[test]
fn instrument_and_tuning_label_survive_writing() {
    let mut song = Song::default();
    song.read_gpif(&gpif_with_staff_tuning("38 45 50 55 57 62"));
    let mut drums = song.tracks[0].clone();
    song.tracks[0].instrument_type = Some("steelGuitar".to_owned());
    song.tracks[0].instrument_name = Some("Steel Guitar".to_owned());
    song.tracks[0].tuning_label = Some("My DADGAD".to_owned());
    // A drum track as GP3-5 store it: percussion flag, no instrument type.
    drums.percussion_track = true;
    drums.instrument_type = None;
    drums.instrument_name = None;
    song.tracks.push(drums);

    let bytes = song.write_gp().expect("write");
    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    let guitar = &reread.tracks[0];
    assert_eq!(guitar.instrument_type.as_deref(), Some("steelGuitar"));
    assert_eq!(guitar.instrument_name.as_deref(), Some("Steel Guitar"));
    assert_eq!(guitar.tuning_label.as_deref(), Some("My DADGAD"));
    let drums = &reread.tracks[1];
    assert!(drums.percussion_track);
    assert_eq!(drums.instrument_type.as_deref(), Some("drumKit"));
}

#[test]
fn staff_tuning_is_read_with_string_1_highest() {
    // GPIF lists pitches from the lowest string up.
    let mut song = Song::default();
    song.read_gpif(&gpif_with_staff_tuning("28 33 38 43"));
    assert_eq!(song.tracks[0].strings, [(1, 43), (2, 38), (3, 33), (4, 28)]);
    assert!(song.tracks[0].has_tuning);
}

#[test]
fn written_tuning_lists_lowest_string_first_and_round_trips() {
    let mut song = Song::default();
    song.read_gpif(&gpif_with_staff_tuning("33 40 45 50 54 59"));
    let bytes = song.write_gp().expect("write");
    let gpif = crate::io::gpx::read_gp_with_audio(&bytes)
        .expect("read written file")
        .0;
    let props = gpif.tracks.tracks[0]
        .properties
        .as_ref()
        .expect("track properties");
    let tuning = props
        .properties
        .iter()
        .find(|p| p.name == "Tuning")
        .and_then(|p| p.pitches.clone());
    assert_eq!(tuning.as_deref(), Some("33 40 45 50 54 59"));

    let mut reread = Song::default();
    reread.read_gp(&bytes).expect("reread");
    assert_eq!(reread.tracks[0].strings, song.tracks[0].strings);
}
