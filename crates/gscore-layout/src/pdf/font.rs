//! Just enough of an OpenType font for writing PDF: the character map, the glyph advances,
//! the em size and the font's box. Only the bundled fonts are read here, but anything
//! unexpected is an error, never a panic.

use crate::LayoutError;

/// What the PDF needs to know about one font.
#[derive(Debug)]
pub(crate) struct FontInfo<'a> {
    /// The whole font file.
    pub(crate) data: &'a [u8],
    /// Font units per em.
    pub(crate) units_per_em: u16,
    /// The font's box: x min, y min, x max, y max, in font units.
    pub(crate) bbox: [i16; 4],
    /// Ascender and descender in font units (the descender is negative).
    pub(crate) ascender: i16,
    pub(crate) descender: i16,
    /// Whether the outlines are CFF (else TrueType).
    pub(crate) cff: bool,
    advances: Vec<u16>,
    cmap: Cmap<'a>,
}

/// A character map subtable.
#[derive(Debug)]
enum Cmap<'a> {
    /// Segment mapping to delta values (the Basic Multilingual Plane).
    Format4(&'a [u8]),
    /// Segmented coverage (all of Unicode).
    Format12(&'a [u8]),
}

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    bytes(data, at).map(u16::from_be_bytes)
}

fn i16_at(data: &[u8], at: usize) -> Option<i16> {
    bytes(data, at).map(i16::from_be_bytes)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    bytes(data, at).map(u32::from_be_bytes)
}

/// The table `tag` of the font in `data`.
fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = usize::from(u16_at(data, 4)?);
    (0..count).find_map(|index| {
        let record = index.checked_mul(16)?.checked_add(12)?;
        if data.get(record..record.checked_add(4)?)? != tag {
            return None;
        }
        let offset = usize::try_from(u32_at(data, record.checked_add(8)?)?).ok()?;
        let length = usize::try_from(u32_at(data, record.checked_add(12)?)?).ok()?;
        data.get(offset..offset.checked_add(length)?)
    })
}

fn malformed(what: &str) -> LayoutError {
    LayoutError::Font(format!("cannot read the font's {what}"))
}

impl<'a> FontInfo<'a> {
    /// Reads `data`, an OpenType font with TrueType or CFF outlines.
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self, LayoutError> {
        let head = table(data, b"head").ok_or_else(|| malformed("head table"))?;
        let units_per_em = u16_at(head, 18)
            .filter(|units| *units > 0)
            .ok_or_else(|| malformed("em size"))?;
        let bbox = [
            i16_at(head, 36).ok_or_else(|| malformed("box"))?,
            i16_at(head, 38).ok_or_else(|| malformed("box"))?,
            i16_at(head, 40).ok_or_else(|| malformed("box"))?,
            i16_at(head, 42).ok_or_else(|| malformed("box"))?,
        ];
        let hhea = table(data, b"hhea").ok_or_else(|| malformed("hhea table"))?;
        let ascender = i16_at(hhea, 4).ok_or_else(|| malformed("ascender"))?;
        let descender = i16_at(hhea, 6).ok_or_else(|| malformed("descender"))?;
        let metrics = usize::from(u16_at(hhea, 34).ok_or_else(|| malformed("metrics count"))?);
        let glyphs = table(data, b"maxp")
            .and_then(|maxp| u16_at(maxp, 4))
            .ok_or_else(|| malformed("glyph count"))?;
        let hmtx = table(data, b"hmtx").ok_or_else(|| malformed("hmtx table"))?;
        let mut advances = Vec::with_capacity(usize::from(glyphs));
        let mut last = 0_u16;
        for glyph in 0..usize::from(glyphs) {
            if glyph < metrics {
                last = glyph
                    .checked_mul(4)
                    .and_then(|at| u16_at(hmtx, at))
                    .ok_or_else(|| malformed("advances"))?;
            }
            advances.push(last);
        }
        let cmap = Self::cmap(data).ok_or_else(|| malformed("character map"))?;
        Ok(FontInfo {
            data,
            units_per_em,
            bbox,
            ascender,
            descender,
            cff: table(data, b"CFF ").is_some(),
            advances,
            cmap,
        })
    }

