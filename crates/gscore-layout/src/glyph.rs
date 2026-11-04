//! The SMuFL glyphs layout uses, by SMuFL name.
//!
//! Draw operations carry a [`Glyph`]; a renderer turns it into a character of the music font
//! with [`Glyph::codepoint`]. Names and code points are from the SMuFL specification
//! (`glyphnames.json`); a test checks that the bundled font's metadata knows every name.

use gscore_model::{Dynamic, Fermata};

/// A music-font glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// Tab clef for staves with 5 or more lines (`6stringTabClef`).
    TabClef6,
    /// Tab clef for staves with 4 or fewer lines (`4stringTabClef`).
    TabClef4,
    /// Time signature digit 0 to 9 (`timeSig0` ... `timeSig9`).
    TimeSigDigit(Digit),
    /// Whole (semibreve) rest (`restWhole`).
    RestWhole,
    /// Half (minim) rest (`restHalf`).
    RestHalf,
    /// Quarter (crotchet) rest (`restQuarter`).
    RestQuarter,
    /// Eighth rest (`rest8th`).
    Rest8th,
    /// 16th rest (`rest16th`).
    Rest16th,
    /// 32nd rest (`rest32nd`).
    Rest32nd,
    /// 64th rest (`rest64th`).
    Rest64th,
    /// 128th rest (`rest128th`).
    Rest128th,
    /// Augmentation dot (`augmentationDot`).
    AugmentationDot,
    /// Quarter note with stem up, for tempo marks (`metNoteQuarterUp`).
    MetronomeQuarter,
    /// Eighth, 16th and 32nd notes with flags, stem up, for feel marks (`metNote8thUp`,
    /// `metNote16thUp`, `metNote32ndUp`).
    Metronome8th,
    Metronome16th,
    Metronome32nd,
    /// The dot after a metronome note (`metAugmentationDot`).
    MetronomeDot,
    /// Text-sized notes that join into beamed pairs: a note (`textBlackNoteLongStem`), the
    /// beam that continues to the next note (`textCont8thBeamLongStem`,
    /// `textCont16thBeamLongStem`) and the note that ends the beam
    /// (`textBlackNoteFrac8thLongStem`, `textBlackNoteFrac16thLongStem`).
    TextNote,
    TextBeam8th,
    TextBeam16th,
    TextEnd8th,
    TextEnd16th,
    /// The tuplet number 3 (`tuplet3`).
    Tuplet3,
    /// The trill ornament (`ornamentTrill`).
    Trill,
    /// Three tremolo strokes (`tremolo3`).
    Tremolo3,
    /// Treble clef (`gClef`).
    GClef,
    /// Bass clef (`fClef`).
    FClef,
    /// Whole-note head (`noteheadWhole`).
    NoteheadWhole,
    /// Half-note head (`noteheadHalf`).
    NoteheadHalf,
    /// Filled head of quarter and shorter notes (`noteheadBlack`).
    NoteheadBlack,
    /// Cross head of a dead note (`noteheadXBlack`).
    NoteheadX,
    /// Circled cross head: open hi-hat, side stick (`noteheadCircleX`).
    NoteheadCircleX,
    /// Diamond head: ride bell (`noteheadDiamondBlack`).
    NoteheadDiamond,
    /// Triangle head: cowbell, tambourine (`noteheadTriangleUpBlack`).
    NoteheadTriangle,
    /// Percussion clef (`unpitchedPercussionClef1`).
    PercussionClef,
    /// Repeat dot (`repeatDot`).
    RepeatDot,
    /// A dynamic mark, `ppp` to `fff` (`dynamicPPP` ... `dynamicFFF`).
    Dynamic(Dynamic),
    /// A fermata above the staff (`fermataVeryShortAbove` ... `fermataVeryLongAbove`).
    Fermata(Fermata),
    /// Sharp (`accidentalSharp`).
    Sharp,
    /// Flat (`accidentalFlat`).
    Flat,
    /// Natural (`accidentalNatural`).
    Natural,
    /// The flag of an unbeamed eighth or shorter note (`flag8thUp` ... `flag128thDown`).
    Flag(Flag),
    /// Accent above and below a note (`articAccentAbove`, `articAccentBelow`).
    AccentAbove,
    AccentBelow,
    /// Staccato dot above and below a note (`articStaccatoAbove`, `articStaccatoBelow`).
    StaccatoAbove,
    StaccatoBelow,
    /// Heavy accent (marcato) above and below a note (`articMarcatoAbove`,
    /// `articMarcatoBelow`).
    MarcatoAbove,
    MarcatoBelow,
    /// Down and up pick strokes (`stringsDownBow`, `stringsUpBow`).
    DownStroke,
    UpStroke,
    /// Hollow diamond head of a natural harmonic (`noteheadDiamondWhite`).
    NoteheadHarmonic,
    /// Parentheses around a ghost note's head (`noteheadParenthesisLeft`,
    /// `noteheadParenthesisRight`).
    ParenthesisLeft,
    ParenthesisRight,
}

