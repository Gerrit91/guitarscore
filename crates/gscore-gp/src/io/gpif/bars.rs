//! The GPIF XML from master bars (measure headers) down to notes and rhythms.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use serde::Deserialize;

use crate::model::{MAX_XPROPERTIES, XProperty};

// ---------------------------------------------------------------------------
// MasterBars (measure headers)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct MasterBarsWrapper {
    #[serde(rename = "MasterBar", default)]
    pub master_bars: Vec<MasterBar>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MasterBar {
    #[serde(rename = "Key", default)]
    pub key: Option<Key>,
    #[serde(rename = "Time", default)]
    pub time: String,
    #[serde(rename = "Bars", default)]
    pub bars: String,
    #[serde(rename = "Repeat", default)]
    pub repeat: Option<Repeat>,
    #[serde(rename = "AlternateEndings", default)]
    pub alternate_endings: Option<String>,
    #[serde(rename = "DoubleBar", default)]
    pub double_bar: Option<String>,
    #[serde(rename = "Section", default)]
    pub section: Option<Section>,
    #[serde(rename = "Fermatas", default)]
    pub fermatas: Option<FermatasWrapper>,
    /// GuitarScore fix: the measure's feel (`Triplet8th`, `Dotted16th`, ...) was not read.
    #[serde(rename = "TripletFeel", default)]
    pub triplet_feel: Option<String>,
    #[serde(rename = "FreeTime", default)]
    pub free_time: Option<String>,
    #[serde(rename = "Directions", default)]
    pub directions: Option<DirectionsWrapper>,
    /// GuitarScore addition: Guitar Pro's opaque layout properties (the design mode's
    /// song-wide bar scale, beaming rules), kept verbatim.
    #[serde(rename = "XProperties", default)]
    pub x_properties: Option<XPropertiesWrapper>,
}

/// The `<XProperties>` of a master bar, bar or beat.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct XPropertiesWrapper {
    #[serde(rename = "XProperty", default)]
    pub properties: Vec<XPropertyTag>,
}

/// One `<XProperty id="...">` with its value in a typed child element. Every value is read
/// as text: it is kept verbatim (a number is written back exactly as Guitar Pro wrote it),
/// and a value that is not a valid number cannot make the whole file fail to open; the
/// ids GuitarScore understands parse their number when they are used.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct XPropertyTag {
    #[serde(rename = "@id", default)]
    pub id: String,
    #[serde(rename = "Int", default)]
    pub int: Option<String>,
    #[serde(rename = "Double", default)]
    pub double: Option<String>,
    #[serde(rename = "Float", default)]
    pub float: Option<String>,
    #[serde(rename = "Text", default)]
    pub text: Option<String>,
}

impl XPropertyTag {
    /// This property as the model keeps it: the value's element name and its text. `None`
    /// for a value this code does not know (it cannot be written back faithfully).
    pub(crate) fn to_xproperty(&self) -> Option<XProperty> {
        let (kind, value) = match (&self.int, &self.double, &self.float, &self.text) {
            (Some(value), _, _, _) => ("Int", value.trim().to_owned()),
            (_, Some(value), _, _) => ("Double", value.trim().to_owned()),
            (_, _, Some(value), _) => ("Float", value.trim().to_owned()),
            (_, _, _, Some(value)) => ("Text", value.clone()),
            _ => return None,
        };
        Some(XProperty {
            id: self.id.clone(),
            kind: kind.to_owned(),
            value,
        })
    }
}

