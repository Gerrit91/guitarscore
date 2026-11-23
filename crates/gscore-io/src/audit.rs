//! What saving a GP7 file leaves out of it: the file is read, written again and the two
//! `score.gpif` fingerprints are compared (see `gscore_gp::gpif_fingerprint`). Unlike the
//! save's own check, which compares songs, this also sees what the reader never picked up.
//!
//! Every key a save loses has a verdict: Guitar Pro rebuilds it itself (nothing is lost),
//! it is a known loss still to be fixed, or it is unexplained (a new loss: `gscore-cli
//! audit --check` and `cargo xtask corpus` fail on it).

use crate::contain::contain;
use crate::error::IoError;
use crate::export::export_to_bytes;
use crate::format::{FileFormat, detect_format};
use crate::import::import_guitar_pro_from_bytes;

/// One key a save leaves out, or leaves out some of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loss {
    /// The fingerprint's key (`Note Property Muted`).
    pub key: String,
    /// How often it occurs in the file, and in the file saved again.
    pub before: usize,
    pub after: usize,
}

/// What a lost key means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Guitar Pro rebuilds it (from the notes, the tuning, its defaults): nothing is lost.
    Rebuilt(&'static str),
    /// Information that is lost, known and still to be fixed.
    Known(&'static str),
    /// Not looked at yet: a new loss.
    Unexplained,
}

/// The keys a save may lose, with their verdict. A key ending in `*` covers every key
/// that starts with what comes before it.
const VERDICTS: &[(&str, Verdict)] = &[
    (
        "Note <Pitch>",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note <Step>",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note <Accidental>",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note <Octave>",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note <Number>",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note Property ConcertPitch",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note Property TransposedPitch",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note Property Midi",
        Verdict::Rebuilt(
            "a note's pitch, worked out from its fret and the tuning (a drum's from its kit piece)",
        ),
    ),
    (
        "Note <InstrumentArticulation>",
        Verdict::Rebuilt("a note's place in the instrument's articulation table"),
    ),
    (
        "Beat <ConcertPitchStemOrientation>",
        Verdict::Rebuilt("a stem direction left to Guitar Pro (Undefined)"),
    ),
    (
        "Beat <TransposedPitchStemOrientation>",
        Verdict::Rebuilt("a stem direction left to Guitar Pro (Undefined)"),
    ),
    (
        "Beat Property PrimaryPickupVolume",
        Verdict::Rebuilt("Guitar Pro's per-beat pickup volume, at its default"),
    ),
    (
        "Beat Property PrimaryPickupTone",
        Verdict::Rebuilt("Guitar Pro's per-beat pickup tone, at its default"),
    ),
    (
        "Beat <Float>",
        Verdict::Rebuilt("the values of the beat's pickup properties"),
    ),
    (
        "Beat <Properties>",
        Verdict::Rebuilt("the container of the beat's properties"),
    ),
    (
        "InstrumentSet *",
        Verdict::Rebuilt("the instrument's articulation and notation table, rebuilt from its type"),
    ),
    (
        "Track <NotationPatch>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <Elements>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <Element>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <Articulations>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <Articulation>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <StaffLine>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <LineCount>",
        Verdict::Rebuilt("a drum kit's notation table"),
    ),
    (
        "Track <Name>",
        Verdict::Rebuilt("names in a drum kit's notation table (the track's name is kept)"),
    ),
    (
        "MasterBar <TransposeAs>",
        Verdict::Rebuilt("a per-bar default"),
    ),
    (
        "Lyrics <Line>",
        Verdict::Rebuilt("an empty lyrics line (a line with text counts as Lyrics text)"),
    ),
    (
        "Lyrics <Text>",
        Verdict::Rebuilt("an empty lyrics line (a line with text counts as Lyrics text)"),
    ),
    (
        "Lyrics <Offset>",
        Verdict::Rebuilt("an empty lyrics line (a line with text counts as Lyrics text)"),
    ),
    ("Track <Lyrics>", Verdict::Rebuilt("a track without lyrics")),
    (
        "Automation <Type>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Automation <Linear>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Automation <Visible>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Automation <Bar>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Automation <Position>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Automation <Value>",
        Verdict::Rebuilt("an automation's fields (its kind counts as Automation type)"),
    ),
    (
        "Track <Automation>",
        Verdict::Rebuilt("an automation (its kind counts as Automation type)"),
    ),
    (
        "Track <Automations>",
        Verdict::Rebuilt("the container of a track's automations"),
    ),
    (
        "MasterTrack <Automation>",
        Verdict::Rebuilt("an automation (its kind counts as Automation type)"),
    ),
    (
        "RSE <Automation>",
        Verdict::Rebuilt("an automation (its kind counts as Automation type)"),
    ),
    (
        "RSE <Automations>",
        Verdict::Rebuilt("the container of a channel strip's automations"),
    ),
    (
        "Automation type DSPParam_11",
        Verdict::Rebuilt("a track's starting volume, kept in its channel strip"),
    ),
    (
        "Automation type DSPParam_12",
        Verdict::Rebuilt("a track's starting pan, kept in its channel strip"),
    ),
    (
        "Automation type DSPParam_00",
        Verdict::Rebuilt("the master's starting volume, kept with the song"),
    ),
    (
        "Automation type DSPParam_01",
        Verdict::Rebuilt("the master's starting pan, kept with the song"),
    ),
    (
        "Track <Transpose>",
        Verdict::Rebuilt("a transposition of 0 (others are kept)"),
    ),
    (
        "Track <Chromatic>",
        Verdict::Rebuilt("a transposition of 0 (others are kept)"),
    ),
    (
        "Track <Octave>",
        Verdict::Rebuilt("a transposition of 0 (others are kept)"),
    ),
    (
        "Track <Staves>",
        Verdict::Rebuilt("the staff's properties, written on the track as Guitar Pro 6 does"),
    ),
    (
        "Track <Staff>",
        Verdict::Rebuilt("the staff's properties, written on the track as Guitar Pro 6 does"),
    ),
    (
        "Staff <Properties>",
        Verdict::Rebuilt("the staff's properties, written on the track as Guitar Pro 6 does"),
    ),
    (
        "Staff Property Tuning",
        Verdict::Rebuilt("written on the track as Guitar Pro 6 does"),
    ),
    (
        "Staff <Pitches>",
        Verdict::Rebuilt("written on the track as Guitar Pro 6 does"),
    ),
    (
        "Staff Property CapoFret",
        Verdict::Rebuilt("written on the track as Guitar Pro 6 does"),
    ),
    (
        "Staff <Items>",
        Verdict::Rebuilt("an empty chord or diagram collection"),
    ),
    (
        "Staff Property ChordCollection",
        Verdict::Rebuilt("an empty chord collection (a chord in one counts as Staff <Chord>)"),
    ),
    (
        "Staff Property ChordWorkingSet",
        Verdict::Rebuilt("an empty chord collection (a chord in one counts as Staff <Chord>)"),
    ),
    (
        "Staff Property DiagramCollection",
        Verdict::Rebuilt("an empty diagram collection (a diagram counts as Staff <Diagram>)"),
    ),
    (
        "Staff Property DiagramWorkingSet",
        Verdict::Rebuilt("an empty diagram collection (a diagram counts as Staff <Diagram>)"),
    ),
    (
        "Score <ScoreZoom>",
        Verdict::Rebuilt("Guitar Pro's zoom when the file was saved"),
    ),
    (
        "Score <ScoreZoomPolicy>",
        Verdict::Rebuilt("Guitar Pro's zoom when the file was saved"),
    ),
    (
        "Score <MultiVoice>",
        Verdict::Rebuilt("Guitar Pro's voice view when the file was saved"),
    ),
    (
        "Note Property Bended",
        Verdict::Known("a bend the writer leaves out (one file, not traced yet)"),
    ),
    (
        "Beat dynamic *",
        Verdict::Known(
            "the dynamic of some beats (a few hundred in a third of the files, not traced yet)",
        ),
    ),
    (
        "Beat <Hairpin>",
        Verdict::Known("crescendo and decrescendo"),
    ),
    ("Beat <Wah>", Verdict::Known("the wah pedal (open, closed)")),
    (
        "Beat <Variation>",
        Verdict::Known("a drum note's variation"),
    ),
    ("Beat <Whammy>", Verdict::Known("a whammy bar shape")),
    (
        "Beat <WhammyExtend>",
        Verdict::Known("a whammy bar held over the next beats"),
    ),
    ("Beat <Rasgueado>", Verdict::Known("a rasgueado's pattern")),
    (
        "MasterBar <Letter>",
        Verdict::Known("a section's letter (A, B, ...)"),
    ),
    ("MasterBar <Length>", Verdict::Known("a bar's own length")),
    (
        "Lyrics text",
        Verdict::Known("lyrics lines beyond what GuitarScore keeps"),
    ),
    ("Automation type Tempo", Verdict::Known("tempo changes")),
    (
        "Automation type Sound",
        Verdict::Known("a track's sound changes during the song"),
    ),
    ("Automation <Text>", Verdict::Known("an automation's text")),
    (
        "RSE <Effect>",
        Verdict::Known("Guitar Pro's master effects (its own playback)"),
    ),
    (
        "RSE <ByPass>",
        Verdict::Known("Guitar Pro's master effects (its own playback)"),
    ),
    (
        "RSE <Parameters>",
        Verdict::Known("Guitar Pro's master effects (its own playback)"),
    ),
    (
        "RSE <ByPassedAutomations>",
        Verdict::Known("Guitar Pro's master effects (its own playback)"),
    ),
    (
        "Track <PalmMute>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <AutoAccentuation>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <AutoBrush>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <PlayingStyle>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <UseOneChannelPerString>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <ForcedSound>",
        Verdict::Known("Guitar Pro's playback setting of the track"),
    ),
    (
        "Track <IconId>",
        Verdict::Known("the track's icon in Guitar Pro"),
    ),
    (
        "Track <SystemsLayout>",
        Verdict::Known("a track's bars per line"),
    ),
    (
        "Track <SystemsDefautLayout>",
        Verdict::Known("a track's bars per line"),
    ),
    (
        "Score <ScoreSystemsLayout>",
        Verdict::Known("the score's bars per line"),
    ),
    (
        "Score <PageHeader>",
        Verdict::Known("the page header's template"),
    ),
    (
        "Score <PageFooter>",
        Verdict::Known("the page footer's template"),
    ),
    (
        "Score <FirstPageHeader>",
        Verdict::Known("the first page's header template"),
    ),
    (
        "Score <FirstPageFooter>",
        Verdict::Known("the first page's footer template"),
    ),
    (
        "Score <WordsAndMusic>",
        Verdict::Known("the words-and-music line's template"),
    ),
    (
        "GPIF <ScoreViews>",
        Verdict::Known("Guitar Pro's score views"),
    ),
    (
        "GPIF <ScoreView>",
        Verdict::Known("Guitar Pro's score views"),
    ),
    (
        "GPIF <AudioTracks>",
        Verdict::Known("Guitar Pro 8's audio tracks"),
    ),
    (
        "Staff Property FretCount",
        Verdict::Known("the number of frets"),
    ),
    ("Staff <Number>", Verdict::Known("the number of frets")),
    (
        "Staff Property PartialCapoFret",
        Verdict::Known("a partial capo"),
    ),
    (
        "Staff Property PartialCapoStringFlags",
        Verdict::Known("a partial capo"),
    ),
    ("Staff <Bitset>", Verdict::Known("a partial capo")),
    (
        "Staff <Fret>",
        Verdict::Known("a capo or a chord diagram's fret"),
    ),
    (
        "Staff Property TuningFlat",
        Verdict::Known("a tuning shown with flats"),
    ),
    ("Staff <Flat>", Verdict::Known("a tuning shown with flats")),
    (
        "Staff <Enable>",
        Verdict::Known("a tuning shown with flats, or a chord diagram's display flags"),
    ),
    ("Staff <Name>", Verdict::Known("the tuning's name")),
    ("Staff <Label>", Verdict::Known("the tuning's label")),
    ("Staff <LabelVisible>", Verdict::Known("the tuning's label")),
    (
        "Staff <Instrument>",
        Verdict::Known("the tuning's instrument"),
    ),
    (
        "Staff <Chord>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <Diagram>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <Item>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <KeyNote>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <BassNote>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <Position>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <Degree>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff <Fingering>",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff Property ShowName",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff Property ShowDiagram",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Staff Property ShowFingering",
        Verdict::Known("the track's chord diagram library"),
    ),
    (
        "Note <Tie>",
        Verdict::Known("a tie in a file where it is not on the note's properties"),
    ),
    ("Note <Float>", Verdict::Known("a note property's value")),
    ("Note <Int>", Verdict::Known("a note property's value")),
    (
        "Note <XProperties>",
        Verdict::Known("Guitar Pro's extra note properties"),
    ),
    (
        "Note XProperty *",
        Verdict::Known("Guitar Pro's extra note properties"),
    ),
    (
        "Note Property Bend*",
        Verdict::Known("a bend's shape in one file"),
    ),
];

/// The verdict on losing `key`.
pub fn verdict(key: &str) -> Verdict {
    VERDICTS
        .iter()
        .find(|(pattern, _)| match pattern.strip_suffix('*') {
            Some(prefix) => key.starts_with(prefix),
            None => key == *pattern,
        })
        .map_or(Verdict::Unexplained, |(_, verdict)| *verdict)
}

/// What saving the GP7 file `data` as GP7 again leaves out, by key. Fails for a file that
/// is not GP7 or cannot be read or written.
pub fn gp7_save_losses(data: &[u8]) -> Result<Vec<Loss>, IoError> {
    let format = detect_format(data)?;
    if format != FileFormat::Gp {
        return Err(IoError::UnsupportedFormat(format!(
            "{format}: only GP7 files are audited"
        )));
    }
    let song = import_guitar_pro_from_bytes(data)?.song;
    let saved = export_to_bytes(&song, FileFormat::Gp)?;
    let before = contain(FileFormat::Gp, "fingerprint", || {
        gscore_gp::gpif_fingerprint(data)
    })?;
    let after = contain(FileFormat::Gp, "fingerprint", || {
        gscore_gp::gpif_fingerprint(&saved)
    })?;
    Ok(before
        .into_iter()
        .filter_map(|(key, before)| {
            let after = after.get(&key).copied().unwrap_or(0);
            (after < before).then_some(Loss { key, before, after })
        })
        .collect())
}