/// A flag: how many beams it stands for (1 for an eighth, up to 5 for a 128th) and
/// whether the stem points up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Flag {
    beams: u8,
    up: bool,
}

impl Flag {
    /// The flag of a note with `beams` beams (1-5), on a stem pointing up or down.
    pub fn new(beams: u8, up: bool) -> Option<Self> {
        (1..=5).contains(&beams).then_some(Flag { beams, up })
    }

    /// Position in SMuFL's flag range: eighth up, eighth down, 16th up, ...
    fn index(self) -> u8 {
        self.beams
            .saturating_sub(1)
            .saturating_mul(2)
            .saturating_add(u8::from(!self.up))
    }
}

/// SMuFL names of the flags, in code point order from `flag8thUp` (U+E240).
const FLAG_NAMES: [&str; 10] = [
    "flag8thUp",
    "flag8thDown",
    "flag16thUp",
    "flag16thDown",
    "flag32ndUp",
    "flag32ndDown",
    "flag64thUp",
    "flag64thDown",
    "flag128thUp",
    "flag128thDown",
];
/// SMuFL code point of `flag8thUp`; the other flags follow it.
const FLAG_FIRST: u32 = 0xE240;

/// A decimal digit, 0 to 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digit(u8);

impl Digit {
    /// The digit `value`, if it is 0 to 9.
    pub fn new(value: u32) -> Option<Self> {
        u8::try_from(value).ok().filter(|&d| d <= 9).map(Digit)
    }

    /// The digit's value.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// SMuFL code point of `timeSig0`; the other digits follow it.
const TIME_SIG_ZERO: u32 = 0xE080;

impl Glyph {
    /// Every glyph, for tests and font checks.
    pub fn all() -> impl Iterator<Item = Glyph> {
        let digits = (0..=9).filter_map(Digit::new).map(Glyph::TimeSigDigit);
        let flags = (1..=5)
            .flat_map(|beams| [true, false].map(|up| Flag::new(beams, up)))
            .flatten()
            .map(Glyph::Flag);
        [
            Glyph::TabClef6,
            Glyph::TabClef4,
            Glyph::RestWhole,
            Glyph::RestHalf,
            Glyph::RestQuarter,
            Glyph::Rest8th,
            Glyph::Rest16th,
            Glyph::Rest32nd,
            Glyph::Rest64th,
            Glyph::Rest128th,
            Glyph::AugmentationDot,
            Glyph::MetronomeQuarter,
            Glyph::Metronome8th,
            Glyph::Metronome16th,
            Glyph::Metronome32nd,
            Glyph::MetronomeDot,
            Glyph::TextNote,
            Glyph::TextBeam8th,
            Glyph::TextBeam16th,
            Glyph::TextEnd8th,
            Glyph::TextEnd16th,
            Glyph::Tuplet3,
            Glyph::Trill,
            Glyph::Tremolo3,
            Glyph::GClef,
            Glyph::FClef,
            Glyph::NoteheadWhole,
            Glyph::NoteheadHalf,
            Glyph::NoteheadBlack,
            Glyph::NoteheadX,
            Glyph::NoteheadCircleX,
            Glyph::NoteheadDiamond,
            Glyph::NoteheadTriangle,
            Glyph::PercussionClef,
            Glyph::RepeatDot,
            Glyph::Sharp,
            Glyph::Flat,
            Glyph::Natural,
            Glyph::AccentAbove,
            Glyph::AccentBelow,
            Glyph::StaccatoAbove,
            Glyph::StaccatoBelow,
            Glyph::MarcatoAbove,
            Glyph::MarcatoBelow,
            Glyph::DownStroke,
            Glyph::UpStroke,
            Glyph::NoteheadHarmonic,
            Glyph::ParenthesisLeft,
            Glyph::ParenthesisRight,
        ]
        .into_iter()
        .chain(digits)
        .chain(flags)
        .chain(Dynamic::ALL.map(Glyph::Dynamic))
        .chain(Fermata::ALL.map(Glyph::Fermata))
    }

