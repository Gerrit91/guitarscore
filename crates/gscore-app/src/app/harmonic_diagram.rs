//! The picture of a string's partials in the harmonic editor, like the classic diagram of
//! harmonics on a string: the vibrating length with one, two, three ... loops and its
//! nodes, the chosen partial shaded in the accent color, a fret ruler over it and the touch
//! point marked.

use egui::{Color32, Painter, Pos2, Shape, Stroke};

use super::harmonic_editor::DIAGRAM;
use super::style;

/// Partials always drawn.
const SHOWN_PARTIALS: u32 = 6;

/// Draws the picture: the string vibrates from `from_fret` (0: the nut) to the bridge, is
/// touched `touch` frets from there, and sounds the `chosen` partial.
pub(super) fn draw(ui: &mut egui::Ui, from_fret: u8, touch: Option<u8>, chosen: Option<u32>) {
    let (rect, _) = ui.allocate_exact_size(DIAGRAM, egui::Sense::hover());
    let tokens = style::tokens(ui);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
    let left = rect.left() + 118.0;
    let right = rect.right() - 16.0;
    let ruler = rect.top() + 26.0;
    let x_at = |frets: f64| left + (right - left) * (1.0 - 2_f64.powf(-frets / 12.0)) as f32;
    paint_ruler(
        &painter,
        left,
        right,
        ruler,
        from_fret,
        &x_at,
        tokens.text_weak,
    );
    let rows = SHOWN_PARTIALS.max(chosen.unwrap_or(0));
    let row_height = (rect.bottom() - ruler - 16.0) / rows as f32;
    for n in 1..=rows {
        let y = ruler + 8.0 + row_height * (n as f32 - 0.5);
        let on = chosen == Some(n);
        let ink = if on { tokens.accent } else { tokens.text_weak };
        let label = match n {
            1 => "1  fundamental".to_owned(),
            _ => format!("{n}  partial"),
        };
        painter.text(
            Pos2::new(rect.left() + 12.0, y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            if on { tokens.text } else { tokens.text_weak },
        );
        paint_partial(&painter, left, right, y, row_height * 0.36, n, on, ink);
    }
    // The touch point: a line down over the frets, and a finger on the chosen partial.
    if let Some(frets) = touch {
        let x = x_at(f64::from(frets));
        let dash = Stroke::new(1.0, tokens.accent.gamma_multiply(0.7));
        painter.add(Shape::dashed_line(
            &[Pos2::new(x, ruler), Pos2::new(x, rect.bottom() - 8.0)],
            dash,
            4.0,
            3.0,
        ));
        if let Some(n) = chosen {
            let y = ruler + 8.0 + row_height * (n as f32 - 0.5);
            painter.circle_filled(Pos2::new(x, y), 6.0, tokens.accent);
            painter.circle_stroke(Pos2::new(x, y), 6.0, Stroke::new(1.5, Color32::WHITE));
        }
    }
}

/// The fret ruler over the vibrating length: a mark per fret, numbers on the dotted frets.
fn paint_ruler(
    painter: &Painter,
    left: f32,
    right: f32,
    y: f32,
    from_fret: u8,
    x_at: &dyn Fn(f64) -> f32,
    ink: Color32,
) {
    painter.line_segment(
        [Pos2::new(left, y), Pos2::new(right, y)],
        Stroke::new(1.0, ink),
    );
    // The nut or the fretted note, and the bridge.
    for x in [left, right] {
        painter.line_segment(
            [Pos2::new(x, y - 6.0), Pos2::new(x, y + 6.0)],
            Stroke::new(2.0, ink),
        );
    }
    for frets in 1..=24_u8 {
        let x = x_at(f64::from(frets));
        let absolute = from_fret.saturating_add(frets);
        painter.line_segment(
            [Pos2::new(x, y - 3.0), Pos2::new(x, y + 3.0)],
            Stroke::new(1.0, ink),
        );
        if matches!(absolute % 12, 0 | 3 | 5 | 7 | 9) && absolute <= 36 {
            painter.text(
                Pos2::new(x, y - 6.0),
                egui::Align2::CENTER_BOTTOM,
                absolute.to_string(),
                egui::FontId::proportional(10.0),
                ink,
            );
        }
    }
}

/// One partial: the string's envelope with `n` loops between `left` and `right`, its
/// nodes as dots.
#[allow(clippy::too_many_arguments)]
fn paint_partial(
    painter: &Painter,
    left: f32,
    right: f32,
    y: f32,
    amplitude: f32,
    n: u32,
    on: bool,
    ink: Color32,
) {
    const STEPS: usize = 160;
    let width = if on { 2.0 } else { 1.2 };
    let swing_at = |t: f32| (std::f32::consts::PI * n as f32 * t).sin();
    if on {
        // The chosen partial's loops, shaded between the two sides.
        let mut mesh = egui::Mesh::default();
        let shade = ink.gamma_multiply(0.22);
        for step in 0..=STEPS {
            let t = step as f32 / STEPS as f32;
            let x = left + (right - left) * t;
            let swing = amplitude * swing_at(t);
            mesh.colored_vertex(Pos2::new(x, y - swing), shade);
            mesh.colored_vertex(Pos2::new(x, y + swing), shade);
        }
        for step in 0..STEPS as u32 {
            let i = step * 2;
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
    for sign in [-1.0_f32, 1.0] {
        let points: Vec<Pos2> = (0..=STEPS)
            .map(|step| {
                let t = step as f32 / STEPS as f32;
                Pos2::new(
                    left + (right - left) * t,
                    y + sign * amplitude * swing_at(t),
                )
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(width, ink)));
    }
    for node in 0..=n {
        let x = left + (right - left) * node as f32 / n as f32;
        painter.circle_filled(Pos2::new(x, y), if on { 3.0 } else { 2.0 }, ink);
    }
}
