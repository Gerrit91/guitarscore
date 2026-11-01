//! The SMuFL glyphs layout uses, by SMuFL name.
//!
//! Draw operations carry a [`Glyph`]; a renderer turns it into a character of the music font
//! with [`Glyph::codepoint`]. Names and code points are from the SMuFL specification
//! (`glyphnames.json`); a test checks that the bundled font's metadata knows every name.

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
    /// The trill ornament (`ornamentTrill`).
    Trill,
    /// Three tremolo strokes (`tremolo3`).
    Tremolo3,
}

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
            Glyph::Trill,
            Glyph::Tremolo3,
        ]
        .into_iter()
        .chain(digits)
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
            Glyph::Trill => "ornamentTrill",
            Glyph::Tremolo3 => "tremolo3",
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
            Glyph::Trill => 0xE566,
            Glyph::Tremolo3 => 0xE222,
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
        assert_eq!(Glyph::rest(3), None);
        let mut names: Vec<_> = Glyph::all().map(Glyph::name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "names are unique");
    }
}
