//! Font metrics: the SMuFL music font's metadata, and text widths of the text font.
//!
//! Layout takes line thicknesses, glyph boxes and advances from the music font's metadata
//! JSON instead of magic numbers. All values are in staff spaces.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::LayoutError;
use crate::glyph::Glyph;

/// The metadata of the bundled music font, Bravura (SIL OFL 1.1, see `THIRD_PARTY.md`).
const BRAVURA_METADATA: &str = include_str!("../../../assets/fonts/bravura/bravura_metadata.json");

/// Largest metadata file accepted (Bravura's is about 1.3 MB).
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

/// Line thicknesses and distances recommended by the music font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngravingDefaults {
    /// Thickness of staff lines.
    pub staff_line_thickness: f32,
    /// Thickness of a normal barline.
    pub thin_barline_thickness: f32,
    /// Thickness of the thick line of a final barline.
    pub thick_barline_thickness: f32,
    /// Gap between the thin and thick line of a final barline.
    pub thin_thick_barline_separation: f32,
    /// Thickness of note stems.
    pub stem_thickness: f32,
    /// Thickness of beams.
    pub beam_thickness: f32,
    /// Gap between two beams.
    pub beam_spacing: f32,
    /// Thickness of ledger lines.
    pub leger_line_thickness: f32,
    /// How far a ledger line extends beyond its notehead on each side.
    pub leger_line_extension: f32,
}

/// A glyph's bounding box relative to its origin, with y **up** as in SMuFL.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlyphBox {
    /// Left edge.
    pub x_min: f32,
    /// Bottom edge.
    pub y_min: f32,
    /// Right edge.
    pub x_max: f32,
    /// Top edge.
    pub y_max: f32,
}

impl GlyphBox {
    /// How far the box's vertical middle is above the origin.
    pub fn middle_above_origin(&self) -> f32 {
        (self.y_min + self.y_max) / 2.0
    }
}

/// Metrics of a SMuFL music font, for the glyphs in [`Glyph::all`].
#[derive(Debug, Clone)]
pub struct MusicFont {
    name: String,
    engraving: EngravingDefaults,
    boxes: HashMap<Glyph, GlyphBox>,
    advances: HashMap<Glyph, f32>,
    /// Named anchor points (`stemUpSE`, ...) of the glyphs that have them.
    anchors: HashMap<(Glyph, String), (f32, f32)>,
}

impl MusicFont {
    /// Bravura, from the metadata bundled into the binary.
    pub fn bravura() -> Result<Self, LayoutError> {
        Self::from_metadata_json(BRAVURA_METADATA)
    }

    /// Reads a SMuFL font metadata file (`<font>_metadata.json`). Fails if it is not valid
    /// metadata or lacks a glyph or value layout needs.
    pub fn from_metadata_json(json: &str) -> Result<Self, LayoutError> {
        if json.len() > MAX_METADATA_BYTES {
            return Err(LayoutError::Font(format!(
                "font metadata is {} bytes; at most {MAX_METADATA_BYTES} are accepted",
                json.len()
            )));
        }
        let raw: RawMetadata = serde_json::from_str(json)
            .map_err(|e| LayoutError::Font(format!("font metadata: {e}")))?;
        let engraving = raw.engraving_defaults.checked()?;

        let mut boxes = HashMap::new();
        let mut advances = HashMap::new();
        for glyph in Glyph::all() {
            let name = glyph.name();
            let missing =
                |what: &str| LayoutError::Font(format!("{}: no {what} for {name}", raw.font_name));
            let raw_box = raw.glyph_b_boxes.get(name).ok_or_else(|| missing("box"))?;
            let [x_min, y_min] = raw_box.b_box_sw;
            let [x_max, y_max] = raw_box.b_box_ne;
            let glyph_box = GlyphBox {
                x_min,
                y_min,
                x_max,
                y_max,
            };
            if ![x_min, y_min, x_max, y_max].iter().all(|v| v.is_finite()) {
                return Err(missing("finite box"));
            }
            boxes.insert(glyph, glyph_box);
            let advance = raw
                .glyph_advance_widths
                .get(name)
                .copied()
                .filter(|a| a.is_finite() && *a >= 0.0)
                .ok_or_else(|| missing("advance width"))?;
            advances.insert(glyph, advance);
        }
        let mut anchors = HashMap::new();
        for glyph in Glyph::all() {
            if let Some(points) = raw.glyphs_with_anchors.get(glyph.name()) {
                for (anchor, &[x, y]) in points {
                    if x.is_finite() && y.is_finite() {
                        anchors.insert((glyph, anchor.clone()), (x, y));
                    }
                }
            }
        }
        Ok(MusicFont {
            name: raw.font_name,
            engraving,
            boxes,
            advances,
            anchors,
        })
    }

