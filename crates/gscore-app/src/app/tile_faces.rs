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
    /// A short abbreviation whose tile opens a dialog.
    TextMore(&'a str),
    /// A word in bold italic, as a score writes it ("tap", "let ring" on two lines).
    Word(&'a str),
    /// A tuplet bracket around its number in the music font's tuplet digits ("3",
    /// "5:4"), or around italic letters ("n:m") when `letters`. `more` adds the corner
    /// triangle of a tile that opens a dialog.
    TupletBracket {
        text: &'a str,
        letters: bool,
        more: bool,
    },
    /// A tie: an engraved arc under two noteheads.
    Tie,
    /// A whole beat tied: two stacked noteheads on each side under one arc.
    BeatTie,
    /// Text above a beat: a "T" under a line.
    BeatText,
    /// A slur over a short line (a beat-level legato run).
    Slur,
    /// A hammer-on or pull-off: an "H" under a slur.
    Hammer,
    /// A bend whose tile opens the bend editor.
    BendMore,
    /// A whammy dip whose tile opens the whammy editor.
    WhammyMore,
    /// A fade-in hairpin ("<").
    Hairpin,
    /// A fade-out hairpin (">").
    HairpinOut,
    /// A volume swell ("<>").
    HairpinSwell,
    /// Text inside a circle ("T", "6"): the left-hand tap and the string number.
    CircledText(&'a str),
    /// A dead slapped beat: a large cross.
    DeadSlap,
    /// An arpeggio: a wavy vertical line with an arrowhead, up for a roll down and down
    /// for a roll up (as Guitar Pro writes them).
    Arpeggio { up: bool },
    /// A pick slide: an "x" and the scrape's wiggle, slanting down or up with the slide.
    PickSlide { down: bool },
    /// A music glyph with the corner triangle of a tile that opens a dialog.
    GlyphMore {
        text: &'a str,
        size: f32,
        baseline: f32,
    },
    /// A slide: a line from `from` to `to`, each -1 (low), 0 (the note) or 1 (high), with
    /// a dot for the note at the end that is 0.
    Slide { from: i8, to: i8 },
    /// A slide from one note to the next: a dot at each end, a slur over it for a legato
    /// slide.
    SlideTo { legato: bool },
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
    /// Free time: a dotted line (no strict meter or metronome).
    FreeTime,
    /// A one-bar repetition: a slash and a dot.
    SimileSingle,
    /// A two-bar repetition: two slashes.
    SimileDouble,
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
pub(crate) fn lettering(
    painter: &Painter,
    at: Pos2,
    text: &str,
    size: f32,
    italic: bool,
    ink: Color32,
) {
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
        Face::TextMore(text) => {
            lettering(painter, c, text, 10.5, false, ink);
            more_corner(painter, rect, ink);
        }
        Face::Word(text) => {
            let size = if text.contains('\n') { 9.5 } else { 11.0 };
            lettering(painter, c, text, size, true, ink);
        }
        Face::TupletBracket {
            text,
            letters,
            more,
        } => tuplet_bracket(painter, rect, text, letters, ink, more),
        Face::Tie => {
            for dx in [-5.0, 6.0] {
                music_glyph(painter, at(dx, 3.0), "\u{ECA5}", 19.0, ink);
            }
            let points = crescent(at(-8.0, 7.0), at(8.0, 7.0), 4.0, 1.6);
            fill_crescent(painter, &points, ink);
        }
        Face::BeatTie => {
            // Two chords, each a pair of stacked heads, joined by one tie.
            for dx in [-6.0, 7.0] {
                for dy in [-3.0, 5.0] {
                    music_glyph(painter, at(dx, dy), "\u{ECA5}", 16.0, ink);
                }
            }
            let points = crescent(at(-7.0, 10.0), at(8.0, 10.0), 4.0, 1.4);
            fill_crescent(painter, &points, ink);
        }
        Face::BeatText => {
            painter.line_segment([at(-8.0, -9.0), at(8.0, -9.0)], Stroke::new(1.5, ink));
            lettering(painter, at(0.0, 2.0), "T", 16.0, false, ink);
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
        Face::BendMore => {
            paint_bend(painter, rect, ink);
            more_corner(painter, rect, ink);
        }
        Face::WhammyMore => {
            paint_whammy(painter, rect, ink);
            more_corner(painter, rect, ink);
        }
        Face::Hairpin => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-9.0, 0.0), at(9.0, -6.0)], stroke);
            painter.line_segment([at(-9.0, 0.0), at(9.0, 6.0)], stroke);
        }
        Face::HairpinOut => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-9.0, -6.0), at(9.0, 0.0)], stroke);
            painter.line_segment([at(-9.0, 6.0), at(9.0, 0.0)], stroke);
        }
        Face::HairpinSwell => {
            let stroke = Stroke::new(1.6, ink);
            painter.line_segment([at(-10.0, 0.0), at(-1.0, -6.0)], stroke);
            painter.line_segment([at(-10.0, 0.0), at(-1.0, 6.0)], stroke);
            painter.line_segment([at(-1.0, -6.0), at(10.0, 0.0)], stroke);
            painter.line_segment([at(-1.0, 6.0), at(10.0, 0.0)], stroke);
        }
        Face::CircledText(text) => {
            painter.circle_stroke(c, 9.5, Stroke::new(1.4, ink));
            lettering(painter, c, text, 12.0, false, ink);
        }
        Face::DeadSlap => {
            let stroke = Stroke::new(2.4, ink);
            painter.line_segment([at(-9.0, -11.0), at(9.0, 11.0)], stroke);
            painter.line_segment([at(-9.0, 11.0), at(9.0, -11.0)], stroke);
        }
        Face::Arpeggio { up } => {
            let stroke = Stroke::new(1.6, ink);
            let x = c.x;
            let (top, bottom) = (c.y - 11.0, c.y + 8.0);
            // A zigzag line, then the arrowhead at the pointing end.
            let step = (bottom - top) / 6.0;
            let mut points = Vec::with_capacity(7);
            for step_index in 0..6 {
                let y = top + step * step_index as f32;
                points.push(egui::pos2(
                    if step_index % 2 == 0 {
                        x - 2.0
                    } else {
                        x + 2.0
                    },
                    y,
                ));
            }
            painter.add(Shape::line(points, stroke));
            let tip = if up {
                egui::pos2(x, top - 4.0)
            } else {
                egui::pos2(x, bottom + 4.0)
            };
            let base = if up {
                egui::pos2(x, top)
            } else {
                egui::pos2(x, bottom)
            };
            painter.add(Shape::convex_polygon(
                vec![tip, base + vec2(4.0, 0.0), base + vec2(-4.0, 0.0)],
                ink,
                Stroke::NONE,
            ));
        }
        Face::PickSlide { down } => {
            lettering(painter, at(-6.0, 1.0), "x", 14.0, false, ink);
            // Screen y grows downwards: a slide down falls to the right.
            let slant = if down { 6.0 } else { -6.0 };
            let stroke = Stroke::new(1.5, ink);
            let (left, right) = (at(-1.0, 0.0), at(10.0, 0.0));
            let mut points = Vec::with_capacity(9);
            for step in 0..9 {
                let t = step as f32 / 8.0;
                points.push(egui::pos2(
                    left.x + (right.x - left.x) * t,
                    left.y + slant * (t - 0.5) + if step % 2 == 0 { -2.5 } else { 2.5 },
                ));
            }
            painter.add(Shape::line(points, stroke));
        }
        Face::GlyphMore {
            text,
            size,
            baseline,
        } => {
            music_glyph(painter, at(0.0, baseline), text, size, ink);
            more_corner(painter, rect, ink);
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
        Face::SlideTo { legato } => {
            let start = at(-8.0, 6.0);
            let end = at(8.0, -2.0);
            painter.line_segment([start, end], Stroke::new(1.8, ink));
            for point in [start, end] {
                painter.circle_filled(point, 3.0, ink);
            }
            if legato {
                let points = crescent(at(-9.0, -4.0), at(9.0, -8.0), -5.0, 1.4);
                fill_crescent(painter, &points, ink);
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
        Face::FreeTime => {
            let stroke = Stroke::new(2.0, ink);
            for step in 0..5 {
                let x = -12.0 + step as f32 * 6.0;
                painter.line_segment([at(x, 3.0), at(x + 3.0, 3.0)], stroke);
            }
        }
        Face::SimileSingle => {
            let stroke = Stroke::new(2.2, ink);
            painter.line_segment([at(-7.0, 9.0), at(4.0, -5.0)], stroke);
            painter.circle_filled(at(8.0, 8.0), 1.8, ink);
        }
        Face::SimileDouble => {
            let stroke = Stroke::new(2.2, ink);
            for dx in [-9.0, 1.0] {
                painter.line_segment([at(dx, 9.0), at(dx + 11.0, -5.0)], stroke);
            }
            painter.circle_filled(at(9.0, 8.0), 1.8, ink);
        }
    }
}

/// The tuplet digits of `text`: 0-9 from U+E880, the colon at U+E88A.
pub(crate) fn tuplet_digits(text: &str) -> String {
    text.chars()
        .filter_map(|ch| match ch {
            ':' => char::from_u32(0xE88A),
            _ => ch.to_digit(10).and_then(|d| char::from_u32(0xE880 + d)),
        })
        .collect()
}

/// A tuplet bracket across `rect` (hooks pointing down) with `text` in its gap: tuplet
/// digits, or bold italic letters when `letters`. With `more`, a small triangle in the
/// bottom right corner says the tile opens a dialog.
pub(crate) fn tuplet_bracket(
    painter: &Painter,
    rect: Rect,
    text: &str,
    letters: bool,
    ink: Color32,
    more: bool,
) {
    let scale = (rect.height() / 32.0).max(0.5);
    let c = rect.center();
    let line_y = c.y - 3.0 * scale;
    let half = if letters {
        let mut job = LayoutJob::default();
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: bold(11.0 * scale),
                color: ink,
                italics: true,
                ..Default::default()
            },
        );
        let galley = painter.layout_job(job);
        let size = galley.size();
        painter.galley(
            egui::pos2(c.x - size.x / 2.0, line_y - size.y / 2.0),
            galley,
            ink,
        );
        size.x / 2.0
    } else {
        // Tuplet digits are about 0.3 em tall: their baseline goes that far below the
        // bracket's middle, half of it.
        let size = 26.0 * scale;
        let digits = tuplet_digits(text);
        let width = painter
            .layout_no_wrap(digits.clone(), music(size), ink)
            .size()
            .x;
        music_glyph(
            painter,
            egui::pos2(c.x, line_y + size * 0.15),
            &digits,
            size,
            ink,
        );
        width / 2.0
    };
    let stroke = Stroke::new(1.3 * scale, ink);
    let gap = 2.0 * scale;
    let (left, right) = (rect.left() + 3.0 * scale, rect.right() - 3.0 * scale);
    let hook = 4.0 * scale;
    for (from, to) in [
        (left, (c.x - half - gap).max(left)),
        ((c.x + half + gap).min(right), right),
    ] {
        if to - from > 1.0 {
            painter.line_segment([egui::pos2(from, line_y), egui::pos2(to, line_y)], stroke);
        }
    }
    for x in [left, right] {
        painter.line_segment(
            [egui::pos2(x, line_y), egui::pos2(x, line_y + hook)],
            stroke,
        );
    }
    if more {
        more_corner(painter, rect, ink);
    }
}