/// The properties of a `<XProperties>` wrapper, bounded to what one bar may keep.
pub(crate) fn xproperties(wrapper: Option<&XPropertiesWrapper>) -> Vec<XProperty> {
    wrapper.map_or_else(Vec::new, |wrapper| {
        wrapper
            .properties
            .iter()
            .filter_map(XPropertyTag::to_xproperty)
            .take(MAX_XPROPERTIES)
            .collect()
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct Key {
    #[serde(rename = "AccidentalCount", default)]
    pub accidental_count: i32,
    #[serde(rename = "Mode", default)]
    pub mode: String,
}

impl Default for Key {
    fn default() -> Self {
        Key {
            accidental_count: 0,
            mode: "Major".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Repeat {
    #[serde(rename = "@start", default)]
    pub start: String,
    #[serde(rename = "@end", default)]
    pub end: String,
    #[serde(rename = "@count", default)]
    pub count: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Section {
    #[serde(rename = "Letter", default)]
    pub letter: Option<String>,
    #[serde(rename = "Text", default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DirectionsWrapper {
    #[serde(rename = "Target", default)]
    pub target: Option<String>,
    #[serde(rename = "Jump", default)]
    pub jump: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FermatasWrapper {
    #[serde(rename = "Fermata", default)]
    pub fermatas: Vec<Fermata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Fermata {
    #[serde(rename = "Type", default)]
    pub fermata_type: Option<String>,
    #[serde(rename = "Offset", default)]
    pub offset: Option<String>,
}

// ---------------------------------------------------------------------------
// Bars (per-track measures)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct BarsWrapper {
    #[serde(rename = "Bar", default)]
    pub bars: Vec<Bar>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bar {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "Voices", default)]
    pub voices: String,
    #[serde(rename = "Clef", default)]
    pub clef: Option<String>,
    #[serde(rename = "SimileMark", default)]
    pub simile_mark: Option<String>,
    /// GuitarScore addition: Guitar Pro's opaque layout properties for this bar (the
    /// design mode's width scale), kept verbatim.
    #[serde(rename = "XProperties", default)]
    pub x_properties: Option<XPropertiesWrapper>,
}

// ---------------------------------------------------------------------------
// Voices
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct VoicesWrapper {
    #[serde(rename = "Voice", default)]
    pub voices: Vec<Voice>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Voice {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "Beats", default)]
    pub beats: String,
}

// ---------------------------------------------------------------------------
// Beats
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct BeatsWrapper {
    #[serde(rename = "Beat", default)]
    pub beats: Vec<Beat>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Beat {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "Notes", default)]
    pub notes: Option<String>,
    #[serde(rename = "Rhythm", default)]
    pub rhythm: Option<RhythmRef>,
    #[serde(rename = "Dynamic", default)]
    pub dynamic: Option<String>,
    #[serde(rename = "GraceNotes", default)]
    pub grace_notes: Option<String>,
    /// Note: "Fadding" is a typo in the upstream GP6 XML format (should be "Fading").
    #[serde(rename = "Fadding", default)]
    pub fadding: Option<String>,
    #[serde(rename = "Tremolo", default)]
    pub tremolo: Option<String>,
    /// GP7 whammy-bar shape (GuitarScore addition; the dive bomb was dropped).
    #[serde(rename = "Whammy", default)]
    pub whammy: Option<Whammy>,
    #[serde(rename = "Wah", default)]
    pub wah: Option<String>,
    /// GP6/GP7 beat-level legato (GuitarScore addition; it used to be dropped).
    #[serde(rename = "Legato", default)]
    pub legato: Option<LegatoInfo>,
    #[serde(rename = "FreeText", default)]
    pub free_text: Option<String>,
    /// GP6/GP7 lyric syllables on the beat: `<Lyrics><Line>..</Line>..</Lyrics>`, one
    /// `<Line>` per lyric line.
    #[serde(rename = "Lyrics", default)]
    pub lyrics: Option<BeatLyrics>,
    /// GP7's octave sign on the beat: `8va`, `8vb`, `15ma` or `15mb`.
    #[serde(rename = "Ottavia", default)]
    pub ottavia: Option<String>,
    /// A chord diagram attached to the beat (its id in the track's diagram collection).
    #[serde(rename = "Chord", default)]
    pub chord: Option<String>,
    /// GuitarScore addition: a rolled chord's direction (`Up` or `Down`).
    #[serde(rename = "Arpeggio", default)]
    pub arpeggio: Option<String>,
    /// GuitarScore addition: a dead slapped beat (an empty `<DeadSlapped />`).
    #[serde(rename = "DeadSlapped", default)]
    pub dead_slapped: Option<EnableTag>,
    #[serde(rename = "Properties", default)]
    pub properties: Option<BeatPropertiesWrapper>,
    /// GuitarScore addition: Guitar Pro's opaque layout properties for this beat (beam
    /// overrides and GP8's design-mode data), kept verbatim.
    #[serde(rename = "XProperties", default)]
    pub x_properties: Option<XPropertiesWrapper>,
}

/// The `<Lyrics>` of a beat: one syllable per lyric line (often empty).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BeatLyrics {
    #[serde(rename = "Line", default)]
    pub lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BeatPropertiesWrapper {
    #[serde(rename = "Property", default)]
    pub properties: Vec<BeatProperty>,
}

/// The shape of a GP7 whammy-bar effect, in hundredths of a semitone.
#[derive(Debug, Deserialize)]
pub(crate) struct Whammy {
    #[serde(rename = "@originValue", default)]
    pub origin_value: f64,
    /// GuitarScore addition: the turning point of a dip (down and back up).
    #[serde(rename = "@middleValue", default)]
    pub middle_value: Option<f64>,
    #[serde(rename = "@destinationValue", default)]
    pub destination_value: f64,
    /// GuitarScore addition: where the middle and the destination are reached, in percent
    /// of the beat.
    #[serde(rename = "@middleOffset1", default)]
    pub middle_offset: Option<f64>,
    #[serde(rename = "@destinationOffset", default)]
    pub destination_offset: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BeatProperty {
    #[serde(rename = "@name", default)]
    pub name: String,
    #[serde(rename = "Direction", default)]
    pub direction: Option<String>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "Enable", default)]
    pub enable: Option<EnableTag>,
    #[serde(rename = "Float", default)]
    pub float: Option<f64>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "Flags", default)]
    pub flags: Option<i32>,
    /// Strength of `VibratoWTremBar` ("Slight" or "Wide").
    #[serde(rename = "Strength", default)]
    pub strength: Option<String>,
    /// GuitarScore addition: a stroke's speed (a note-value denominator) in a
    /// `GuitarScoreBrush`/`GuitarScoreArpeggio` property.
    #[serde(rename = "Duration", default)]
    pub duration: Option<i32>,
    /// GuitarScore addition: `on` or `before` in the same properties (the stroke's
    /// timing).
    #[serde(rename = "Text", default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RhythmRef {
    #[serde(rename = "@ref", default)]
    pub r#ref: i32,
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct NotesWrapper {
    #[serde(rename = "Note", default)]
    pub notes: Vec<Note>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Note {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "Properties")]
    pub properties: NoteProperties,
    #[serde(rename = "Tie", default)]
    pub tie: Option<TieInfo>,
    #[serde(rename = "Vibrato", default)]
    pub vibrato: Option<String>,
    #[serde(rename = "LetRing", default)]
    pub let_ring: Option<EnableTag>,
    #[serde(rename = "AntiAccent", default)]
    pub anti_accent: Option<String>,
    #[serde(rename = "Accent", default)]
    pub accent: Option<i32>,
    #[serde(rename = "Trill", default)]
    pub trill: Option<i32>,
    #[serde(rename = "Ornament", default)]
    pub ornament: Option<String>,
    /// GuitarScore addition: the fingers (`P`, `I`, `M`, `A`, `C`, and `Open` for the left
    /// hand).
    #[serde(rename = "LeftFingering", default)]
    pub left_fingering: Option<String>,
    #[serde(rename = "RightFingering", default)]
    pub right_fingering: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TieInfo {
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "@origin", default)]
    pub origin: String,
    #[serde(rename = "@destination", default)]
    pub destination: String,
}

/// The `<Legato>` tag of a beat; both attributes are `"true"` or `"false"`.
#[derive(Debug, Deserialize)]
pub(crate) struct LegatoInfo {
    #[serde(rename = "@origin", default)]
    pub origin: String,
    #[serde(rename = "@destination", default)]
    pub destination: String,
}

/// An empty self-closing tag used as a presence flag (e.g., `<Enable />`, `<LetRing />`).
#[derive(Debug, Deserialize)]
pub(crate) struct EnableTag;

#[derive(Debug, Deserialize)]
pub(crate) struct NoteProperties {
    #[serde(rename = "Property", default)]
    pub properties: Vec<Property>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Property {
    #[serde(rename = "@name", default)]
    pub name: String,
    // Value sub-elements — each property uses at most one of these
    #[serde(rename = "Fret", default)]
    pub fret: Option<i32>,
    #[serde(rename = "String", default)]
    pub string: Option<i32>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "Pitch", default)]
    pub pitch: Option<Pitch>,
    /// A plain number (GuitarScore uses it for the channel's reverb and chorus sends).
    #[serde(rename = "Number", default)]
    pub number: Option<i32>,
    #[serde(rename = "Enable", default)]
    pub enable: Option<EnableTag>,
    #[serde(rename = "Float", default)]
    pub float: Option<f64>,
    #[serde(rename = "Flags", default)]
    pub flags: Option<i32>,
    #[serde(rename = "HFret", default)]
    pub hfret: Option<f64>,
    #[serde(rename = "HType", default)]
    pub htype: Option<String>,
    #[serde(rename = "Pitches", default)]
    pub pitches: Option<String>,
    /// GuitarScore addition: user-given name of a tuning (empty for presets).
    #[serde(rename = "Label", default)]
    pub label: Option<String>,
    /// GuitarScore addition: a text value (the track's symbol, when it is a string).
    #[serde(rename = "Text", default)]
    pub text: Option<String>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "Direction", default)]
    pub direction: Option<String>,
    /// GuitarScore addition: a chord-diagram collection's items.
    #[serde(rename = "Items", default)]
    pub items: Option<DiagramItems>,
    /// GuitarScore addition: a GP6 drum note's kit element (which drum)...
    #[serde(rename = "Element", default)]
    pub element: Option<i32>,
    /// ...and its variation (how it is struck).
    #[serde(rename = "Variation", default)]
    pub variation: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DiagramItems {
    #[serde(rename = "Item", default)]
    pub items: Vec<DiagramItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DiagramItem {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "@name", default)]
    pub name: String,
    #[serde(rename = "Diagram", default)]
    pub diagram: Option<Diagram>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Diagram {
    #[serde(rename = "@stringCount", default)]
    pub string_count: i32,
    #[serde(rename = "@fretCount", default)]
    pub fret_count: i32,
    #[serde(rename = "@baseFret", default)]
    pub base_fret: i32,
    #[serde(rename = "Fret", default)]
    pub frets: Vec<DiagramFret>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DiagramFret {
    #[serde(rename = "@string", default)]
    pub string: i32,
    #[serde(rename = "@fret", default)]
    pub fret: i32,
}

#[expect(
    dead_code,
    reason = "keyboard and vocal notes are stored by pitch; not read yet"
)]
#[derive(Debug, Deserialize)]
pub(crate) struct Pitch {
    #[serde(rename = "Step", default)]
    pub step: String,
    #[serde(rename = "Octave", default)]
    pub octave: i32,
    #[serde(rename = "Accidental", default)]
    pub accidental: Option<String>,
}

// ---------------------------------------------------------------------------
// Rhythms
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct RhythmsWrapper {
    #[serde(rename = "Rhythm", default)]
    pub rhythms: Vec<Rhythm>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Rhythm {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "NoteValue", default)]
    pub note_value: String,
    #[serde(rename = "AugmentationDot", default)]
    pub augmentation_dot: Option<AugmentationDot>,
    #[serde(rename = "PrimaryTuplet", default)]
    pub primary_tuplet: Option<PrimaryTuplet>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AugmentationDot {
    #[serde(rename = "@count", default)]
    pub count: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrimaryTuplet {
    #[serde(rename = "@num", default)]
    pub num: i32,
    #[serde(rename = "@den", default)]
    pub den: i32,
}
