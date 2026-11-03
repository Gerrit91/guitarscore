//! Renders a [`Layout`] on pages to PDF: one PDF page per layout page, vector drawing, and
//! the fonts embedded as subsets (Bravura and Noto Sans are SIL OFL, which allows it). Text
//! carries a Unicode map, so it can be searched and copied.
//!
//! The draw operations are the same the screen and SVG get; each lands on the page its
//! anchor lies on. Coordinates turn from staff spaces (y down, origin at the top of the
//! stack of pages) into points (y up, origin at each page's bottom-left corner).

mod font;

use std::collections::BTreeMap;

use pdf_writer::types::{CidFontType, FontFlags, LineCapStyle, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};
use subsetter::GlyphRemapper;

use crate::draw::{DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::theme::{Rgb, Theme};
use crate::{Layout, LayoutError};
use font::FontInfo;

/// Points per millimetre.
const POINTS_PER_MM: f32 = 72.0 / 25.4;
/// More pages than this is not a score any PDF viewer should be handed.
const MAX_PAGES: usize = 10_000;

/// The font files a PDF embeds: music glyphs, regular and bold text.
#[derive(Debug, Clone, Copy)]
pub struct PdfFonts<'a> {
    /// The SMuFL music font (OpenType).
    pub music: &'a [u8],
    /// The text font.
    pub text: &'a [u8],
    /// The bold text font.
    pub bold: &'a [u8],
}

impl PdfFonts<'static> {
    /// The bundled fonts: Bravura, Noto Sans and Noto Sans Bold.
    pub fn bundled() -> Self {
        PdfFonts {
            music: include_bytes!("../../../../assets/fonts/bravura/Bravura.otf"),
            text: include_bytes!("../../../../assets/fonts/noto-sans/NotoSans-Regular.ttf"),
            bold: include_bytes!("../../../../assets/fonts/noto-sans/NotoSans-Bold.ttf"),
        }
    }
}

/// One embedded font: its file, the glyphs used (renumbered for the subset) and the
/// characters they show, for the Unicode map.
struct UsedFont<'a> {
    info: FontInfo<'a>,
    /// The PDF resource name (`/M`, `/T`, `/B`) and the PostScript name.
    resource: &'static [u8],
    base_name: &'static [u8],
    remapper: GlyphRemapper,
    /// The original glyph of every subset glyph, by subset glyph id.
    originals: Vec<u16>,
    /// Which character each subset glyph shows.
    unicode: BTreeMap<u16, char>,
}

impl<'a> UsedFont<'a> {
    fn new(
        data: &'a [u8],
        resource: &'static [u8],
        base_name: &'static [u8],
    ) -> Result<Self, LayoutError> {
        Ok(UsedFont {
            info: FontInfo::parse(data)?,
            resource,
            base_name,
            remapper: GlyphRemapper::new(),
            // The subset always keeps .notdef as glyph 0.
            originals: vec![0],
            unicode: BTreeMap::new(),
        })
    }

    /// `text` as two-byte subset glyph ids (the font's Identity-H encoding), and its width
    /// in ems.
    fn encode(&mut self, text: &str) -> (Vec<u8>, f32) {
        let mut out = Vec::with_capacity(text.len().saturating_mul(2));
        let mut width = 0.0_f32;
        for c in text.chars() {
            let original = self.info.glyph(c).unwrap_or(0);
            let glyph = self.remapper.remap(original);
            if usize::from(glyph) == self.originals.len() {
                self.originals.push(original);
            }
            if original != 0 {
                self.unicode.entry(glyph).or_insert(c);
            }
            out.extend_from_slice(&glyph.to_be_bytes());
            width += self.info.per_mille(f32::from(self.info.advance(original))) / 1000.0;
        }
        (out, width)
    }

    /// Writes the font's objects: the Type 0 font at `ids.font`, its CID font, descriptor,
    /// subset file and Unicode map.
    fn write(&self, pdf: &mut Pdf, ids: FontIds) -> Result<(), LayoutError> {
        let subset = subsetter::subset(self.info.data, 0, &self.remapper)
            .map_err(|error| LayoutError::Font(format!("cannot subset a font: {error:?}")))?;
        let info = &self.info;
        let mut name = b"GSCORE+".to_vec();
        name.extend_from_slice(self.base_name);

        pdf.type0_font(ids.font)
            .base_font(Name(&name))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(ids.cid_font)
            .to_unicode(ids.unicode);

        let system = SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        };
        let mut cid = pdf.cid_font(ids.cid_font);
        cid.subtype(if info.cff {
            CidFontType::Type0
        } else {
            CidFontType::Type2
        })
        .base_font(Name(&name))
        .system_info(system)
        .font_descriptor(ids.descriptor)
        .default_width(0.0);
        if !info.cff {
            cid.cid_to_gid_map_predefined(Name(b"Identity"));
        }
        cid.widths().consecutive(
            0,
            self.originals
                .iter()
                .map(|glyph| info.per_mille(f32::from(info.advance(*glyph)))),
        );
        drop(cid);

