//! Guitar Pro song data: track.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

/// Settings of the track.
#[derive(Debug, Clone)]
pub struct TrackSettings {
    pub tablature: bool,
    pub notation: bool,
    /// GuitarScore addition: GP7's slash-notation flag (`PartConfiguration`, bit 2).
    pub slash: bool,
    pub diagram_are_below: bool,
    pub show_rhythm: bool,
    pub force_horizontal: bool,
    pub force_channels: bool,
    pub diagram_list: bool,
    pub diagram_in_score: bool,
    pub auto_let_ring: bool,
    pub auto_brush: bool,
    pub extend_rhythmic: bool,
}

impl Default for TrackSettings {
    fn default() -> Self {
        TrackSettings {
            tablature: true,
            notation: true,
            slash: false,
            diagram_are_below: false,
            show_rhythm: false,
            force_horizontal: false,
            force_channels: false,
            diagram_list: true,
            diagram_in_score: false,
            auto_let_ring: false,
            auto_brush: false,
            extend_rhythmic: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub number: i32,
    pub offset: i32,
    pub channel_index: usize, //pub channel_id: i32,
    pub solo: bool,
    pub mute: bool,
    pub visible: bool,
    pub name: String,
    pub short_name: String,
    /// A guitar string with a special tuning.
    pub strings: Vec<(i8, i8)>,
    pub color: i32,
    pub percussion_track: bool,
    pub twelve_stringed_guitar_track: bool,
    pub banjo_track: bool,
    pub port: u8,
    pub fret_count: u8,
    pub indicate_tuning: bool,
    pub use_rse: bool,
    pub rse: TrackRse,
    pub measures: Vec<Measure>,
    pub settings: TrackSettings,
    /// MIDI program from GPIF (GP6/GP7)
    pub midi_program_gpif: Option<i32>,
    /// Chromatic transposition (GP6/GP7)
    pub transpose_chromatic: i32,
    /// Octave transposition (GP6/GP7)
    pub transpose_octave: i32,
    /// GuitarScore addition: false if the file stores no tuning for this track (e.g. GP7
    /// vocal or piano tracks). `strings` then holds placeholder E-standard strings, which
    /// are not written back.
    pub has_tuning: bool,
    /// GuitarScore addition: GP7 instrument type id (`electricGuitar`, `drumKit`, `voice`,
    /// ...), `None` for formats that do not store one. Not written back yet.
    pub instrument_type: Option<String>,
    /// GuitarScore addition: GP7 instrument display name ("Electric Guitar"), if stored.
    pub instrument_name: Option<String>,
    /// GuitarScore addition: user-given tuning name from the file (GP6/GP7), if set.
    /// Not written back yet.
    pub tuning_label: Option<String>,
    /// GuitarScore addition: at most this many bars on one line of the page; `None` lays
    /// lines out by width alone. GP7 files keep it in a GuitarScore attribute of the track
    /// (`gsBarsPerSystem`).
    pub bars_per_system: Option<u8>,
    /// GuitarScore addition: the symbol the app draws for the track (`electricGuitar`,
    /// `drums`, `piano`, ...), independent of the instrument's sound. GP7 files keep it as
    /// a track property (`GuitarScoreSymbol`); older formats drop it.
    pub symbol: Option<String>,
}

impl Default for Track {
    fn default() -> Self {
        Track {
            number: 1,
            offset: 0,
            channel_index: 0, //channel_id: 25,
            solo: false,
            mute: false,
            visible: true,
            name: String::from("Track 1"),
            short_name: String::new(),
            strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)],
            banjo_track: false,
            twelve_stringed_guitar_track: false,
            percussion_track: false,
            fret_count: 24,
            color: 0xff0000,
            port: 1,
            indicate_tuning: false,
            use_rse: false,
            rse: TrackRse::default(),
            measures: Vec::new(),
            settings: TrackSettings::default(),
            midi_program_gpif: None,
            transpose_chromatic: 0,
            transpose_octave: 0,
            has_tuning: true,
            instrument_type: None,
            instrument_name: None,
            tuning_label: None,
            bars_per_system: None,
            symbol: None,
        }
    }
}