    /// The font's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The font's engraving defaults.
    pub fn engraving(&self) -> &EngravingDefaults {
        &self.engraving
    }

    /// The glyph's bounding box. Every glyph has one (checked when the font is loaded).
    pub fn bbox(&self, glyph: Glyph) -> GlyphBox {
        self.boxes.get(&glyph).copied().unwrap_or_default()
    }

    /// A named anchor of the glyph (for example `stemUpSE`, where an up-stem attaches to a
    /// notehead), relative to its origin with y up, if the font defines it.
    pub fn anchor(&self, glyph: Glyph, name: &str) -> Option<(f32, f32)> {
        self.anchors.get(&(glyph, name.to_owned())).copied()
    }

    /// The glyph's advance width. Every glyph has one (checked when the font is loaded).
    pub fn advance(&self, glyph: Glyph) -> f32 {
        self.advances.get(&glyph).copied().unwrap_or_default()
    }
}

/// The part of a SMuFL metadata file that layout reads.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetadata {
    font_name: String,
    engraving_defaults: RawEngravingDefaults,
    #[serde(rename = "glyphBBoxes")]
    glyph_b_boxes: HashMap<String, RawBox>,
    glyph_advance_widths: HashMap<String, f32>,
    #[serde(default)]
    glyphs_with_anchors: HashMap<String, HashMap<String, [f32; 2]>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEngravingDefaults {
    staff_line_thickness: f32,
    thin_barline_thickness: f32,
    thick_barline_thickness: f32,
    thin_thick_barline_separation: f32,
    stem_thickness: f32,
    beam_thickness: f32,
    beam_spacing: f32,
    leger_line_thickness: f32,
    leger_line_extension: f32,
}

