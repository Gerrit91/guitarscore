// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Gpif {
    /// GP7 uses "GPVersion", GP6 uses "GPRevision"
    #[serde(rename = "GPVersion", default)]
    pub version: Option<String>,
    #[serde(rename = "GPRevision", default)]
    pub revision: Option<String>,
    #[serde(rename = "Score")]
    pub score: Score,
    #[serde(rename = "MasterTrack")]
    pub master_track: MasterTrack,
    #[serde(rename = "Tracks")]
    pub tracks: TracksWrapper,
    #[serde(rename = "MasterBars")]
    pub master_bars: MasterBarsWrapper,
    #[serde(rename = "Bars")]
    pub bars: BarsWrapper,
    #[serde(rename = "Voices")]
    pub voices: VoicesWrapper,
    #[serde(rename = "Beats")]
    pub beats: BeatsWrapper,
    #[serde(rename = "Notes")]
    pub notes: NotesWrapper,
    #[serde(rename = "Rhythms")]
    pub rhythms: RhythmsWrapper,
}

// ---------------------------------------------------------------------------
// Score metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Score {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub sub_title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(rename = "Words", default)]
    pub words: String,
    #[serde(rename = "Music", default)]
    pub music: String,
    /// GuitarScore addition: used instead of Words and Music when both are the same person.
    #[serde(default)]
    pub words_and_music: String,
    #[serde(default)]
    pub copyright: String,
    #[serde(default)]
    pub tabber: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub notices: String,
}

