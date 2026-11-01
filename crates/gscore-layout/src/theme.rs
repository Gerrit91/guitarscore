//! Light and dark palettes: what color each [`ColorToken`] gets. Defined once here; the SVG
//! renderer and the GUI both read them.

use std::fmt;

use crate::draw::ColorToken;

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl fmt::Display for Rgb {
    /// `#rrggbb`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// A palette for every [`ColorToken`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Name for settings and the command line (`light`, `dark`).
    pub name: &'static str,
    /// Page background.
    pub paper: Rgb,
    /// Staff lines.
    pub staff: Rgb,
    /// Barlines.
    pub barline: Rgb,
    /// Fret numbers.
    pub fret_number: Rgb,
    /// Clefs, time signatures, rests.
    pub notation: Rgb,
    /// Title block.
    pub title: Rgb,
    /// Measure numbers, tuning, tempo.
    pub annotation: Rgb,
}

impl Theme {
    /// Dark ink on white paper.
    pub const LIGHT: Theme = Theme {
        name: "light",
        paper: Rgb(0xff, 0xff, 0xff),
        staff: Rgb(0x70, 0x70, 0x70),
        barline: Rgb(0x30, 0x30, 0x30),
        fret_number: Rgb(0x10, 0x10, 0x10),
        notation: Rgb(0x10, 0x10, 0x10),
        title: Rgb(0x10, 0x10, 0x10),
        annotation: Rgb(0x40, 0x40, 0x40),
    };

    /// Light ink on a dark background.
    pub const DARK: Theme = Theme {
        name: "dark",
        paper: Rgb(0x1e, 0x1f, 0x22),
        staff: Rgb(0x80, 0x84, 0x8a),
        barline: Rgb(0xb0, 0xb4, 0xba),
        fret_number: Rgb(0xec, 0xee, 0xf0),
        notation: Rgb(0xec, 0xee, 0xf0),
        title: Rgb(0xec, 0xee, 0xf0),
        annotation: Rgb(0xb8, 0xbc, 0xc2),
    };

    /// Every built-in theme.
    pub const ALL: [&'static Theme; 2] = [&Theme::LIGHT, &Theme::DARK];

    /// The built-in theme called `name`.
    pub fn by_name(name: &str) -> Option<&'static Theme> {
        Theme::ALL.into_iter().find(|theme| theme.name == name)
    }

    /// The color for `token`.
    pub fn color(&self, token: ColorToken) -> Rgb {
        match token {
            ColorToken::Paper => self.paper,
            ColorToken::Staff => self.staff,
            ColorToken::Barline => self.barline,
            ColorToken::FretNumber => self.fret_number,
            ColorToken::Notation => self.notation,
            ColorToken::Title => self.title,
            ColorToken::Annotation => self.annotation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_are_found_by_name_and_keep_ink_readable() {
        assert_eq!(Theme::by_name("dark"), Some(&Theme::DARK));
        assert_eq!(Theme::by_name("sepia"), None);
        assert_eq!(Rgb(0x1e, 0x1f, 0x22).to_string(), "#1e1f22");
        // Ink must differ clearly from the paper in both themes.
        for theme in Theme::ALL {
            let luma = |c: Rgb| u32::from(c.0) + u32::from(c.1) + u32::from(c.2);
            let paper = luma(theme.paper);
            let ink = luma(theme.fret_number);
            assert!(paper.abs_diff(ink) > 400, "{}", theme.name);
        }
    }
}
