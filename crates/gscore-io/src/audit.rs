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