        let [x_min, y_min, x_max, y_max] = info.bbox.map(|value| info.per_mille(f32::from(value)));
        let mut descriptor = pdf.font_descriptor(ids.descriptor);
        descriptor
            .name(Name(&name))
            .flags(if self.resource == b"M" {
                FontFlags::SYMBOLIC
            } else {
                FontFlags::NON_SYMBOLIC
            })
            .bbox(PdfRect::new(x_min, y_min, x_max, y_max))
            .italic_angle(0.0)
            .ascent(info.per_mille(f32::from(info.ascender)))
            .descent(info.per_mille(f32::from(info.descender)))
            .cap_height(info.per_mille(f32::from(info.ascender)) * 0.7)
            .stem_v(80.0);
        if info.cff {
            descriptor.font_file3(ids.file);
        } else {
            descriptor.font_file2(ids.file);
        }
        drop(descriptor);

        let mut file = pdf.stream(ids.file, &subset);
        if info.cff {
            file.pair(Name(b"Subtype"), Name(b"OpenType"));
        }
        drop(file);

        let mut cmap = UnicodeCmap::new(Name(b"Custom"), system);
        for (glyph, c) in &self.unicode {
            cmap.pair(*glyph, *c);
        }
        pdf.cmap(ids.unicode, &cmap.finish());
        Ok(())
    }
}

/// The object ids of one font.
#[derive(Debug, Clone, Copy)]
struct FontIds {
    font: Ref,
    cid_font: Ref,
    descriptor: Ref,
    file: Ref,
    unicode: Ref,
}

/// Hands out object ids in order.
struct Ids(i32);

impl Ids {
    fn next(&mut self) -> Ref {
        self.0 = self.0.saturating_add(1);
        Ref::new(self.0)
    }

    fn font(&mut self) -> FontIds {
        FontIds {
            font: self.next(),
            cid_font: self.next(),
            descriptor: self.next(),
            file: self.next(),
            unicode: self.next(),
        }
    }
}

/// The page an operation belongs to: the one its anchor lies on (or the nearest).
fn page_of(pages: &[Rect], op: &DrawOp) -> usize {
    let y = match op {
        DrawOp::Line { from, .. } | DrawOp::Curve { from, .. } => from.y,
        DrawOp::Rect { rect, .. } => rect.origin.y,
        DrawOp::Polygon { points, .. } => points.first().map_or(0.0, |point| point.y),
        DrawOp::Glyph { at, .. } | DrawOp::Text { at, .. } => at.y,
    };
    pages
        .iter()
        .rposition(|page| page.origin.y <= y)
        .unwrap_or(0)
}

