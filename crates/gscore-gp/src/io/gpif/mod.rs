//! The GPIF XML of GP6 and GP7+ files, as `serde` structures: score metadata, master
//! track and tracks here, measures down to notes in [`bars`].

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod bars;

use serde::Deserialize;

pub(crate) use bars::*;

#[derive(Debug, Deserialize)]
pub(crate) struct Gpif {
    /// GP7 uses "GPVersion", GP6 uses "GPRevision"
    #[serde(rename = "GPVersion", default)]
    pub version: Option<String>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
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
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "Tracks", default)]
    pub tracks_count: String,
    #[serde(rename = "Automations", default)]
    pub automations: Option<AutomationsWrapper>,
    /// GuitarScore addition: the master effects, which hold the master volume and pan.
    #[serde(rename = "RSE", default)]
    pub rse: Option<MasterRse>,
}

/// The master track's `<RSE>` block.
#[derive(Debug, Deserialize)]
pub(crate) struct MasterRse {
    #[serde(rename = "Master", default)]
    pub master: Option<MasterEffects>,
}

/// `<Master>`: the master effect chain.
#[derive(Debug, Deserialize)]
pub(crate) struct MasterEffects {
    #[serde(rename = "Effect", default)]
    pub effects: Vec<MasterEffect>,
}

/// One master effect: its id and its parameter list.
#[derive(Debug, Deserialize)]
pub(crate) struct MasterEffect {
    #[serde(rename = "@id", default)]
    pub id: String,
    #[serde(rename = "Parameters", default)]
    pub parameters: Option<String>,
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
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
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
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
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
    /// GP7: the MIDI port and channels of the track (GP6 uses `GeneralMidi`).
    #[serde(rename = "MidiConnection", default)]
    pub midi_connection: Option<MidiConnection>,
    /// GP7: whether the track is playing normally, soloed or muted.
    #[serde(rename = "PlaybackState", default)]
    pub playback_state: Option<String>,
    /// GP7: the track's RSE channel strip, which stores the mixer volume and pan.
    #[serde(rename = "RSE", default)]
    pub rse: Option<TrackRse>,
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
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "SecondaryChannel", default)]
    pub secondary_channel: Option<i32>,
}

/// GP7's `<MidiConnection>`: where a track sends its MIDI.
#[derive(Debug, Deserialize)]
pub(crate) struct MidiConnection {
    #[serde(rename = "Port", default)]
    pub port: Option<i32>,
    #[serde(rename = "PrimaryChannel", default)]
    pub primary_channel: Option<i32>,
    #[expect(dead_code, reason = "part of the GPIF format; parsed, not used yet")]
    #[serde(rename = "SecondaryChannel", default)]
    pub secondary_channel: Option<i32>,
}

/// GP7's `<RSE>` block of a track.
#[derive(Debug, Deserialize)]
pub(crate) struct TrackRse {
    #[serde(rename = "ChannelStrip", default)]
    pub channel_strip: Option<ChannelStrip>,
}

/// The RSE channel strip; its `<Parameters>` hold the track's mixer state.
#[derive(Debug, Deserialize)]
pub(crate) struct ChannelStrip {
    #[serde(rename = "Parameters", default)]
    pub parameters: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Transpose {
    #[serde(rename = "Chromatic", default)]
    pub chromatic: Option<i32>,
    #[serde(rename = "Octave", default)]
    pub octave: Option<i32>,
}