/// A bend: a quarter ellipse that leaves level and rises straight into the base of a solid
/// arrowhead, like the score's bend.
fn paint_bend(painter: &Painter, rect: Rect, ink: Color32) {
    const QUARTER_ELLIPSE: f32 = 0.552_284_8;
    let c = rect.center();
    let at = |dx: f32, dy: f32| egui::pos2(c.x + dx, c.y + dy);
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

/// A whammy dip: down and back up.
fn paint_whammy(painter: &Painter, rect: Rect, ink: Color32) {
    let c = rect.center();
    let at = |dx: f32, dy: f32| egui::pos2(c.x + dx, c.y + dy);
    let points = vec![at(-10.0, -4.0), at(-5.0, -4.0), at(0.0, 6.0), at(5.0, -4.0)];
    painter.add(Shape::line(points, Stroke::new(1.8, ink)));
    painter.line_segment([at(5.0, -4.0), at(10.0, -4.0)], Stroke::new(1.8, ink));
}

/// The small triangle in a tile's bottom right corner that says it opens a dialog.
pub(super) fn more_corner(painter: &Painter, rect: Rect, ink: Color32) {
    let corner = rect.right_bottom() - vec2(2.0, 2.0);
    painter.add(Shape::convex_polygon(
        vec![corner, corner - vec2(6.0, 0.0), corner - vec2(0.0, 6.0)],
        ink,
        Stroke::NONE,
    ));
}
