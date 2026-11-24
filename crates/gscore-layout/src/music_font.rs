//! The music fonts the score can be drawn with. Bravura is always bundled; Petaluma (a
//! handwritten jazz look) and Leland (MuseScore's engraved look) come with the
//! `extra-music-fonts` feature, which the desktop builds turn on and the browser build
//! leaves off (its download stays small). All three are SMuFL fonts under the SIL OFL 1.1
//! (see `THIRD_PARTY.md`).

use gscore_model::Fermata;

use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::metrics::MusicFont;

/// A bundled music font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MusicFontChoice {
    /// Steinberg's engraved SMuFL reference font.
    #[default]
    Bravura,
    /// Steinberg's handwritten jazz font.
    Petaluma,
    /// MuseScore's engraved font.
    Leland,
}

static BRAVURA_FONT: &[u8] = include_bytes!("../../../assets/fonts/bravura/Bravura.otf");
#[cfg(feature = "extra-music-fonts")]
static PETALUMA_FONT: &[u8] = include_bytes!("../../../assets/fonts/petaluma/Petaluma.otf");
#[cfg(feature = "extra-music-fonts")]
static LELAND_FONT: &[u8] = include_bytes!("../../../assets/fonts/leland/Leland.otf");
#[cfg(feature = "extra-music-fonts")]
const PETALUMA_METADATA: &str =
    include_str!("../../../assets/fonts/petaluma/petaluma_metadata.json");
#[cfg(feature = "extra-music-fonts")]
const LELAND_METADATA: &str = include_str!("../../../assets/fonts/leland/leland_metadata.json");

impl MusicFontChoice {
    /// Every music font this build bundles, Bravura first.
    pub fn available() -> &'static [MusicFontChoice] {
        if cfg!(feature = "extra-music-fonts") {
            &[
                MusicFontChoice::Bravura,
                MusicFontChoice::Petaluma,
                MusicFontChoice::Leland,
            ]
        } else {
            &[MusicFontChoice::Bravura]
        }
    }

    /// The id the settings keep (`bravura`, `petaluma`, `leland`).
    pub fn id(self) -> &'static str {
        match self {
            MusicFontChoice::Bravura => "bravura",
            MusicFontChoice::Petaluma => "petaluma",
            MusicFontChoice::Leland => "leland",
        }
    }

    /// The font `id` names, if this build bundles it; anything else is Bravura.
    pub fn from_id(id: &str) -> MusicFontChoice {
        MusicFontChoice::available()
            .iter()
            .copied()
            .find(|choice| choice.id() == id)
            .unwrap_or_default()
    }

    /// The font's name.
    pub fn name(self) -> &'static str {
        match self {
            MusicFontChoice::Bravura => "Bravura",
            MusicFontChoice::Petaluma => "Petaluma",
            MusicFontChoice::Leland => "Leland",
        }
    }

    /// The font file (OpenType). A font this build does not bundle gives Bravura's.
    pub fn file(self) -> &'static [u8] {
        match self {
            #[cfg(feature = "extra-music-fonts")]
            MusicFontChoice::Petaluma => PETALUMA_FONT,
            #[cfg(feature = "extra-music-fonts")]
            MusicFontChoice::Leland => LELAND_FONT,
            _ => BRAVURA_FONT,
        }
    }

    /// Glyphs this font does not draw itself: they are taken from Bravura's metrics and
    /// drawn with Bravura. Leland is missing a few marks guitar tablature and scores use
    /// (its text-sized notes, the white diamond of a natural harmonic, the shortest
    /// fermata).
    pub fn fallbacks(self) -> &'static [Glyph] {
        match self {
            MusicFontChoice::Leland => &[
                Glyph::TextNote,
                Glyph::TextBeam8th,
                Glyph::TextBeam16th,
                Glyph::TextEnd8th,
                Glyph::TextEnd16th,
                Glyph::NoteheadHarmonic,
                Glyph::Fermata(Fermata::VeryShort),
            ],
            MusicFontChoice::Bravura | MusicFontChoice::Petaluma => &[],
        }
    }

    /// Whether `glyph` comes from Bravura because this font does not have it.
    pub fn falls_back(self, glyph: Glyph) -> bool {
        self.fallbacks().contains(&glyph)
    }

    /// The font's metrics, for layout. A font this build does not bundle gives Bravura's.
    pub fn metrics(self) -> Result<MusicFont, LayoutError> {
        match self {
            #[cfg(feature = "extra-music-fonts")]
            MusicFontChoice::Petaluma => MusicFont::from_metadata_and_font(
                PETALUMA_METADATA,
                PETALUMA_FONT,
                self.fallbacks(),
            ),
            #[cfg(feature = "extra-music-fonts")]
            MusicFontChoice::Leland => {
                MusicFont::from_metadata_and_font(LELAND_METADATA, LELAND_FONT, self.fallbacks())
            }
            _ => MusicFont::bravura(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::MusicFontChoice;

    /// Each font's fallback list is exactly the glyphs it does not draw itself, so layout
    /// takes Bravura's metrics for real gaps only.
    #[cfg(feature = "extra-music-fonts")]
    #[test]
    fn a_fonts_fallbacks_are_exactly_the_glyphs_it_lacks() {
        for (choice, bytes, metadata) in [
            (
                MusicFontChoice::Petaluma,
                super::PETALUMA_FONT,
                super::PETALUMA_METADATA,
            ),
            (
                MusicFontChoice::Leland,
                super::LELAND_FONT,
                super::LELAND_METADATA,
            ),
        ] {
            let font = crate::pdf::FontInfo::parse(bytes).unwrap();
            let raw: serde_json::Value = serde_json::from_str(metadata).unwrap();
            let boxes = raw["glyphBBoxes"].as_object().unwrap();
            for glyph in crate::Glyph::all() {
                let has =
                    font.glyph(glyph.codepoint()).is_some() && boxes.contains_key(glyph.name());
                assert_eq!(
                    choice.falls_back(glyph),
                    !has,
                    "{}: {}",
                    choice.name(),
                    glyph.name()
                );
            }
        }
    }

    #[test]
    fn every_bundled_music_font_has_every_glyph_layout_uses() {
        for &choice in MusicFontChoice::available() {
            let font = choice
                .metrics()
                .unwrap_or_else(|error| panic!("{}: {error}", choice.name()));
            assert_eq!(font.name(), choice.name());
            assert!(!choice.file().is_empty());
            assert_eq!(MusicFontChoice::from_id(choice.id()), choice);
        }
        assert_eq!(MusicFontChoice::from_id("comic"), MusicFontChoice::Bravura);
    }
}
