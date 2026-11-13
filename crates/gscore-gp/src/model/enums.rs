//! Guitar Pro song data: enums.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatStatus {
    Empty,
    Normal,
    Rest,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TupletBracket {
    None,
    Start,
    End,
}

/// Octave signs
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Octave {
    None,
    Ottava,
    Quindicesima,
    OttavaBassa,
    QuindicesimaBassa,
}

/// All beat stroke directions
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatStrokeDirection {
    None,
    Up,
    Down,
}

/// Characteristic of articulation
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlapEffect {
    None,
    Tapping,
    Slapping,
    Popping,
}

/// Voice directions indicating the direction of beams
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceDirection {
    None,
    Up,
    Down,
}

/// Type of the chord.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordType {
    /// Major chord.
    Major,
    /// Dominant seventh chord.
    Seventh,
    /// Major seventh chord.
    MajorSeventh,
    /// Add sixth chord.
    Sixth,
    /// Minor chord.
    Minor,
    /// Minor seventh chord.
    MinorSeventh,
    /// Minor major seventh chord.
    MinorMajor,
    /// Minor add sixth chord.
    MinorSixth,
    /// Suspended second chord.
    SuspendedSecond,
    /// Suspended fourth chord.
    SuspendedFourth,
    /// Seventh suspended second chord.
    SeventhSuspendedSecond,
    /// Seventh suspended fourth chord.
    SeventhSuspendedFourth,
    /// Diminished chord.
    Diminished,
    /// Augmented chord.
    Augmented,
    /// Power chord.
    Power,

    Unknown(u8),
}

/// Tonality of the chord
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordAlteration {
    /// Perfect.
    Perfect,
    /// Diminished.
    Diminished,
    /// Augmented.
    Augmented,
}

/// Extension type of the chord
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordExtension {
    None,
    /// Ninth chord.
    Ninth,
    /// Eleventh chord.
    Eleventh,
    /// Thirteenth chord.
    Thirteenth,
    Unknown(u8),
}

/// All Bend presets
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BendType {
    /// No Preset.
    None,

    // Bends
    /// A simple bend.
    Bend,
    /// A bend and release afterwards.
    BendRelease,
    /// A bend, then release and rebend.
    BendReleaseBend,
    /// Prebend.
    Prebend,
    /// Prebend and then release.
    PrebendRelease,

    // Tremolo Bar
    /// Dip the bar down and then back up.
    Dip,
    /// Dive the bar.
    Dive,
    /// Release the bar up.
    ReleaseUp,
    /// Dip the bar up and then back down.
    InvertedDip,
    /// Return the bar.
    Return,
    /// Release the bar down.
    ReleaseDown,
}

/// All transition types for grace notes.
#[repr(i8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraceEffectTransition {
    /// No transition
    None = 0,
    /// Slide from the grace note to the real one.
    Slide,
    /// Perform a bend from the grace note to the real one.
    Bend,
    /// Perform a hammer on.
    Hammer,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarmonicType {
    Natural = 1,
    Artificial,
    Tapped,
    Pinch,
    Semi,
}

/// An enumeration of different triplet feels.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TripletFeel {
    None,
    Eighth,
    Sixteenth,
    /// GuitarScore addition (GPIF `Dotted8th`): dotted eighth and sixteenth.
    DottedEighth,
    /// GuitarScore addition (GPIF `Dotted16th`).
    DottedSixteenth,
    /// GuitarScore addition (GPIF `Scottish8th`): a short sixteenth, then a dotted eighth.
    ScottishEighth,
    /// GuitarScore addition (GPIF `Scottish16th`).
    ScottishSixteenth,
}

impl TripletFeel {
    /// The GPIF `<TripletFeel>` value, or `None` for straight.
    pub(crate) fn gpif_name(&self) -> Option<&'static str> {
        Some(match self {
            TripletFeel::None => return None,
            TripletFeel::Eighth => "Triplet8th",
            TripletFeel::Sixteenth => "Triplet16th",
            TripletFeel::DottedEighth => "Dotted8th",
            TripletFeel::DottedSixteenth => "Dotted16th",
            TripletFeel::ScottishEighth => "Scottish8th",
            TripletFeel::ScottishSixteenth => "Scottish16th",
        })
    }

    /// The feel a GPIF `<TripletFeel>` value names; unknown values read as straight.
    pub(crate) fn from_gpif_name(name: &str) -> Self {
        match name.trim() {
            "Triplet8th" => TripletFeel::Eighth,
            "Triplet16th" => TripletFeel::Sixteenth,
            "Dotted8th" => TripletFeel::DottedEighth,
            "Dotted16th" => TripletFeel::DottedSixteenth,
            "Scottish8th" => TripletFeel::ScottishEighth,
            "Scottish16th" => TripletFeel::ScottishSixteenth,
            _ => TripletFeel::None,
        }
    }
}

/// An enumeration of available clefs
#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeasureClef {
    Treble,
    Bass,
    Tenor,
    Alto,
}

/// A line break directive: `NONE: no line break`, `BREAK: break line`, `Protect the line from breaking`.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineBreak {
    None,
    Break,
    Protect,
    /// GuitarScore addition: start a new page here. GP3-5 files keep it as a line break;
    /// GP7 files keep it in a GuitarScore attribute of the track (`gsPageBreaks`).
    Page,
}

/// A navigation sign like *Coda* (𝄌: U+1D10C) or *Segno* (𝄋 or 𝄉: U+1D10B or U+1D109).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DirectionSign {
    Coda,
    DoubleCoda,
    Segno,
    SegnoSegno,
    Fine,
    DaCapo,
    DaCapoAlCoda,
    DaCapoAlDoubleCoda,
    DaCapoAlFine,
    DaSegno,
    DaSegnoAlCoda,
    DaSegnoAlDoubleCoda,
    DaSegnoAlFine,
    DaSegnoSegno,
    DaSegnoSegnoAlCoda,
    DaSegnoSegnoAlDoubleCoda,
    DaSegnoSegnoAlFine,
    DaCoda,
    DaDoubleCoda,
}

/// An enumeration of all supported slide types.
#[repr(i8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlideType {
    IntoFromAbove = -2,
    IntoFromBelow = -1,
    None = 0,
    ShiftSlideTo,
    LegatoSlideTo,
    OutDownwards,
    OutUpWards,
    /// GuitarScore addition: GPIF's pick slides (pick scrapes) down and up the strings
    /// (slide flags 64 and 128). GP3-5 have none; they are not written there.
    PickSlideDown,
    PickSlideUp,
}

/// An enumeration of all supported note types.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteType {
    Rest,
    Normal,
    Tie,
    Dead,
    Unknown(u8),
}

/// Left and right hand fingering used in tabs and chord diagram editor.
#[repr(i8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fingering {
    /// Open or muted.
    Open = -1,
    /// Thumb.
    Thumb = 0,
    /// Index finger.
    Index,
    /// Middle finger.
    Middle,
    /// Annular finger.
    Annular,
    /// Little finger.
    Little,
    /// GuitarScore addition: GPIF's left-hand `Open`, the note on the open string written
    /// with finger `0`. GP3-5 have no such value; it is written there as no fingering.
    OpenString,

    Unknown(i8),
}

/// Values of auto-accentuation on the beat found in track RSE settings
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum Accentuation {
    None,
    VerySoft,
    Soft,
    Medium,
    Strong,
    VeryStrong,
}
