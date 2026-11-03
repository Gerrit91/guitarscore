//! What palette tiles show: engraved music-font glyphs where Bravura has the sign, bold
//! or bold-italic lettering for text marks (as printed scores write them), and a few
//! signs drawn with filled shapes where the font has none.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, vec2};

use crate::fonts::{BOLD, MUSIC};

/// What a tile shows.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Face<'a> {
    /// A SMuFL glyph: its code point, font size and how far below the tile's center its
    /// baseline sits (notes with stems sit low, a whole note in the middle).
    Glyph {
        text: &'a str,
        size: f32,
        baseline: f32,
    },
    /// A short abbreviation in bold ("P.M.", "A.H.").
    Text(&'a str),
    /// A word in bold italic, as a score writes it ("tap", "let ring" on two lines).
    Word(&'a str),
    /// A tuplet number in the music font's italic digits ("3", "5:4").
    Tuplet(&'a str),
    /// A tie: an engraved arc under two noteheads.
    Tie,
    /// A slur over a short line (a beat-level legato run).
    Slur,
    /// A hammer-on or pull-off: an "H" under a slur.
    Hammer,
    /// A bend: a curve rising to an arrow.
    Bend,
    /// A whammy dip: down and back up.
    Whammy,
    /// A fade-in hairpin ("<").
    Hairpin,
    /// A slide: a line from `from` to `to`, each -1 (low), 0 (the note) or 1 (high), with
    /// a dot for the note at the end that is 0.
    Slide { from: i8, to: i8 },
    /// A line break: a line that turns down into the next system.
    Break,
    /// Bars on a line: short barlines over a baseline.
    Bars,
    /// A tempo mark: a quarter note followed by "=".
    Tempo,
    /// A time signature: 4 over 4.
    TimeSig,
    /// A key signature: three sharps.
    KeySig,
    /// A section (rehearsal) mark: a framed "A".
    Section,
    /// A start repeat sign.
    RepeatStart,
    /// An end repeat sign.
    RepeatEnd,
    /// A double barline.
    DoubleBar,
    /// An alternative-ending bracket with "1.".
    Endings,
    /// A swing feel: a quarter and an eighth under a triplet bracket.
    Swing,
    /// A fermata.
    Fermata,
}

/// The music font at `size`.
fn music(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MUSIC.into()))
}

/// The bold text font at `size`.
fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// Paints a music-font glyph with its baseline (not its box) centered at `at`.
pub(crate) fn music_glyph(painter: &Painter, at: Pos2, text: &str, size: f32, ink: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), music(size), ink);
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(0.0);
    let width = galley.size().x;
    painter.galley(egui::pos2(at.x - width / 2.0, at.y - baseline), galley, ink);
}

/// Paints bold text centered on `at`, slanted when `italic`; several lines stack.
fn lettering(painter: &Painter, at: Pos2, text: &str, size: f32, italic: bool, ink: Color32) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: bold(size),
            color: ink,
            italics: italic,
            line_height: Some(size * 0.95),
            ..Default::default()
        },
    );
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    let size = galley.size();
    painter.galley(at - vec2(0.0, size.y / 2.0), galley, ink);
}

/// Points of a filled crescent (an engraved tie or slur) from `left` to `right`, bulging
/// by `rise` (negative: upwards) and `thick` at its middle.
fn crescent(left: Pos2, right: Pos2, rise: f32, thick: f32) -> Vec<Pos2> {
    const STEPS: usize = 16;
    let point = |t: f32, bulge: f32| {
        let x = left.x + (right.x - left.x) * t;
        let y = left.y + (right.y - left.y) * t + bulge * 4.0 * t * (1.0 - t);
        egui::pos2(x, y)
    };
    let outer = (0..=STEPS).map(|step| point(step as f32 / STEPS as f32, rise));
    let inner = (0..=STEPS)
        .rev()
        .map(|step| point(step as f32 / STEPS as f32, rise - thick * rise.signum()));
    outer.chain(inner).collect()
}

