//! Guitar Pro song data: headers.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub data: String,
    pub number: (u8, u8, u8),
    pub clipboard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipboard {
    pub start_measure: i32,
    pub stop_measure: i32,
    pub start_track: i32,
    pub stop_track: i32,
    pub start_beat: i32,
    pub stop_beat: i32,
    pub sub_bar_copy: bool,
}

impl Default for Clipboard {
    fn default() -> Self {
        Clipboard {
            start_measure: 1,
            stop_measure: 1,
            start_track: 1,
            stop_track: 1,
            start_beat: 1,
            stop_beat: 1,
            sub_bar_copy: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeasureHeader {
    pub number: u16,
    pub start: i64,
    pub time_signature: TimeSignature,
    pub tempo: i32,
    pub marker: Option<Marker>,
    pub repeat_open: bool,
    pub repeat_alternative: u8,
    pub repeat_close: i8,
    pub triplet_feel: TripletFeel,
    pub direction: Option<DirectionSign>,
    /// Tonality of the measure
    pub key_signature: KeySignature,
    pub double_bar: bool,
    /// Fermatas from GPIF (GP6/GP7)
    pub fermatas: Vec<(String, String)>,
    /// Free time (no metronome) from GPIF (GP6/GP7)
    pub free_time: bool,
}

impl Default for MeasureHeader {
    fn default() -> Self {
        MeasureHeader {
            number: 1,
            start: DURATION_QUARTER_TIME,
            tempo: 0,
            repeat_open: false,
            repeat_alternative: 0,
            repeat_close: -1,
            triplet_feel: TripletFeel::None,
            direction: None,
            key_signature: KeySignature::default(),
            double_bar: false,
            marker: None,
            time_signature: TimeSignature {
                numerator: 4,
                denominator: Duration::default(),
                beams: vec![2, 2, 2, 2],
            },
            fermatas: Vec::new(),
            free_time: false,
        }
    }
}

/// A marker annotation for beats.
#[derive(Debug, Clone)]
pub struct Marker {
    pub title: String,
    pub color: i32,
}

impl Marker {
    /// The color a marker gets when the file has none: GP6/GP7 store no marker color. A
    /// light grey stays calm on the app's dark song grid, unlike Guitar Pro's red.
    pub const DEFAULT_COLOR: i32 = 0x00_B0_B0_B0;
}

impl Default for Marker {
    fn default() -> Self {
        Marker {
            title: "Section".to_owned(),
            color: Self::DEFAULT_COLOR,
        }
    }
}

/// This class can store the information about a group of measures which are repeated.
#[derive(Debug, Clone, Default)]
pub struct RepeatGroup {
    pub measure_headers: Vec<usize>,
    pub closings: Vec<usize>,
    pub openings: Vec<usize>,
    pub is_closed: bool,
}
