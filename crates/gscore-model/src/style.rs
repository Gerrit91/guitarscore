//! Colors and section markers that the editor shows: a track's color and a measure's
//! section marker. Colors are 24-bit RGB, as Guitar Pro stores them.

/// A color as 24-bit RGB: `0xRRGGBB`. The high byte is always cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackColor(u32);

impl TrackColor {
    /// The color Guitar Pro gives a new track (red).
    pub const DEFAULT: TrackColor = TrackColor(0x00FF_0000);

    /// A color from the low 24 bits of `rgb`; higher bits are ignored.
    pub const fn new(rgb: u32) -> Self {
        TrackColor(rgb & 0x00FF_FFFF)
    }

    /// A color from its red, green and blue parts.
    pub const fn from_parts(red: u8, green: u8, blue: u8) -> Self {
        TrackColor::new(((red as u32) << 16) | ((green as u32) << 8) | blue as u32)
    }

    /// A color from the stored value. Guitar Pro stores it as an `i32`, but only the low
    /// 24 bits are the color.
    pub const fn from_gp(value: i32) -> Self {
        TrackColor::new(value as u32)
    }

    /// The color as `0xRRGGBB`.
    pub fn rgb(self) -> u32 {
        self.0
    }

    /// The red, green and blue parts.
    pub fn parts(self) -> (u8, u8, u8) {
        // Masked to 24 bits, so every part fits.
        let red = u8::try_from((self.0 >> 16) & 0xFF).unwrap_or(0);
        let green = u8::try_from((self.0 >> 8) & 0xFF).unwrap_or(0);
        let blue = u8::try_from(self.0 & 0xFF).unwrap_or(0);
        (red, green, blue)
    }
}

/// A measure's key signature: how many sharps or flats, and whether it is minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySignature {
    /// Sharps (positive) or flats (negative), -7 to 7.
    pub key: i8,
    /// Whether the key is minor.
    pub minor: bool,
}

/// A section marker: a named point in the song (Guitar Pro's "section" marker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// What the section is called.
    pub title: String,
    /// The color the marker is shown in.
    pub color: TrackColor,
}

impl Section {
    /// The color a section gets when the file stores none (GP6/GP7 have no marker color):
    /// a light grey, called "Light grey" in the palette the editor offers for tracks too.
    pub const DEFAULT_COLOR: TrackColor = TrackColor::from_gp(gscore_gp::Marker::DEFAULT_COLOR);
}

/// Which staves a track's page shows: standard notation, tablature and slash notation, each
/// on or off. At least one is always on (Guitar Pro's per-track switches, plus slash
/// notation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackStaves {
    flags: u8,
}

/// The switches of [`TrackStaves`].
const NOTATION: u8 = 0b001;
const TABLATURE: u8 = 0b010;
const SLASH: u8 = 0b100;

impl Default for TrackStaves {
    fn default() -> Self {
        TrackStaves::NOTATION_AND_TAB
    }
}

impl TrackStaves {
    /// Standard notation above the tablature.
    pub const NOTATION_AND_TAB: TrackStaves = TrackStaves {
        flags: NOTATION | TABLATURE,
    };
    /// Only the tablature.
    pub const TAB_ONLY: TrackStaves = TrackStaves { flags: TABLATURE };
    /// Only standard notation.
    pub const NOTATION_ONLY: TrackStaves = TrackStaves { flags: NOTATION };
    /// Tablature and slash notation.
    pub const TAB_AND_SLASH: TrackStaves = TrackStaves {
        flags: TABLATURE | SLASH,
    };
    /// Standard notation and slash notation.
    pub const NOTATION_AND_SLASH: TrackStaves = TrackStaves {
        flags: NOTATION | SLASH,
    };
    /// All three staves.
    pub const ALL_STAVES: TrackStaves = TrackStaves {
        flags: NOTATION | TABLATURE | SLASH,
    };
    /// Only slash notation.
    pub const SLASH_ONLY: TrackStaves = TrackStaves { flags: SLASH };

    /// Every choice, in menu order.
    pub const ALL: [TrackStaves; 7] = [
        TrackStaves::NOTATION_AND_TAB,
        TrackStaves::TAB_ONLY,
        TrackStaves::NOTATION_ONLY,
        TrackStaves::TAB_AND_SLASH,
        TrackStaves::NOTATION_AND_SLASH,
        TrackStaves::ALL_STAVES,
        TrackStaves::SLASH_ONLY,
    ];

