//! The key-signature picker: a circle of fifths with the major keys outside and their
//! relative minors inside, and the chosen key signature engraved on a small staff in the
//! middle. A click on a segment chooses that key.

use std::f32::consts::TAU;

use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Sense, Shape, Stroke, vec2};

use super::style;
use super::tile_faces::music_glyph;
use crate::fonts::BOLD;

/// Major key names around the circle, clockwise from the top (C), as letter and
/// accidental (`'#'`, `'b'` or none); the bottom three have two spellings.
const MAJOR: [(&str, char); 12] = [
    ("C", ' '),
    ("G", ' '),
    ("D", ' '),
    ("A", ' '),
    ("E", ' '),
    ("B", ' '),
    ("F", '#'),
    ("D", 'b'),
    ("A", 'b'),
    ("E", 'b'),
    ("B", 'b'),
    ("F", ' '),
];

/// The relative minors, in the same order.
const MINOR: [(&str, char); 12] = [
    ("A", ' '),
    ("E", ' '),
    ("B", ' '),
    ("F", '#'),
    ("C", '#'),
    ("G", '#'),
    ("D", '#'),
    ("B", 'b'),
    ("F", ' '),
    ("C", ' '),
    ("G", ' '),
    ("D", ' '),
];

/// Names of the keys with seven accidentals and of the flat spelling of six.
const ENHARMONIC_MAJOR: [(i8, &str, char); 3] = [(-7, "C", 'b'), (-6, "G", 'b'), (7, "C", '#')];
const ENHARMONIC_MINOR: [(i8, &str, char); 3] = [(-7, "A", 'b'), (-6, "E", 'b'), (7, "A", '#')];

/// The segment (0 = top, clockwise) of a key with `key` accidentals (negative: flats).
pub(super) fn segment_of(key: i8) -> usize {
    usize::try_from(i32::from(key).rem_euclid(12)).unwrap_or(0)
}

/// The key a click on `segment` chooses: sharps up to six (F♯), flats from five (D♭).
/// Clicking the segment of the current key keeps its spelling.
fn key_of(segment: usize, current: i8) -> i8 {
    if segment_of(current) == segment {
        return current;
    }
    let key = i8::try_from(segment).unwrap_or(0);
    if key > 6 { key - 12 } else { key }
}

/// The other spelling of a key with five or more accidentals (B = C♭, F♯ = G♭, D♭ = C♯).
pub(super) fn enharmonic(key: i8) -> Option<i8> {
    match key {
        5..=7 => Some(key - 12),
        -7..=-5 => Some(key + 12),
        _ => None,
    }
}

/// The key's name as letter and accidental, e.g. ("F", '#') for F♯ major.
pub(super) fn name(key: i8, minor: bool) -> (&'static str, char) {
    let enharmonics = if minor {
        &ENHARMONIC_MINOR
    } else {
        &ENHARMONIC_MAJOR
    };
    if let Some(&(_, letter, accidental)) = enharmonics.iter().find(|(k, ..)| *k == key) {
        return (letter, accidental);
    }
    let names = if minor { &MINOR } else { &MAJOR };
    names.get(segment_of(key)).copied().unwrap_or(("C", ' '))
}

/// The key's name as plain text ("F# major", "Bb minor").
pub(super) fn display_name(key: i8, minor: bool) -> String {
    let (letter, accidental) = name(key, minor);
    let accidental = accidental.to_string();
    let accidental = accidental.trim();
    let mode = if minor { "minor" } else { "major" };
    format!("{letter}{accidental} {mode}")
}