    /// The best Unicode subtable: full Unicode (format 12) before the BMP (format 4).
    fn cmap(data: &'a [u8]) -> Option<Cmap<'a>> {
        let cmap = table(data, b"cmap")?;
        let count = usize::from(u16_at(cmap, 2)?);
        let mut best: Option<Cmap<'a>> = None;
        for index in 0..count {
            let record = index.checked_mul(8)?.checked_add(4)?;
            let platform = u16_at(cmap, record)?;
            let encoding = u16_at(cmap, record.checked_add(2)?)?;
            let unicode = platform == 0 || (platform == 3 && matches!(encoding, 1 | 10));
            if !unicode {
                continue;
            }
            let offset = usize::try_from(u32_at(cmap, record.checked_add(4)?)?).ok()?;
            let sub = cmap.get(offset..)?;
            match u16_at(sub, 0)? {
                12 => return Some(Cmap::Format12(sub)),
                4 => best = Some(Cmap::Format4(sub)),
                _ => {}
            }
        }
        best
    }

    /// The glyph for `c`, if the font has one.
    pub(crate) fn glyph(&self, c: char) -> Option<u16> {
        let code = u32::from(c);
        let glyph = match self.cmap {
            Cmap::Format12(sub) => {
                let groups = usize::try_from(u32_at(sub, 12)?).ok()?;
                (0..groups).find_map(|index| {
                    let at = index.checked_mul(12)?.checked_add(16)?;
                    let start = u32_at(sub, at)?;
                    let end = u32_at(sub, at.checked_add(4)?)?;
                    let first = u32_at(sub, at.checked_add(8)?)?;
                    (start..=end)
                        .contains(&code)
                        .then(|| first.checked_add(code.checked_sub(start)?))
                        .flatten()
                })?
            }
            Cmap::Format4(sub) => u32::from(format4(sub, u16::try_from(code).ok()?)?),
        };
        u16::try_from(glyph).ok().filter(|glyph| *glyph != 0)
    }

    /// The advance width of `glyph` in font units.
    pub(crate) fn advance(&self, glyph: u16) -> u16 {
        self.advances.get(usize::from(glyph)).copied().unwrap_or(0)
    }

    /// `value` font units in thousandths of an em, as PDF font dictionaries want them.
    pub(crate) fn per_mille(&self, value: f32) -> f32 {
        value * 1000.0 / f32::from(self.units_per_em)
    }
}

/// Looks `code` up in a format 4 subtable.
fn format4(sub: &[u8], code: u16) -> Option<u16> {
    let segments = usize::from(u16_at(sub, 6)? / 2);
    let ends = 14_usize;
    let starts = ends.checked_add(segments.checked_mul(2)?)?.checked_add(2)?;
    let deltas = starts.checked_add(segments.checked_mul(2)?)?;
    let ranges = deltas.checked_add(segments.checked_mul(2)?)?;
    for segment in 0..segments {
        let offset = segment.checked_mul(2)?;
        let end = u16_at(sub, ends.checked_add(offset)?)?;
        if end < code {
            continue;
        }
        let start = u16_at(sub, starts.checked_add(offset)?)?;
        if start > code {
            return None;
        }
        let delta = u16_at(sub, deltas.checked_add(offset)?)?;
        let range_at = ranges.checked_add(offset)?;
        let range = usize::from(u16_at(sub, range_at)?);
        if range == 0 {
            return Some(code.wrapping_add(delta));
        }
        let at = range_at
            .checked_add(range)?
            .checked_add(usize::from(code.checked_sub(start)?).checked_mul(2)?)?;
        let glyph = u16_at(sub, at)?;
        return (glyph != 0).then(|| glyph.wrapping_add(delta));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::PdfFonts;

    #[test]
    fn the_bundled_fonts_map_their_characters() {
        let fonts = PdfFonts::bundled();
        let text = FontInfo::parse(fonts.text).unwrap();
        assert!(!text.cff, "Noto Sans has TrueType outlines");
        assert_eq!(text.units_per_em, 1000);
        let a = text.glyph('A').unwrap();
        assert!(text.advance(a) > 400 && text.advance(a) < 800);
        assert!(text.glyph('½').is_some());
        assert_eq!(text.glyph('\u{E050}'), None, "no music in the text font");

        let music = FontInfo::parse(fonts.music).unwrap();
        assert!(music.cff, "Bravura has CFF outlines");
        // The G clef, in SMuFL's private use area.
        let clef = music.glyph('\u{E050}').unwrap();
        assert!(music.advance(clef) > 0);
        FontInfo::parse(fonts.bold).unwrap();
    }

    #[test]
    fn every_glyph_code_point_is_the_named_glyph() {
        // The font's own advance at each code point matches the metadata's advance for the
        // glyph's SMuFL name, so a wrong code point shows up here.
        let fonts = PdfFonts::bundled();
        let music = FontInfo::parse(fonts.music).unwrap();
        let metrics = crate::MusicFont::bravura().unwrap();
        for glyph in crate::Glyph::all() {
            let id = music
                .glyph(glyph.codepoint())
                .unwrap_or_else(|| panic!("{} is not in the font", glyph.name()));
            // Advances in em; a staff space is a quarter em.
            let font_advance = music.per_mille(f32::from(music.advance(id))) / 250.0;
            assert!(
                (font_advance - metrics.advance(glyph)).abs() < 0.01,
                "{}: font {font_advance}, metadata {}",
                glyph.name(),
                metrics.advance(glyph)
            );
        }
    }

    #[test]
    fn broken_fonts_are_errors() {
        let fonts = PdfFonts::bundled();
        for cut in [0, 4, 12, 100, 1000] {
            assert!(FontInfo::parse(&fonts.text[..cut]).is_err(), "cut at {cut}");
        }
        assert!(FontInfo::parse(&[0xff; 64]).is_err());
    }
}