// ---------------------------------------------------------------------------
// MasterTrack (tempo automations)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct MasterTrack {
    #[serde(rename = "Tracks", default)]
    pub tracks_count: String,
    #[serde(rename = "Automations", default)]
    pub automations: Option<AutomationsWrapper>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AutomationsWrapper {
    #[serde(rename = "Automation", default)]
    pub automations: Vec<Automation>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Automation {
    #[serde(rename = "Type", default)]
    pub automation_type: String,
    #[serde(rename = "Value", default)]
    pub value: Option<AutomationValue>,
    #[serde(rename = "Bar", default)]
    pub bar: i32,
    // GP7 stores this as a fractional position within the bar in `[0.0, 1.0)`
    // (e.g. `0.75`), not an integer tick offset.
    #[serde(rename = "Position", default)]
    pub position: f32,
}

/// `<Value>` content of an automation. Most automation types (Tempo, …) use
/// plain text (`<Value>125 2</Value>`); **GP8** `SyncPoint` automations use a
/// structured element instead:
///   `<Value><BarIndex>0</BarIndex><BarOccurrence>0</BarOccurrence>…</Value>`
///
/// quick-xml maps named child elements onto matching fields (consuming the
/// element), while any *unmatched* element or bare text lands in `items`
/// (the `$value` catch-all). Verified experimentally with quick-xml 0.41:
/// both channels fill independently, so future GP versions that add more
/// children stay parseable and their data is preserved in `items`.
#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct AutomationValue {
    /// Unmatched child elements / bare text, in document order.
    #[serde(rename = "$value", default)]
    pub items: Vec<String>,
    /// SyncPoint: the master bar this anchor belongs to.
    #[serde(rename = "BarIndex", default)]
    pub bar_index: Option<i32>,
    /// SyncPoint: which repeat occurrence of the bar (0 = first pass).
    #[serde(rename = "BarOccurrence", default)]
    pub bar_occurrence: Option<i32>,
    /// SyncPoint: tempo the *score* uses at this anchor (after this point).
    #[serde(rename = "ModifiedTempo", default)]
    pub modified_tempo: Option<f64>,
    /// SyncPoint: original song tempo.
    #[serde(rename = "OriginalTempo", default)]
    pub original_tempo: Option<f64>,
    /// SyncPoint: absolute audio-frame offset into the backing track.
    #[serde(rename = "FrameOffset", default)]
    pub frame_offset: Option<i64>,
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct TracksWrapper {
    #[serde(rename = "Track", default)]
    pub tracks: Vec<Track>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Track {
    #[serde(rename = "@id", default)]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "ShortName", default)]
    pub short_name: String,
    #[serde(rename = "Color", default)]
    pub color: Option<String>,
    /// GP6: track-level properties (Tuning, DiagramCollection, etc.)
    #[serde(rename = "Properties", default)]
    pub properties: Option<TrackPropertiesWrapper>,
    /// GP7: staves with per-staff properties
    #[serde(rename = "Staves", default)]
    pub staves: Option<StavesWrapper>,
    #[serde(rename = "GeneralMidi", default)]
    pub general_midi: Option<GeneralMidi>,
    #[serde(rename = "Transpose", default)]
    pub transpose: Option<Transpose>,
    /// GuitarScore addition: the instrument (GP7), e.g. type `electricGuitar`, `drumKit`.
    #[serde(rename = "InstrumentSet", default)]
    pub instrument_set: Option<InstrumentSet>,
    /// GuitarScore addition: the track's sounds (GP7). GP7 names the MIDI program here
    /// instead of in `<GeneralMidi>`.
    #[serde(rename = "Sounds", default)]
    pub sounds: Option<SoundsWrapper>,
}

/// GuitarScore addition: `<Sounds>` of a GP7 track.
#[derive(Debug, Deserialize)]
pub(crate) struct SoundsWrapper {
    #[serde(rename = "Sound", default)]
    pub sounds: Vec<Sound>,
}

/// GuitarScore addition: one `<Sound>` of a GP7 track (only the MIDI part).
#[derive(Debug, Deserialize)]
pub(crate) struct Sound {
    #[serde(rename = "MIDI", default)]
    pub midi: Option<SoundMidi>,
}

/// GuitarScore addition: the `<MIDI>` part of a GP7 `<Sound>`.
#[derive(Debug, Deserialize)]
pub(crate) struct SoundMidi {
    #[serde(rename = "Program", default)]
    pub program: Option<i32>,
}

/// GuitarScore addition: `<InstrumentSet>` of a GP7 track (only the fields used).
#[derive(Debug, Deserialize)]
pub(crate) struct InstrumentSet {
    /// Display name, e.g. "Electric Guitar".
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    /// Type id, e.g. `electricGuitar`, `electricBass`, `drumKit`, `voice`.
    #[serde(rename = "Type", default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TrackPropertiesWrapper {
    #[serde(rename = "Property", default)]
    pub properties: Vec<Property>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StavesWrapper {
    #[serde(rename = "Staff", default)]
    pub staves: Vec<Staff>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Staff {
    #[serde(rename = "Properties", default)]
    pub properties: Option<StaffPropertiesWrapper>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StaffPropertiesWrapper {
    #[serde(rename = "Property", default)]
    pub properties: Vec<Property>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeneralMidi {
    #[serde(rename = "Program", default)]
    pub program: Option<i32>,
    #[serde(rename = "Port", default)]
    pub port: Option<i32>,
    #[serde(rename = "PrimaryChannel", default)]
    pub primary_channel: Option<i32>,
    #[serde(rename = "SecondaryChannel", default)]
    pub secondary_channel: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Transpose {
    #[serde(rename = "Chromatic", default)]
    pub chromatic: Option<i32>,
    #[serde(rename = "Octave", default)]
    pub octave: Option<i32>,
}

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
    #[serde(rename = "FreeTime", default)]
    pub free_time: Option<String>,
    #[serde(rename = "Directions", default)]
    pub directions: Option<DirectionsWrapper>,
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
    #[serde(rename = "Properties", default)]
    pub properties: Option<BeatPropertiesWrapper>,
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
    #[serde(rename = "Enable", default)]
    pub enable: Option<EnableTag>,
    #[serde(rename = "Float", default)]
    pub float: Option<f64>,
    #[serde(rename = "Flags", default)]
    pub flags: Option<i32>,
    /// Strength of `VibratoWTremBar` ("Slight" or "Wide").
    #[serde(rename = "Strength", default)]
    pub strength: Option<String>,
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
}

#[derive(Debug, Deserialize)]
pub(crate) struct TieInfo {
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
    #[serde(rename = "Pitch", default)]
    pub pitch: Option<Pitch>,
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
    #[serde(rename = "Direction", default)]
    pub direction: Option<String>,
}

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