fn fill(content: &mut Content, rgb: Rgb) {
    let Rgb(r, g, b) = rgb;
    content.set_fill_rgb(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
}

fn stroke(content: &mut Content, rgb: Rgb) {
    let Rgb(r, g, b) = rgb;
    content.set_stroke_rgb(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
}

/// The layout's pages as a PDF document titled `title`. `staff_space_mm` sets the physical
/// size, like for SVG; `theme` the colors (the light one prints best).
pub fn to_pdf(
    layout: &Layout,
    theme: &Theme,
    staff_space_mm: f32,
    fonts: &PdfFonts<'_>,
    title: &str,
) -> Result<Vec<u8>, LayoutError> {
    let scale = staff_space_mm * POINTS_PER_MM;
    if !(scale.is_finite() && scale > 0.0) {
        return Err(LayoutError::Unsupported(format!(
            "a staff space of {staff_space_mm} mm"
        )));
    }
    if layout.pages.is_empty() || layout.pages.len() > MAX_PAGES {
        return Err(LayoutError::Unsupported(format!(
            "{} pages",
            layout.pages.len()
        )));
    }
    let mut music = UsedFont::new(fonts.music, b"M", b"Bravura")?;
    let mut text = UsedFont::new(fonts.text, b"T", b"NotoSans-Regular")?;
    let mut bold = UsedFont::new(fonts.bold, b"B", b"NotoSans-Bold")?;

    // One content stream per page, the paper first.
    let mut contents: Vec<Content> = layout
        .pages
        .iter()
        .map(|page| {
            let mut content = Content::new();
            fill(&mut content, theme.paper);
            content.rect(0.0, 0.0, page.width * scale, page.height * scale);
            content.fill_nonzero();
            content
        })
        .collect();
    for op in &layout.ops {
        let index = page_of(&layout.pages, op);
        let (Some(page), Some(content)) = (layout.pages.get(index), contents.get_mut(index)) else {
            continue;
        };
        let at = |point: Point| {
            (
                (point.x - page.origin.x) * scale,
                (page.origin.y + page.height - point.y) * scale,
            )
        };
        match op {
            DrawOp::Line {
                from,
                to,
                thickness,
                color,
            } => {
                stroke(content, theme.color(*color));
                content.set_line_width(thickness * scale);
                content.set_line_cap(LineCapStyle::ButtCap);
                let (x1, y1) = at(*from);
                let (x2, y2) = at(*to);
                content.move_to(x1, y1).line_to(x2, y2).stroke();
            }
            DrawOp::Rect { rect, color } => {
                fill(content, theme.color(*color));
                let (x, y) = at(Point::new(rect.origin.x, rect.origin.y + rect.height));
                content
                    .rect(x, y, rect.width * scale, rect.height * scale)
                    .fill_nonzero();
            }
            DrawOp::Polygon { points, color } => {
                let mut corners = points.iter().map(|point| at(*point));
                if let Some((x, y)) = corners.next() {
                    fill(content, theme.color(*color));
                    content.move_to(x, y);
                    for (x, y) in corners {
                        content.line_to(x, y);
                    }
                    content.close_path().fill_nonzero();
                }
            }
            DrawOp::Curve {
                from,
                to,
                control1,
                control2,
                thickness,
                color,
            } => {
                stroke(content, theme.color(*color));
                content.set_line_width(thickness * scale);
                content.set_line_cap(LineCapStyle::RoundCap);
                let (x0, y0) = at(*from);
                let (x1, y1) = at(*control1);
                let (x2, y2) = at(*control2);
                let (x3, y3) = at(*to);
                content
                    .move_to(x0, y0)
                    .cubic_to(x1, y1, x2, y2, x3, y3)
                    .stroke();
            }
            DrawOp::Glyph {
                glyph,
                at: anchor,
                scale: glyph_scale,
                color,
            } => {
                let (bytes, _) = music.encode(&glyph.codepoint().to_string());
                // One em of a SMuFL font is four staff spaces.
                let size = 4.0 * glyph_scale * scale;
                let (x, y) = at(*anchor);
                show(
                    content,
                    music.resource,
                    size,
                    x,
                    y,
                    theme.color(*color),
                    &bytes,
                );
            }
            DrawOp::Text {
                text: words,
                at: anchor,
                size,
                font,
                anchor: alignment,
                color,
            } => {
                let used = match font {
                    TextFont::Regular => &mut text,
                    TextFont::Bold => &mut bold,
                };
                let (bytes, ems) = used.encode(words);
                let size = size * scale;
                let (x, y) = at(*anchor);
                let x = match alignment {
                    TextAnchor::Start => x,
                    TextAnchor::Middle => x - ems * size / 2.0,
                    TextAnchor::End => x - ems * size,
                };
                show(
                    content,
                    used.resource,
                    size,
                    x,
                    y,
                    theme.color(*color),
                    &bytes,
                );
            }
        }
    }

    let mut pdf = Pdf::new();
    let mut ids = Ids(0);
    let catalog = ids.next();
    let tree = ids.next();
    let info = ids.next();
    let page_ids: Vec<(Ref, Ref)> = layout
        .pages
        .iter()
        .map(|_| (ids.next(), ids.next()))
        .collect();
    let used: Vec<(&UsedFont<'_>, FontIds)> = [&music, &text, &bold]
        .into_iter()
        .filter(|font| font.originals.len() > 1)
        .map(|font| (font, ids.font()))
        .collect();

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().map(|(page, _)| *page))
        .count(i32::try_from(page_ids.len()).unwrap_or(i32::MAX));
    let mut document = pdf.document_info(info);
    if !title.trim().is_empty() {
        document.title(TextStr(title));
    }
    document.producer(TextStr("GuitarScore"));
    drop(document);
    for ((page_id, content_id), (page, content)) in
        page_ids.iter().zip(layout.pages.iter().zip(contents))
    {
        let mut writer = pdf.page(*page_id);
        writer
            .media_box(PdfRect::new(
                0.0,
                0.0,
                page.width * scale,
                page.height * scale,
            ))
            .parent(tree)
            .contents(*content_id);
        let mut resources = writer.resources();
        let mut fonts = resources.fonts();
        for (font, font_ids) in &used {
            fonts.pair(Name(font.resource), font_ids.font);
        }
        drop(fonts);
        drop(resources);
        drop(writer);
        pdf.stream(*content_id, &content.finish());
    }
    for (font, font_ids) in used {
        font.write(&mut pdf, font_ids)?;
    }
    Ok(pdf.finish())
}

/// Shows encoded text in font `resource` at `x`, `y` (the baseline's start).
fn show(
    content: &mut Content,
    resource: &[u8],
    size: f32,
    x: f32,
    y: f32,
    color: Rgb,
    bytes: &[u8],
) {
    fill(content, color);
    content
        .begin_text()
        .set_font(Name(resource), size)
        .set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y])
        .show(Str(bytes))
        .end_text();
}