/// Paints a key name: the letter in bold, the accidental from the music font, an "m" for
/// minor keys.
fn paint_name(
    painter: &Painter,
    at: Pos2,
    (letter, accidental): (&str, char),
    minor: bool,
    size: f32,
    ink: Color32,
) {
    let font = FontId::new(size, FontFamily::Name(BOLD.into()));
    let letter_text = if minor {
        format!("{letter}{}", if accidental == ' ' { "m" } else { "" })
    } else {
        letter.to_owned()
    };
    let galley = painter.layout_no_wrap(letter_text, font.clone(), ink);
    let accidental_glyph = match accidental {
        '#' => Some("\u{E262}"),
        'b' => Some("\u{E260}"),
        _ => None,
    };
    let glyph_width = if accidental_glyph.is_some() {
        size * 0.55
    } else {
        0.0
    };
    let minor_after = minor && accidental != ' ';
    let m_galley = minor_after.then(|| painter.layout_no_wrap("m".to_owned(), font, ink));
    let m_width = m_galley.as_ref().map_or(0.0, |g| g.size().x);
    let total = galley.size().x + glyph_width + m_width;
    let left = at.x - total / 2.0;
    let top = at.y - galley.size().y / 2.0;
    let letter_width = galley.size().x;
    painter.galley(egui::pos2(left, top), galley, ink);
    if let Some(glyph) = accidental_glyph {
        music_glyph(
            painter,
            egui::pos2(
                left + letter_width + glyph_width / 2.0,
                at.y + if accidental == 'b' {
                    size * 0.14
                } else {
                    size * 0.02
                },
            ),
            glyph,
            size * 1.2,
            ink,
        );
    }
    if let Some(m) = m_galley {
        painter.galley(egui::pos2(left + letter_width + glyph_width, top), m, ink);
    }
}

/// A ring segment from angle `from` to `to` (radians, 0 = up, clockwise) between radii.
fn sector(center: Pos2, inner: f32, outer: f32, from: f32, to: f32) -> Vec<Pos2> {
    const STEPS: usize = 12;
    let point = |radius: f32, angle: f32| center + radius * vec2(angle.sin(), -angle.cos());
    let along = |radius: f32| {
        (0..=STEPS).map(move |step| point(radius, from + (to - from) * step as f32 / STEPS as f32))
    };
    along(outer)
        .chain(along(inner).collect::<Vec<_>>().into_iter().rev())
        .collect()
}