/// Fills a crescent (not convex, so as a triangle fan of its two edges).
fn fill_crescent(painter: &Painter, points: &[Pos2], ink: Color32) {
    let half = points.len() / 2;
    let mut mesh = egui::Mesh::default();
    for &point in points {
        mesh.colored_vertex(point, ink);
    }
    for i in 0..half.saturating_sub(1) {
        let (a, b) = (i as u32, (i + 1) as u32);
        let (c, d) = ((points.len() - 1 - i) as u32, (points.len() - 2 - i) as u32);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(Shape::mesh(mesh));
}

/// Paints `face` into the tile `rect` with `ink`.
pub(super) fn paint_face(painter: &Painter, rect: Rect, face: Face<'_>, ink: Color32) {
    let c = rect.center();
    let at = |dx: f32, dy: f32| egui::pos2(c.x + dx, c.y + dy);
    match face {
        Face::Glyph {
            text,
            size,
            baseline,
        } => music_glyph(painter, at(0.0, baseline), text, size, ink),
        Face::Text(text) => lettering(painter, c, text, 10.5, false, ink),
        Face::Word(text) => {
            let size = if text.contains('\n') { 9.5 } else { 11.0 };
            lettering(painter, c, text, size, true, ink);
        }
        Face::Tuplet(text) => {
            // The tuplet digits: 0-9 from U+E880, the colon at U+E88A.
            let digits: String = text
                .chars()
                .filter_map(|ch| match ch {
                    ':' => char::from_u32(0xE88A),
                    _ => ch.to_digit(10).and_then(|d| char::from_u32(0xE880 + d)),
                })
                .collect();
            music_glyph(painter, at(0.0, 5.0), &digits, 24.0, ink);
        }
        Face::Tie => {
            for dx in [-5.0, 6.0] {
                music_glyph(painter, at(dx, 3.0), "\u{ECA5}", 19.0, ink);
            }
            let points = crescent(at(-8.0, 7.0), at(8.0, 7.0), 4.0, 1.6);
            fill_crescent(painter, &points, ink);
        }
        Face::Slur => {
            let points = crescent(at(-9.0, 5.0), at(9.0, 5.0), -10.0, 2.0);
            fill_crescent(painter, &points, ink);
        }
        Face::Hammer => {
            let points = crescent(at(-8.0, -6.0), at(8.0, -6.0), -5.0, 1.5);
            fill_crescent(painter, &points, ink);
            lettering(painter, at(0.0, 4.0), "H", 12.0, false, ink);
        }
        Face::Bend => {
            // Like the score's bend: a quarter ellipse that leaves level and rises straight
            // into the base of a solid arrowhead.
            const QUARTER_ELLIPSE: f32 = 0.552_284_8;
            let start = at(-8.0, 8.0);
            let tip = at(5.0, -10.0);
            let base = tip + vec2(0.0, 7.0);
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    start,
                    egui::pos2(start.x + (base.x - start.x) * QUARTER_ELLIPSE, start.y),
                    egui::pos2(base.x, base.y + (start.y - base.y) * QUARTER_ELLIPSE),
                    base,
                ],
                false,
                Color32::TRANSPARENT,
                Stroke::new(1.8, ink),
            ));
            painter.add(Shape::convex_polygon(
                vec![tip, base + vec2(4.0, 0.5), base + vec2(-4.0, 0.5)],
                ink,
                Stroke::NONE,
            ));
        }
        Face::Whammy => {
            let points = vec![at(-10.0, -4.0), at(-5.0, -4.0), at(0.0, 6.0), at(5.0, -4.0)];
            painter.add(Shape::line(points, Stroke::new(1.8, ink)));
            painter.line_segment([at(5.0, -4.0), at(10.0, -4.0)], Stroke::new(1.8, ink));
        }
        Face::Hairpin => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-9.0, 0.0), at(9.0, -6.0)], stroke);
            painter.line_segment([at(-9.0, 0.0), at(9.0, 6.0)], stroke);
        }
        Face::Slide { from, to } => {
            let y = |level: i8| c.y - 6.0 * f32::from(level);
            let start = egui::pos2(rect.left() + 6.0, y(from));
            let end = egui::pos2(rect.right() - 6.0, y(to));
            painter.line_segment([start, end], Stroke::new(1.8, ink));
            for (level, point) in [(from, start), (to, end)] {
                if level == 0 {
                    painter.circle_filled(point, 3.0, ink);
                }
            }
        }
        Face::Break => {
            let stroke = Stroke::new(1.8, ink);
            let corner = at(6.0, -6.0);
            painter.line_segment([at(-10.0, -6.0), corner], stroke);
            painter.line_segment([corner, at(6.0, 3.0)], stroke);
            painter.add(Shape::convex_polygon(
                vec![at(6.0, 9.0), at(2.0, 3.0), at(10.0, 3.0)],
                ink,
                Stroke::NONE,
            ));
        }
        Face::Bars => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-10.0, 7.0), at(10.0, 7.0)], stroke);
            for dx in [-6.0, 0.0, 6.0] {
                painter.line_segment([at(dx, -8.0), at(dx, 7.0)], stroke);
            }
        }
        Face::Tempo => {
            music_glyph(painter, at(-5.0, 7.0), "\u{ECA5}", 22.0, ink);
            lettering(painter, at(6.0, 1.0), "=", 13.0, false, ink);
        }
        Face::TimeSig => {
            for dy in [-5.5, 5.5] {
                music_glyph(painter, at(0.0, dy), "\u{E084}", 20.0, ink);
            }
        }
        Face::KeySig => {
            // F#, C# and G# as a treble staff places them.
            for (dx, dy) in [(-7.0, -2.0), (0.0, 4.0), (7.0, -5.0)] {
                music_glyph(painter, at(dx, dy), "\u{E262}", 20.0, ink);
            }
        }
        Face::Section => {
            let frame = Rect::from_center_size(c, vec2(16.0, 17.0));
            painter.rect_stroke(frame, 2.0, Stroke::new(1.4, ink), egui::StrokeKind::Inside);
            lettering(painter, c, "A", 12.0, false, ink);
        }
        Face::RepeatStart => music_glyph(painter, at(0.0, 12.0), "\u{E040}", 24.0, ink),
        Face::RepeatEnd => music_glyph(painter, at(0.0, 12.0), "\u{E041}", 24.0, ink),
        Face::DoubleBar => music_glyph(painter, at(0.0, 12.0), "\u{E031}", 24.0, ink),
        Face::Endings => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-10.0, -9.0), at(10.0, -9.0)], stroke);
            painter.line_segment([at(-10.0, -9.0), at(-10.0, 6.0)], stroke);
            lettering(painter, at(1.0, 0.0), "1.", 12.0, false, ink);
        }
        Face::Swing => {
            // Bravura's text notes: bracket start, quarter, 3, eighth, bracket end.
            music_glyph(
                painter,
                at(0.0, 8.0),
                "\u{E201}\u{E1F1}\u{E202}\u{E1F3}\u{E203}",
                15.0,
                ink,
            );
        }
        Face::Fermata => music_glyph(painter, at(0.0, 4.0), "\u{E4C0}", 30.0, ink),
    }
}