    /// The SMuFL glyph name.
    pub fn name(self) -> &'static str {
        match self {
            Glyph::TabClef6 => "6stringTabClef",
            Glyph::TabClef4 => "4stringTabClef",
            Glyph::TimeSigDigit(digit) => match digit.get() {
                0 => "timeSig0",
                1 => "timeSig1",
                2 => "timeSig2",
                3 => "timeSig3",
                4 => "timeSig4",
                5 => "timeSig5",
                6 => "timeSig6",
                7 => "timeSig7",
                8 => "timeSig8",
                _ => "timeSig9",
            },
            Glyph::RestWhole => "restWhole",
            Glyph::RestHalf => "restHalf",
            Glyph::RestQuarter => "restQuarter",
            Glyph::Rest8th => "rest8th",
            Glyph::Rest16th => "rest16th",
            Glyph::Rest32nd => "rest32nd",
            Glyph::Rest64th => "rest64th",
            Glyph::Rest128th => "rest128th",
            Glyph::AugmentationDot => "augmentationDot",
            Glyph::MetronomeQuarter => "metNoteQuarterUp",
            Glyph::Metronome8th => "metNote8thUp",
            Glyph::Metronome16th => "metNote16thUp",
            Glyph::Metronome32nd => "metNote32ndUp",
            Glyph::MetronomeDot => "metAugmentationDot",
            Glyph::TextNote => "textBlackNoteLongStem",
            Glyph::TextBeam8th => "textCont8thBeamLongStem",
            Glyph::TextBeam16th => "textCont16thBeamLongStem",
            Glyph::TextEnd8th => "textBlackNoteFrac8thLongStem",
            Glyph::TextEnd16th => "textBlackNoteFrac16thLongStem",
            Glyph::Tuplet3 => "tuplet3",
            Glyph::Trill => "ornamentTrill",
            Glyph::Tremolo3 => "tremolo3",
            Glyph::GClef => "gClef",
            Glyph::FClef => "fClef",
            Glyph::NoteheadWhole => "noteheadWhole",
            Glyph::NoteheadHalf => "noteheadHalf",
            Glyph::NoteheadBlack => "noteheadBlack",
            Glyph::NoteheadX => "noteheadXBlack",
            Glyph::NoteheadCircleX => "noteheadCircleX",
            Glyph::NoteheadDiamond => "noteheadDiamondBlack",
            Glyph::NoteheadTriangle => "noteheadTriangleUpBlack",
            Glyph::PercussionClef => "unpitchedPercussionClef1",
            Glyph::RepeatDot => "repeatDot",
            Glyph::Dynamic(dynamic) => match dynamic {
                Dynamic::Ppp => "dynamicPPP",
                Dynamic::Pp => "dynamicPP",
                Dynamic::P => "dynamicPiano",
                Dynamic::Mp => "dynamicMP",
                Dynamic::Mf => "dynamicMF",
                Dynamic::F => "dynamicForte",
                Dynamic::Ff => "dynamicFF",
                Dynamic::Fff => "dynamicFFF",
            },
            Glyph::Fermata(fermata) => match fermata {
                Fermata::VeryShort => "fermataVeryShortAbove",
                Fermata::Short => "fermataShortAbove",
                Fermata::Medium => "fermataAbove",
                Fermata::Long => "fermataLongAbove",
                Fermata::VeryLong => "fermataVeryLongAbove",
            },
            Glyph::Sharp => "accidentalSharp",
            Glyph::Flat => "accidentalFlat",
            Glyph::Natural => "accidentalNatural",
            Glyph::Flag(flag) => FLAG_NAMES
                .get(usize::from(flag.index()))
                .copied()
                .unwrap_or("flag8thUp"),
            Glyph::AccentAbove => "articAccentAbove",
            Glyph::AccentBelow => "articAccentBelow",
            Glyph::StaccatoAbove => "articStaccatoAbove",
            Glyph::StaccatoBelow => "articStaccatoBelow",
            Glyph::MarcatoAbove => "articMarcatoAbove",
            Glyph::MarcatoBelow => "articMarcatoBelow",
            Glyph::DownStroke => "stringsDownBow",
            Glyph::UpStroke => "stringsUpBow",
            Glyph::NoteheadHarmonic => "noteheadDiamondWhite",
            Glyph::ParenthesisLeft => "noteheadParenthesisLeft",
            Glyph::ParenthesisRight => "noteheadParenthesisRight",
        }
    }