impl RawEngravingDefaults {
    /// The values, if all are usable (finite, positive and below one staff height).
    fn checked(self) -> Result<EngravingDefaults, LayoutError> {
        let values = [
            ("staffLineThickness", self.staff_line_thickness),
            ("thinBarlineThickness", self.thin_barline_thickness),
            ("thickBarlineThickness", self.thick_barline_thickness),
            (
                "thinThickBarlineSeparation",
                self.thin_thick_barline_separation,
            ),
            ("stemThickness", self.stem_thickness),
            ("beamThickness", self.beam_thickness),
            ("beamSpacing", self.beam_spacing),
            ("legerLineThickness", self.leger_line_thickness),
            ("legerLineExtension", self.leger_line_extension),
        ];
        if let Some((name, value)) = values
            .iter()
            .find(|(_, v)| !v.is_finite() || *v <= 0.0 || *v > 4.0)
        {
            return Err(LayoutError::Font(format!(
                "engraving default {name} = {value} is out of range"
            )));
        }
        Ok(EngravingDefaults {
            staff_line_thickness: self.staff_line_thickness,
            thin_barline_thickness: self.thin_barline_thickness,
            thick_barline_thickness: self.thick_barline_thickness,
            thin_thick_barline_separation: self.thin_thick_barline_separation,
            stem_thickness: self.stem_thickness,
            beam_thickness: self.beam_thickness,
            beam_spacing: self.beam_spacing,
            leger_line_thickness: self.leger_line_thickness,
            leger_line_extension: self.leger_line_extension,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBox {
    #[serde(rename = "bBoxNE")]
    b_box_ne: [f32; 2],
    #[serde(rename = "bBoxSW")]
    b_box_sw: [f32; 2],
}

/// Text widths of the text font, so layout can size columns around fret numbers.
///
/// The values are Noto Sans's advance widths (as a fraction of the font size) for the
/// characters fret numbers use; other characters get an average width. Exact widths for
/// arbitrary text need a text shaper and come with the GUI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    /// Height of capital letters and digits.
    pub cap_height: f32,
    /// Advance of a digit (Noto Sans digits all have the same width).
    pub digit_advance: f32,
    /// Advance of a parenthesis.
    pub paren_advance: f32,
    /// Advance of a lowercase `x`.
    pub x_advance: f32,
    /// Advance assumed for every other character.
    pub average_advance: f32,
}

impl TextMetrics {
    /// Noto Sans Regular (units per em 1000; read from the bundled font file).
    pub const NOTO_SANS: TextMetrics = TextMetrics {
        cap_height: 0.714,
        digit_advance: 0.572,
        paren_advance: 0.300,
        x_advance: 0.529,
        average_advance: 0.58,
    };

    /// The width of `text` at font size `size`.
    pub fn width(&self, text: &str, size: f32) -> f32 {
        let em: f32 = text
            .chars()
            .map(|c| match c {
                '0'..='9' => self.digit_advance,
                '(' | ')' => self.paren_advance,
                'x' => self.x_advance,
                _ => self.average_advance,
            })
            .sum();
        em * size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bravura_metadata_has_every_glyph_layout_uses() {
        let font = MusicFont::bravura().unwrap();
        assert_eq!(font.name(), "Bravura");
        assert!((font.engraving().staff_line_thickness - 0.13).abs() < 1e-6);
        let clef = font.bbox(Glyph::TabClef6);
        assert!(clef.y_max > 2.0 && clef.y_min < -2.0, "{clef:?}");
        assert!(font.advance(Glyph::RestQuarter) > 0.5);
        assert_eq!(
            font.anchor(Glyph::NoteheadBlack, "stemUpSE"),
            Some((1.18, 0.168))
        );
        assert_eq!(font.anchor(Glyph::NoteheadBlack, "noSuchAnchor"), None);
    }

    #[test]
    fn bad_metadata_is_an_error() {
        for json in [
            "",
            "{}",
            "[1, 2]",
            r#"{"fontName": "X", "engravingDefaults": {}, "glyphBBoxes": {}, "glyphAdvanceWidths": {}}"#,
        ] {
            assert!(MusicFont::from_metadata_json(json).is_err(), "{json}");
        }
        let without_glyphs = r#"{"fontName": "X", "engravingDefaults": {
            "staffLineThickness": 0.13, "thinBarlineThickness": 0.16,
            "thickBarlineThickness": 0.5, "thinThickBarlineSeparation": 0.4,
            "stemThickness": 0.12, "beamThickness": 0.5, "beamSpacing": 0.25,
            "legerLineThickness": 0.16, "legerLineExtension": 0.4},
            "glyphBBoxes": {}, "glyphAdvanceWidths": {}}"#;
        let error = MusicFont::from_metadata_json(without_glyphs).unwrap_err();
        assert!(error.to_string().contains("no box"), "{error}");
        let nan = without_glyphs.replace("0.13", "1e999");
        assert!(MusicFont::from_metadata_json(&nan).is_err());
    }

    #[test]
    fn digits_are_tabular() {
        let text = TextMetrics::NOTO_SANS;
        assert_eq!(text.width("12", 1.0), text.width("88", 1.0));
        assert!(text.width("(12)", 1.0) > text.width("12", 1.0));
    }
}
