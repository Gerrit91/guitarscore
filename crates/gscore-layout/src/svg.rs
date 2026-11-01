//! Renders draw operations to SVG, for golden tests and `gscore-cli render`.
//!
//! The SVG names the fonts (Bravura for glyphs, Noto Sans for text) instead of embedding
//! them; a viewer needs them installed (they are in `assets/fonts/`). Numbers are written
//! with two decimals so the output is stable across platforms.

use std::fmt::Write as _;

use crate::Layout;
use crate::draw::{DrawOp, TextAnchor, TextFont};
use crate::theme::Theme;

/// Font family of music glyphs.
const MUSIC_FAMILY: &str = "Bravura";
/// Font family of text.
const TEXT_FAMILY: &str = "Noto Sans";

/// The layout as an SVG document. `staff_space_mm` sets the physical size.
pub fn to_svg(layout: &Layout, theme: &Theme, staff_space_mm: f32) -> String {
    let mut out = String::new();
    // Writing to a `String` cannot fail; the results are ignored on purpose.
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" \
         viewBox=\"0 0 {} {}\">",
        num(layout.width * staff_space_mm),
        num(layout.height * staff_space_mm),
        num(layout.width),
        num(layout.height)
    );
    let _ = writeln!(
        out,
        "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
        theme.paper
    );
    for op in &layout.ops {
        write_op(&mut out, op, theme);
    }
    out.push_str("</svg>\n");
    out
}

fn write_op(out: &mut String, op: &DrawOp, theme: &Theme) {
    let _ = match op {
        DrawOp::Line {
            from,
            to,
            thickness,
            color,
        } => writeln!(
            out,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            num(from.x),
            num(from.y),
            num(to.x),
            num(to.y),
            theme.color(*color),
            num(*thickness)
        ),
        DrawOp::Rect { rect, color } => writeln!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            num(rect.origin.x),
            num(rect.origin.y),
            num(rect.width),
            num(rect.height),
            theme.color(*color)
        ),
        DrawOp::Glyph {
            glyph,
            at,
            scale,
            color,
        } => writeln!(
            out,
            "<text x=\"{}\" y=\"{}\" font-family=\"{MUSIC_FAMILY}\" font-size=\"{}\" \
             fill=\"{}\" data-glyph=\"{}\">&#x{:X};</text>",
            num(at.x),
            num(at.y),
            // One em of a SMuFL font is four staff spaces.
            num(4.0 * scale),
            theme.color(*color),
            glyph.name(),
            u32::from(glyph.codepoint())
        ),
        DrawOp::Text {
            text,
            at,
            size,
            font,
            anchor,
            color,
        } => {
            let weight = match font {
                TextFont::Regular => "",
                TextFont::Bold => " font-weight=\"bold\"",
            };
            let anchor = match anchor {
                TextAnchor::Start => "",
                TextAnchor::Middle => " text-anchor=\"middle\"",
                TextAnchor::End => " text-anchor=\"end\"",
            };
            writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" font-family=\"{TEXT_FAMILY}\" font-size=\"{}\"{weight}{anchor} \
                 fill=\"{}\">{}</text>",
                num(at.x),
                num(at.y),
                num(*size),
                theme.color(*color),
                escape(text)
            )
        }
    };
}

/// A number with at most two decimals, without trailing zeros (`1.5`, `2`, `-0.25`).
fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".to_owned(),
        other => other.to_owned(),
    }
}

/// Escapes text for XML.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters are not allowed in XML 1.0 text.
            c if c.is_control() && c != '\t' && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_stable() {
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(0.126), "0.13");
        assert_eq!(num(f32::NAN), "0");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape("<A & B> \"x\"\u{0}"),
            "&lt;A &amp; B&gt; &quot;x&quot;"
        );
        assert_eq!(escape("Sérénade ♩"), "Sérénade ♩");
    }
}