    /// The choice for Guitar Pro's switches (and GuitarScore's slash switch); a track
    /// showing none of them shows its tab.
    pub fn from_flags(notation: bool, tablature: bool, slash: bool) -> Self {
        let mut flags = 0;
        if notation {
            flags |= NOTATION;
        }
        if tablature {
            flags |= TABLATURE;
        }
        if slash {
            flags |= SLASH;
        }
        if flags == 0 {
            return TrackStaves::TAB_ONLY;
        }
        TrackStaves { flags }
    }

    /// The switches: `(notation, tablature, slash)`.
    pub fn flags(self) -> (bool, bool, bool) {
        (
            self.flags & NOTATION != 0,
            self.flags & TABLATURE != 0,
            self.flags & SLASH != 0,
        )
    }

    /// Whether standard notation is shown.
    pub fn notation(self) -> bool {
        self.flags & NOTATION != 0
    }

    /// Whether the tablature is shown.
    pub fn tablature(self) -> bool {
        self.flags & TABLATURE != 0
    }

    /// Whether slash notation is shown.
    pub fn slash(self) -> bool {
        self.flags & SLASH != 0
    }

    /// The same choice with standard notation switched `on` or off. Switching off the last
    /// staff leaves the tablature on.
    pub fn with_notation(self, on: bool) -> Self {
        self.with(NOTATION, on)
    }

    /// The same choice with the tablature switched `on` or off.
    pub fn with_tablature(self, on: bool) -> Self {
        self.with(TABLATURE, on)
    }

    /// The same choice with slash notation switched `on` or off.
    pub fn with_slash(self, on: bool) -> Self {
        self.with(SLASH, on)
    }

    fn with(self, flag: u8, on: bool) -> Self {
        let flags = if on {
            self.flags | flag
        } else {
            self.flags & !flag
        };
        if flags == 0 {
            TrackStaves::TAB_ONLY
        } else {
            TrackStaves { flags }
        }
    }

    /// How many staves are shown (1 to 3).
    pub fn count(self) -> usize {
        usize::try_from(self.flags.count_ones()).unwrap_or(1)
    }

    /// The id the command language uses.
    pub fn id(self) -> &'static str {
        match self.flags {
            f if f == NOTATION | TABLATURE => "both",
            f if f == TABLATURE => "tab",
            f if f == NOTATION => "notation",
            f if f == TABLATURE | SLASH => "tab+slash",
            f if f == NOTATION | SLASH => "notation+slash",
            f if f == NOTATION | TABLATURE | SLASH => "both+slash",
            _ => "slash",
        }
    }

    /// The choice named by `id` (see [`TrackStaves::id`]).
    pub fn from_id(id: &str) -> Option<Self> {
        TrackStaves::ALL
            .into_iter()
            .find(|staves| staves.id() == id)
    }

    /// A label for menus.
    pub fn label(self) -> &'static str {
        match self.flags {
            f if f == NOTATION | TABLATURE => "Notes and tabs",
            f if f == TABLATURE => "Tabs only",
            f if f == NOTATION => "Notes only",
            f if f == TABLATURE | SLASH => "Tabs and slash",
            f if f == NOTATION | SLASH => "Notes and slash",
            f if f == NOTATION | TABLATURE | SLASH => "Notes, tabs and slash",
            _ => "Slash only",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keep_their_low_24_bits() {
        let red = TrackColor::new(0xFF_FF0000);
        assert_eq!(red.rgb(), 0xFF0000);
        assert_eq!(red.parts(), (0xFF, 0, 0));
        assert_eq!(TrackColor::from_gp(-1).rgb(), 0xFFFFFF);
        assert_eq!(TrackColor::from_gp(0x123456).parts(), (0x12, 0x34, 0x56));
    }

    #[test]
    fn colors_assemble_from_their_parts() {
        assert_eq!(TrackColor::from_parts(0x12, 0x34, 0x56).rgb(), 0x123456);
        assert_eq!(
            TrackColor::from_parts(0xFF, 0xFF, 0xFF).parts(),
            (0xFF, 0xFF, 0xFF)
        );
    }

    #[test]
    fn staves_map_to_guitar_pros_switches_and_ids() {
        for staves in TrackStaves::ALL {
            let (notation, tablature, slash) = staves.flags();
            assert_eq!(TrackStaves::from_flags(notation, tablature, slash), staves);
            assert_eq!(TrackStaves::from_id(staves.id()), Some(staves));
            assert!(staves.count() >= 1);
        }
        assert_eq!(
            TrackStaves::from_flags(false, false, false),
            TrackStaves::TAB_ONLY
        );
        assert_eq!(TrackStaves::from_id("both "), None);
        // Switching off the last staff keeps the tablature.
        assert_eq!(
            TrackStaves::SLASH_ONLY.with_slash(false),
            TrackStaves::TAB_ONLY
        );
        assert!(TrackStaves::TAB_ONLY.with_slash(true).slash());
    }
}