/// Fills a ring segment (it is not convex) as triangles between its two arcs.
fn fill_sector(painter: &Painter, points: &[Pos2], color: Color32) {
    let mut mesh = egui::Mesh::default();
    for &point in points {
        mesh.colored_vertex(point, color);
    }
    let n = points.len();
    let half = n / 2;
    for i in 0..half.saturating_sub(1) {
        let (a, b) = (i as u32, (i + 1) as u32);
        let (c, d) = ((n - 1 - i) as u32, (n - 2 - i) as u32);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(Shape::mesh(mesh));
}

/// Staff positions (0 = bottom line, in half spaces) of the sharps and flats of a treble
/// key signature, in the order they are added.
const SHARP_STEPS: [i32; 7] = [8, 5, 9, 6, 3, 7, 4];
const FLAT_STEPS: [i32; 7] = [4, 7, 3, 6, 2, 5, 1];

/// Engraves a treble staff with the key signature, centered at `center`.
pub(super) fn paint_signature(painter: &Painter, center: Pos2, key: i8, space: f32, ink: Color32) {
    let count = usize::from(key.unsigned_abs()).min(7);
    let accidental_width = space * 1.15;
    let width = space * 3.6 + accidental_width * count.max(1) as f32 + space;
    let left = center.x - width / 2.0;
    let bottom = center.y + space * 2.0;
    for line in 0..5 {
        let y = bottom - space * line as f32;
        painter.line_segment(
            [egui::pos2(left, y), egui::pos2(left + width, y)],
            Stroke::new((space * 0.12).max(1.0), ink.gamma_multiply(0.8)),
        );
    }
    let size = space * 4.0;
    // The G clef sits on the second line.
    music_glyph(
        painter,
        egui::pos2(left + space * 1.6, bottom - space),
        "\u{E050}",
        size,
        ink,
    );
    let (steps, glyph) = if key >= 0 {
        (&SHARP_STEPS, "\u{E262}")
    } else {
        (&FLAT_STEPS, "\u{E260}")
    };
    for (index, step) in steps.iter().take(count).enumerate() {
        let x = left + space * 3.6 + accidental_width * (index as f32 + 0.5);
        let y = bottom - space * 0.5 * *step as f32;
        music_glyph(painter, egui::pos2(x, y), glyph, size, ink);
    }
}

/// The circle; `key` and `minor` change on a click. `diameter` is the widget's size.
pub(super) fn circle(ui: &mut egui::Ui, key: &mut i8, minor: &mut bool, diameter: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Circle of fifths: {}", display_name(*key, *minor)),
        )
    });
    let tokens = style::tokens(ui);
    let center = rect.center();
    let outer = diameter / 2.0;
    let middle = outer * 0.70;
    let inner = outer * 0.45;
    let slice = TAU / 12.0;
    // Which segment and ring the pointer is over.
    let hit = |pos: Pos2| -> Option<(usize, bool)> {
        let offset = pos - center;
        let radius = offset.length();
        if !(inner..=outer).contains(&radius) {
            return None;
        }
        let angle = offset.x.atan2(-offset.y).rem_euclid(TAU);
        let segment = ((angle + slice / 2.0) / slice).floor() as usize % 12;
        Some((segment, radius < middle))
    };
    let hovered = response.hover_pos().and_then(hit);
    if response.clicked()
        && let Some((segment, is_minor)) = response.interact_pointer_pos().and_then(hit)
    {
        *key = key_of(segment, *key);
        *minor = is_minor;
    }
    let painter = ui.painter_at(rect);
    let chosen = segment_of(*key);
    for segment in 0..12 {
        let from = segment as f32 * slice - slice / 2.0;
        let to = from + slice;
        for is_minor in [false, true] {
            let (r0, r1) = if is_minor {
                (inner, middle)
            } else {
                (middle, outer)
            };
            let on = segment == chosen && is_minor == *minor;
            let fill = if on {
                tokens.accent
            } else if hovered == Some((segment, is_minor)) {
                tokens.control_active
            } else if is_minor {
                tokens.control
            } else {
                tokens.control_hover
            };
            fill_sector(&painter, &sector(center, r0, r1, from, to), fill);
            let ink = if on { tokens.on_accent } else { tokens.text };
            let mid_angle = (from + to) / 2.0;
            let radius = (r0 + r1) / 2.0;
            let at = center + radius * vec2(mid_angle.sin(), -mid_angle.cos());
            let names = if is_minor { &MINOR } else { &MAJOR };
            let label = if on {
                name(*key, *minor)
            } else {
                names.get(segment).copied().unwrap_or(("C", ' '))
            };
            let size = if is_minor {
                outer * 0.085
            } else {
                outer * 0.115
            };
            paint_name(&painter, at, label, is_minor, size, ink);
        }
    }
    // Gaps between the segments and the rings, in the window's color.
    let gap = Stroke::new(2.5, ui.visuals().window_fill);
    for segment in 0..12 {
        let angle = segment as f32 * slice - slice / 2.0;
        let direction = vec2(angle.sin(), -angle.cos());
        painter.line_segment(
            [center + inner * direction, center + outer * direction],
            gap,
        );
    }
    painter.circle_stroke(center, middle, gap);
    // The middle: the key signature on a staff and how many accidentals it has.
    painter.circle_filled(center, inner - 2.0, ui.visuals().extreme_bg_color);
    paint_signature(
        &painter,
        center - vec2(0.0, inner * 0.1),
        *key,
        inner * 0.105,
        tokens.text,
    );
    let count = key.unsigned_abs();
    let accidentals = match (count, *key > 0) {
        (0, _) => "no sharps or flats".to_owned(),
        (1, true) => "1 sharp".to_owned(),
        (1, false) => "1 flat".to_owned(),
        (n, true) => format!("{n} sharps"),
        (n, false) => format!("{n} flats"),
    };
    painter.text(
        center + vec2(0.0, inner * 0.58),
        Align2::CENTER_CENTER,
        accidentals,
        FontId::proportional(inner * 0.16),
        tokens.text_weak,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_and_spellings() {
        assert_eq!(segment_of(0), 0);
        assert_eq!(segment_of(1), 1);
        assert_eq!(segment_of(-1), 11);
        assert_eq!(segment_of(-6), 6);
        assert_eq!(key_of(7, 0), -5);
        assert_eq!(key_of(6, 0), 6);
        assert_eq!(key_of(6, -6), -6);
        assert_eq!(enharmonic(5), Some(-7));
        assert_eq!(enharmonic(-5), Some(7));
        assert_eq!(enharmonic(4), None);
        assert_eq!(display_name(-3, false), "Eb major");
        assert_eq!(display_name(3, true), "F# minor");
        assert_eq!(display_name(-7, false), "Cb major");
        assert_eq!(display_name(0, true), "A minor");
    }
}