    /// The glyph's character in a SMuFL font.
    pub fn codepoint(self) -> char {
        let code = match self {
            Glyph::TabClef6 => 0xE06D,
            Glyph::TabClef4 => 0xE06E,
            Glyph::TimeSigDigit(digit) => TIME_SIG_ZERO.saturating_add(u32::from(digit.get())),
            Glyph::RestWhole => 0xE4E3,
            Glyph::RestHalf => 0xE4E4,
            Glyph::RestQuarter => 0xE4E5,
            Glyph::Rest8th => 0xE4E6,
            Glyph::Rest16th => 0xE4E7,
            Glyph::Rest32nd => 0xE4E8,
            Glyph::Rest64th => 0xE4E9,
            Glyph::Rest128th => 0xE4EA,
            Glyph::AugmentationDot => 0xE1E7,
            Glyph::MetronomeQuarter => 0xECA5,
            Glyph::Metronome8th => 0xECA7,
            Glyph::Metronome16th => 0xECA9,
            Glyph::Metronome32nd => 0xECAB,
            Glyph::MetronomeDot => 0xECB7,
            Glyph::TextNote => 0xE1F1,
            Glyph::TextBeam8th => 0xE1F8,
            Glyph::TextBeam16th => 0xE1FA,
            Glyph::TextEnd8th => 0xE1F3,
            Glyph::TextEnd16th => 0xE1F5,
            Glyph::Tuplet3 => 0xE883,
            Glyph::Trill => 0xE566,
            Glyph::Tremolo3 => 0xE222,
            Glyph::GClef => 0xE050,
            Glyph::FClef => 0xE062,
            Glyph::NoteheadWhole => 0xE0A2,
            Glyph::NoteheadHalf => 0xE0A3,
            Glyph::NoteheadBlack => 0xE0A4,
            Glyph::NoteheadX => 0xE0A9,
            Glyph::NoteheadCircleX => 0xE0B3,
            Glyph::NoteheadDiamond => 0xE0DB,
            Glyph::NoteheadTriangle => 0xE0BE,
            Glyph::PercussionClef => 0xE069,
            Glyph::RepeatDot => 0xE044,
            Glyph::Dynamic(dynamic) => match dynamic {
                Dynamic::Ppp => 0xE52A,
                Dynamic::Pp => 0xE52B,
                Dynamic::P => 0xE520,
                Dynamic::Mp => 0xE52C,
                Dynamic::Mf => 0xE52D,
                Dynamic::F => 0xE522,
                Dynamic::Ff => 0xE52F,
                Dynamic::Fff => 0xE530,
            },
            Glyph::Fermata(fermata) => match fermata {
                Fermata::VeryShort => 0xE4C2,
                Fermata::Short => 0xE4C4,
                Fermata::Medium => 0xE4C0,
                Fermata::Long => 0xE4C6,
                Fermata::VeryLong => 0xE4C8,
            },
            Glyph::Sharp => 0xE262,
            Glyph::Flat => 0xE260,
            Glyph::Natural => 0xE261,
            Glyph::Flag(flag) => FLAG_FIRST.saturating_add(u32::from(flag.index())),
            Glyph::AccentAbove => 0xE4A0,
            Glyph::AccentBelow => 0xE4A1,
            Glyph::StaccatoAbove => 0xE4A2,
            Glyph::StaccatoBelow => 0xE4A3,
            Glyph::MarcatoAbove => 0xE4AC,
            Glyph::MarcatoBelow => 0xE4AD,
            Glyph::DownStroke => 0xE610,
            Glyph::UpStroke => 0xE612,
            Glyph::NoteheadHarmonic => 0xE0DD,
            Glyph::ParenthesisLeft => 0xE0F5,
            Glyph::ParenthesisRight => 0xE0F6,
        };
        // All codes above are in the Private Use Area, which is valid `char` range.
        char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
    }

    /// The rest for a note value (`1` whole, `2` half, `4` quarter, ...), if there is one.
    pub fn rest(value: u16) -> Option<Glyph> {
        Some(match value {
            1 => Glyph::RestWhole,
            2 => Glyph::RestHalf,
            4 => Glyph::RestQuarter,
            8 => Glyph::Rest8th,
            16 => Glyph::Rest16th,
            32 => Glyph::Rest32nd,
            64 => Glyph::Rest64th,
            128 => Glyph::Rest128th,
            _ => return None,
        })
    }

    /// The time signature digits of `number`, most significant first.
    pub fn time_sig_digits(number: u32) -> Vec<Glyph> {
        number
            .to_string()
            .chars()
            .filter_map(|c| c.to_digit(10))
            .filter_map(Digit::new)
            .map(Glyph::TimeSigDigit)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_code_points_follow_smufl() {
        assert_eq!(Glyph::TabClef6.codepoint(), '\u{E06D}');
        assert_eq!(Glyph::time_sig_digits(4)[0].name(), "timeSig4");
        assert_eq!(Glyph::time_sig_digits(9)[0].codepoint(), '\u{E089}');
        assert_eq!(
            Glyph::time_sig_digits(12)
                .iter()
                .map(|g| g.name())
                .collect::<Vec<_>>(),
            ["timeSig1", "timeSig2"]
        );
        assert_eq!(Glyph::rest(8), Some(Glyph::Rest8th));
        let flag = |beams, up| Glyph::Flag(Flag::new(beams, up).unwrap());
        assert_eq!(flag(1, true).name(), "flag8thUp");
        assert_eq!(flag(2, false).name(), "flag16thDown");
        assert_eq!(flag(5, false).codepoint(), '\u{E249}');
        assert_eq!(Flag::new(6, true), None);
        assert_eq!(Glyph::rest(3), None);
        let mut names: Vec<_> = Glyph::all().map(Glyph::name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "names are unique");
    }
}
