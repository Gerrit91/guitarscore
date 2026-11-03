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

/// Which staves a track's page shows (Guitar Pro's per-track notation and tab switches).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TrackStaves {
    /// Standard notation above the tablature.
    #[default]
    NotationAndTab,
    /// Only the tablature.
    TabOnly,
    /// Only standard notation.
    NotationOnly,
}

impl TrackStaves {
    /// Every choice, in menu order.
    pub const ALL: [TrackStaves; 3] = [
        TrackStaves::NotationAndTab,
        TrackStaves::TabOnly,
        TrackStaves::NotationOnly,
    ];

    /// The choice for Guitar Pro's two switches; a track showing neither shows its tab.
    pub fn from_flags(notation: bool, tablature: bool) -> Self {
        match (notation, tablature) {
            (true, true) => TrackStaves::NotationAndTab,
            (true, false) => TrackStaves::NotationOnly,
            (false, _) => TrackStaves::TabOnly,
        }
    }

    /// Guitar Pro's switches: `(notation, tablature)`.
    pub fn flags(self) -> (bool, bool) {
        match self {
            TrackStaves::NotationAndTab => (true, true),
            TrackStaves::TabOnly => (false, true),
            TrackStaves::NotationOnly => (true, false),
        }
    }

    /// Whether standard notation is shown.
    pub fn notation(self) -> bool {
        self.flags().0
    }

    /// Whether the tablature is shown.
    pub fn tablature(self) -> bool {
        self.flags().1
    }

    /// The id the command language uses.
    pub fn id(self) -> &'static str {
        match self {
            TrackStaves::NotationAndTab => "both",
            TrackStaves::TabOnly => "tab",
            TrackStaves::NotationOnly => "notation",
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
        match self {
            TrackStaves::NotationAndTab => "Notes and tabs",
            TrackStaves::TabOnly => "Tabs only",
            TrackStaves::NotationOnly => "Notes only",
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
            let (notation, tablature) = staves.flags();
            assert_eq!(TrackStaves::from_flags(notation, tablature), staves);
            assert_eq!(TrackStaves::from_id(staves.id()), Some(staves));
        }
        assert_eq!(TrackStaves::from_flags(false, false), TrackStaves::TabOnly);
        assert_eq!(TrackStaves::from_id("both "), None);
    }
}
