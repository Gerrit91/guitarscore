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
    /// GuitarScore addition: the parts of each track that are kept verbatim, by track
    /// index. They are not part of the serde model because they must survive unchanged
    /// (Guitar Pro's `<Sounds>` and `<AudioEngineState>`); see [`capture_track_raw`].
    #[serde(skip)]
    pub track_raw: Vec<TrackRaw>,
}

/// The parts of one GPIF track that are kept exactly as the file has them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrackRaw {
    /// The whole `<Sounds>` element: the RSE sound chosen for the track (its path,
    /// bank patch, pickups and effect chain), which GuitarScore does not edit yet.
    pub sounds: Option<String>,
    /// The whole `<AudioEngineState>` element (`RSE` or `MIDI`).
    pub audio_engine: Option<String>,
}

/// One element captured verbatim from a track.
#[derive(Debug, Clone, Copy)]
enum RawElement {
    Sounds,
    AudioEngine,
}

/// Captures the raw XML of the elements of each `score.gpif` track that are kept
/// verbatim: its `<Sounds>` and `<AudioEngineState>`. The result has one entry per
/// `<Track>` element, in file order, so it lines up with the track index.
///
/// This walks the XML a second time instead of extending the serde model: `<Sounds>`
/// holds CDATA (`<Name>`, `<Label>`), an open-ended effect chain and fields of newer
/// Guitar Pro versions, all of which must survive a load-save cycle unchanged. A file
/// whose XML the reader cannot walk simply keeps nothing extra (deserialization has
/// already reported a real parse error before this runs).
pub(crate) fn capture_track_raw(xml: &str) -> Vec<TrackRaw> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut tracks: Vec<TrackRaw> = Vec::new();
    let mut depth: usize = 0;
    // The element being captured: what it is, where its first byte is, and its depth.
    let mut capturing: Option<(RawElement, usize, usize)> = None;
    let position = |reader: &quick_xml::Reader<&[u8]>| {
        usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX)
    };

    loop {
        let start = position(&reader);
        let event = match reader.read_event() {
            Ok(event) => event,
            // The song itself was parsed before; keep the elements captured so far.
            Err(_) => break,
        };
        match &event {
            Event::Start(element) => {
                depth = depth.saturating_add(1);
                let name = element.name();
                let name = name.as_ref();
                if name == "Track" && depth == 3 {
                    // `<GPIF><Tracks><Track>`: one entry per track, in order.
                    tracks.push(TrackRaw::default());
                } else if capturing.is_none() && !tracks.is_empty() {
                    let captured = match name {
                        "Sounds" => Some(RawElement::Sounds),
                        "AudioEngineState" => Some(RawElement::AudioEngine),
                        _ => None,
                    };
                    if let Some(captured) = captured {
                        capturing = Some((captured, start, depth));
                    }
                }
            }
            Event::Empty(element) => {
                // A self-closing element is complete at once.
                if capturing.is_none() && !tracks.is_empty() {
                    let name = element.name();
                    let raw = xml.get(start..position(&reader));
                    match (name.as_ref(), raw) {
                        ("Sounds", Some(raw)) => {
                            if let Some(track) = tracks.last_mut() {
                                track.sounds = Some(raw.to_owned());
                            }
                        }
                        ("AudioEngineState", Some(raw)) => {
                            if let Some(track) = tracks.last_mut() {
                                track.audio_engine = Some(raw.to_owned());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::End(_) => {
                if let Some((captured, begin, element_depth)) = capturing
                    && element_depth == depth
                {
                    let raw = xml.get(begin..position(&reader));
                    if let (Some(raw), Some(track)) = (raw, tracks.last_mut()) {
                        match captured {
                            RawElement::Sounds => track.sounds = Some(raw.to_owned()),
                            RawElement::AudioEngine => track.audio_engine = Some(raw.to_owned()),
                        }
                    }
                    capturing = None;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    tracks
}

/// Regression tests for the verbatim track elements.
#[cfg(test)]
mod raw_tests {
    use super::*;

    #[test]
    fn sounds_and_audio_engine_are_captured_verbatim_per_track() {
        let xml = "<GPIF><Tracks count=\"2\">\
            <Track id=\"0\"><Name>A</Name>\
            <Sounds><Sound><Name><![CDATA[Blues]]></Name><Path>Stringed/EG</Path>\
            <RSE><EffectChain><Effect id=\"E1\"><Parameters>0.1 0.2</Parameters></Effect>\
            </EffectChain></RSE></Sound></Sounds>\
            <AudioEngineState>RSE</AudioEngineState></Track>\
            <Track id=\"1\"><Name>B</Name><Sounds/></Track>\
            </Tracks></GPIF>";
        let raw = capture_track_raw(xml);
        assert_eq!(raw.len(), 2);
        let first = raw.first().expect("one per track");
        assert_eq!(
            first.sounds.as_deref(),
            Some(
                "<Sounds><Sound><Name><![CDATA[Blues]]></Name><Path>Stringed/EG</Path>\
                 <RSE><EffectChain><Effect id=\"E1\"><Parameters>0.1 0.2</Parameters></Effect>\
                 </EffectChain></RSE></Sound></Sounds>"
            )
        );
        assert_eq!(
            first.audio_engine.as_deref(),
            Some("<AudioEngineState>RSE</AudioEngineState>")
        );
        let second = raw.get(1).expect("second track");
        assert_eq!(second.sounds.as_deref(), Some("<Sounds/>"));
        assert!(second.audio_engine.is_none());
    }

    #[test]
    fn a_missing_sounds_leaves_none_and_foreign_tracks_do_not_shift_the_indices() {
        // `<MasterTrack><Tracks>` holds ids, not `<Track>` elements: it must not count.
        let xml = "<GPIF><MasterTrack><Tracks>0 1</Tracks></MasterTrack>\
            <Tracks><Track id=\"0\"><Name>A</Name></Track>\
            <Track id=\"1\"><Name>B</Name><Sounds><Sound/></Sounds></Track></Tracks></GPIF>";
        let raw = capture_track_raw(xml);
        assert_eq!(raw.len(), 2);
        assert!(raw.first().expect("first").sounds.is_none());
        assert_eq!(
            raw.get(1).and_then(|track| track.sounds.as_deref()),
            Some("<Sounds><Sound/></Sounds>")
        );
    }
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
    /// Bars per line when every track is shown (Guitar Pro's multitrack view), `"4 4 2"`;
    /// the per-track `SystemsLayout` is for one track on its own.
    #[serde(rename = "ScoreSystemsLayout", default)]
    pub score_systems_layout: Option<String>,
    /// The default bars per line of the multitrack view.
    #[serde(rename = "ScoreSystemsDefaultLayout", default)]
    pub score_systems_default: Option<String>,
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
    /// GuitarScore addition: how many bars each system (line) of the page holds, e.g.
    /// `"4 4 3"` (GP7; the last count goes on for the rest).
    #[serde(rename = "SystemsLayout", default)]
    pub systems_layout: Option<String>,
    /// GuitarScore addition: the bars at which a new page starts (0-based), a GuitarScore
    /// attribute (Guitar Pro has no page breaks).
    #[serde(rename = "@gsPageBreaks", default)]
    pub page_breaks: Option<String>,
    /// GuitarScore addition: at most this many bars per line, a GuitarScore attribute.
    #[serde(rename = "@gsBarsPerSystem", default)]
    pub bars_per_system: Option<u8>,
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
    /// GuitarScore addition: the track's lyric lines as typed (GP6/GP7 `<Lyrics>`; the
    /// syllables are also on the beats).
    #[serde(rename = "Lyrics", default)]
    pub lyrics: Option<TrackLyrics>,
}

/// GuitarScore addition: `<Lyrics>` of a track, one `<Line>` per lyric line.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct TrackLyrics {
    #[serde(rename = "Line", default)]
    pub lines: Vec<TrackLyricLine>,
}

/// One lyric line of a track: its text and the bar it starts at (0-based).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct TrackLyricLine {
    #[serde(rename = "Text", default)]
    pub text: String,
    #[serde(rename = "Offset", default)]
    pub offset: i32,
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
