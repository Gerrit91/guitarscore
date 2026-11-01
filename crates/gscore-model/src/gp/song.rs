//! Guitar Pro song data: song.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see LICENSE-guitarpro next to this file). Changes: docs/guitarpro.md.

use crate::gp::*;

// Struct utility to read file: https://stackoverflow.com/questions/55555538/what-is-the-correct-way-to-read-a-binary-file-in-chunks-of-a-fixed-size-and-stor
/// A musical-position → audio-time anchor used to synchronize the score with an
/// embedded backing track (**GP8** feature). Derived from `<Automation
/// Type="SyncPoint">` in the GPIF; `frame_offset` is an absolute audio-frame
/// offset, so `frame_offset / sample_rate` yields a playback timestamp.
#[derive(Debug, Clone, Default)]
pub struct SyncPoint {
    pub bar_index: i32,
    pub bar_occurrence: i32,
    pub modified_tempo: Option<f64>,
    pub original_tempo: Option<f64>,
    pub frame_offset: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Song {
    pub version: Version,
    pub clipboard: Option<Clipboard>,

    pub name: String,
    pub subtitle: String, //Guitar Pro
    pub artist: String,
    pub album: String,
    pub words: String,  //GP
    pub author: String, //music by
    pub date: String,
    pub copyright: String,
    /// Tab writer
    pub writer: String,
    pub transcriber: String,
    pub instructions: String,
    pub comments: String,
    pub notice: Vec<String>,

    pub tracks: Vec<Track>,
    pub measure_headers: Vec<MeasureHeader>,
    pub channels: Vec<MidiChannel>,
    pub lyrics: Lyrics,
    pub tempo: i16,
    pub hide_tempo: bool,
    pub tempo_name: String,
    pub key: KeySignature,

    /// SyncPoint anchors (GP8), in file order.
    pub sync_points: Vec<SyncPoint>,
    /// Embedded backing-track audio (e.g. `Content/Assets/*.mp3`), raw bytes
    /// (GP8 feature).
    pub backing_track_audio: Option<Vec<u8>>,

    pub triplet_feel: TripletFeel,
    pub master_effect: RseMasterEffect,

    pub page_setup: PageSetup,

    //Used to read the file
    pub current_measure_number: Option<usize>,
    pub current_track: Option<usize>,
    pub current_voice_number: Option<usize>,
    pub current_beat_number: Option<usize>,
}

impl Default for Song {
    fn default() -> Self {
        Song {
            version: Version {
                data: String::with_capacity(30),
                clipboard: false,
                number: (5, 1, 0),
            },
            clipboard: None,
            name: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            album: String::new(),
            words: String::new(),
            author: String::new(),
            date: String::new(),
            copyright: String::new(),
            writer: String::new(),
            transcriber: String::new(),
            comments: String::new(),
            notice: Vec::new(),
            instructions: String::new(),
            tracks: Vec::new(),
            measure_headers: Vec::new(),
            channels: Vec::with_capacity(64),
            lyrics: Lyrics::default(),
            tempo: 120,
            hide_tempo: false,
            tempo_name: String::from("Moderate"),
            key: KeySignature::default(),
            sync_points: Vec::new(),
            backing_track_audio: None,

            triplet_feel: TripletFeel::None,
            current_measure_number: None,
            current_track: None,
            current_voice_number: None,
            current_beat_number: None,

            page_setup: PageSetup::default(),

            master_effect: RseMasterEffect::default(),
        }
    }
}
