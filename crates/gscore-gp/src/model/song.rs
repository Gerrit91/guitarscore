//! Guitar Pro song data: song.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

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
    /// GuitarScore addition: Guitar Pro's `<WordsAndMusic>` credit line, kept as written
    /// (the reader also uses it for an empty words or music field).
    pub words_and_music: String,
    /// GuitarScore addition: Guitar Pro's page header and footer templates (HTML in CDATA,
    /// with `%TITLE%`-style placeholders), kept verbatim so a save does not lose them.
    pub page_header: String,
    pub page_footer: String,
    pub first_page_header: String,
    pub first_page_footer: String,

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
    /// GuitarScore addition: GP7's master volume and pan (the `I01_VolumeAndPan` master
    /// effect), each 0.0 to 1.0 (pan 0.5 is the middle); `None` if the file had none.
    pub master_volume_pan: Option<(f32, f32)>,
    /// GuitarScore addition: Guitar Pro's master effect chain (every `<Effect>` of the
    /// master `<RSE>`), kept so a save writes its effects back; only the volume and pan one
    /// is used (see [`Song::master_volume_pan`]) and is written from the model.
    pub gpif_master_effects: Vec<MasterEffectInfo>,
    /// GuitarScore addition: Guitar Pro 8's `<AudioTracks>` element exactly as written
    /// (mostly empty), kept so a save does not lose it.
    pub gpif_audio_tracks: Option<String>,
    /// GuitarScore addition: Guitar Pro's `<ScoreViews>` element exactly as written (a list
    /// of view ids), kept verbatim. It may list ids that no longer match the tracks after
    /// heavy editing; mapping it to GuitarScore's own views is still open.
    pub gpif_score_views: Option<String>,
    /// GuitarScore addition: the bar-0 tempo automation's `<Text>` ("Moderate", "Allegro"),
    /// if the file names its tempo there; kept so a save does not lose it.
    pub gpif_tempo_text: Option<String>,

    pub page_setup: PageSetup,
    /// GuitarScore addition: Guitar Pro's systems layout for all tracks shown together
    /// (`ScoreSystemsLayout`, bars per line, `"4 4 2"`), and its default bars per line;
    /// GP7 only, kept as written.
    pub score_systems_layout: Option<String>,
    pub score_systems_default: Option<String>,
    /// GuitarScore addition: GP7's style sheet, kept so saving writes back what is not
    /// edited (the page setup is read from it and written into it).
    pub stylesheet: Option<Stylesheet>,
    /// GuitarScore addition: GP7's `LayoutConfiguration` beside the per-track visibility
    /// (which is [`Track::visible`]), kept as read; `None` for a song that had none.
    pub layout_configuration: Option<LayoutConfiguration>,

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
            words_and_music: String::new(),
            page_header: String::new(),
            page_footer: String::new(),
            first_page_header: String::new(),
            first_page_footer: String::new(),
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
            score_systems_layout: None,
            score_systems_default: None,

            triplet_feel: TripletFeel::None,
            current_measure_number: None,
            current_track: None,
            current_voice_number: None,
            current_beat_number: None,

            page_setup: PageSetup::default(),
            stylesheet: None,
            layout_configuration: None,

            master_effect: RseMasterEffect::default(),
            master_volume_pan: None,
            gpif_master_effects: Vec::new(),
            gpif_audio_tracks: None,
            gpif_score_views: None,
            gpif_tempo_text: None,
        }
    }
}

/// GuitarScore addition: one master effect of Guitar Pro's chain (its `<Effect>` element:
/// the id names the effect, `<ByPass/>` switches it off, `<Parameters>` holds its values,
/// and it may carry bypassed automations and automation points). Kept so a save writes back
/// what GuitarScore does not edit.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterEffectInfo {
    pub id: String,
    pub by_pass: bool,
    /// The effect's parameter list exactly as written, if it has one.
    pub parameters: Option<String>,
    /// The automation types Guitar Pro has switched off (`<ByPassedAutomations>`).
    pub bypassed_automations: Option<String>,
    /// The effect's automation points (the master volume's mid-song changes among them).
    pub automations: Vec<MasterAutomation>,
}

/// GuitarScore addition: one automation point of a master effect (Guitar Pro's
/// `<Automation>`), kept so a mid-song master volume change is not lost.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterAutomation {
    pub automation_type: String,
    pub linear: bool,
    /// The 0-based bar it sits in.
    pub bar: i32,
    /// The fraction of the bar where it happens.
    pub position: f32,
    pub visible: bool,
    /// Its own text label (`<Text>`, e.g. a tempo's name), if it has one.
    pub text: Option<String>,
    /// Its value exactly as written (`0.66`, `125 2`, ...).
    pub value: String,
}

/// GuitarScore addition: the parts of GP7's `Content/LayoutConfiguration` beside each
/// track's visibility, whose meaning is not known; kept so saving writes them back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutConfiguration {
    /// The leading big-endian number (4 to 7 in the files seen).
    pub kind: u32,
    /// The two bytes after it (`01 00` or `03 00`).
    pub flags: [u8; 2],
    /// The trailing number, which looks like the selected track.
    pub selected: u32,
}

impl Default for LayoutConfiguration {
    /// What Guitar Pro 7 writes for a song nobody changed the view of.
    fn default() -> Self {
        LayoutConfiguration {
            kind: 4,
            flags: [1, 0],
            selected: 0,
        }
    }
}
