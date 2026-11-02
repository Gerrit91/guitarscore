//! Colors and section markers that the editor shows: a track's color and a measure's
//! section marker. Colors are 24-bit RGB, as Guitar Pro stores them.

/// A color as 24-bit RGB: `0xRRGGBB`. The high byte is always cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackColor(u32);

impl TrackColor {
    /// The color Guitar Pro gives a new track (red).
    pub const DEFAULT: TrackColor = TrackColor(0x00FF_0000);

    /// A color from the low 24 bits of `rgb`; higher bits are ignored.
    pub fn new(rgb: u32) -> Self {
        TrackColor(rgb & 0x00FF_FFFF)
    }

    /// A color from the stored value. Guitar Pro stores it as an `i32`, but only the low
    /// 24 bits are the color.
    pub fn from_gp(value: i32) -> Self {
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
}
