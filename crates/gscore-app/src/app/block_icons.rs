//! The symbols of the sound chain's blocks, drawn with the painter in the app's flat line
//! style: what each block is, recognisable at a glance on the board.

use std::f32::consts::{PI, TAU};

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::chain_settings::BlockChoice;

/// Draws the symbol of `choice` inside `rect` (square, centered) in `color`. Nothing for
/// an empty place.
pub(super) fn paint(painter: &Painter, rect: Rect, choice: BlockChoice, color: Color32) {
    let size = rect.width().min(rect.height());
    let c = rect.center();
    let stroke = Stroke::new((size * 0.075).max(1.2), color);
    let thin = Stroke::new((size * 0.055).max(1.0), color);
    let at = |x: f32, y: f32| pos2(c.x + x * size, c.y + y * size);
    let wave = |from: f32, to: f32, amplitude: f32, periods: f32, shift: f32, y: f32| {
        (0..=32)
            .map(|step| {
                let share = step as f32 / 32.0;
                let x = from + (to - from) * share;
                at(x, y - amplitude * (share * periods * TAU + shift).sin())
            })
            .collect::<Vec<Pos2>>()
    };
    match choice {
        BlockChoice::Empty => {}
        BlockChoice::Drive => {
            // A wave clipped flat at the top and bottom: what an overdrive does to it.
            let limit = 0.2;
            let points: Vec<Pos2> = (0..=40)
                .map(|step| {
                    let share = step as f32 / 40.0;
                    let y = (-0.42 * (share * 2.0 * TAU).sin()).clamp(-limit, limit);
                    at(-0.44 + 0.88 * share, y)
                })
                .collect();
            painter.add(Shape::line(points, stroke));
        }
        BlockChoice::Amp => {
            // The amplifier sign: a triangle pointing right, a line in and a line out.
            let triangle = vec![at(-0.24, -0.34), at(0.3, 0.0), at(-0.24, 0.34)];
            painter.add(Shape::closed_line(triangle, stroke));
            painter.line_segment([at(-0.46, 0.0), at(-0.24, 0.0)], stroke);
            painter.line_segment([at(0.3, 0.0), at(0.46, 0.0)], stroke);
        }
        BlockChoice::Cabinet => {
            // A speaker seen from the front: the frame, the cone's rim and its dust cap.
            let frame = Rect::from_center_size(c, vec2(size * 0.86, size * 0.86));
            painter.rect_stroke(frame, size * 0.1, thin, egui::StrokeKind::Inside);
            painter.circle_stroke(c, size * 0.3, stroke);
            painter.circle_stroke(c, size * 0.2, thin);
            painter.circle_filled(c, size * 0.09, color);
        }
        BlockChoice::Room => {
            // A room: a house's outline with a sound arc inside it.
            let outline = vec![
                at(-0.38, -0.02),
                at(0.0, -0.4),
                at(0.38, -0.02),
                at(0.38, 0.4),
                at(-0.38, 0.4),
            ];
            painter.add(Shape::closed_line(outline, stroke));
            let arc: Vec<Pos2> = (0..=16)
                .map(|step| {
                    let angle = -PI * 0.3 + PI * 0.6 * step as f32 / 16.0;
                    at(-0.16 + 0.26 * angle.cos(), 0.16 + 0.26 * angle.sin())
                })
                .collect();
            painter.add(Shape::line(arc, thin));
            painter.circle_filled(at(-0.16, 0.16), size * 0.05, color);
        }
        BlockChoice::Reverb => {
            // Sound spreading from a point: a dot and three widening arcs.
            let source = at(-0.36, 0.0);
            painter.circle_filled(source, size * 0.07, color);
            for (index, radius) in [0.26_f32, 0.48, 0.7].into_iter().enumerate() {
                let arc: Vec<Pos2> = (0..=16)
                    .map(|step| {
                        let angle = -PI * 0.28 + PI * 0.56 * step as f32 / 16.0;
                        pos2(
                            source.x + radius * size * angle.cos(),
                            source.y + radius * size * angle.sin(),
                        )
                    })
                    .collect();
                let width = if index == 0 { stroke } else { thin };
                painter.add(Shape::line(arc, width));
            }
        }
        BlockChoice::Delay => {
            // Echoes: one sound and its repeats, each quieter.
            for (index, height) in [0.78_f32, 0.54, 0.34, 0.2].into_iter().enumerate() {
                let x = -0.34 + index as f32 * 0.23;
                painter.line_segment([at(x, -height / 2.0), at(x, height / 2.0)], stroke);
            }
        }
        BlockChoice::Chorus => {
            // Two copies of a wave, slightly apart.
            painter.add(Shape::line(wave(-0.44, 0.44, 0.2, 1.5, 0.0, -0.06), stroke));
            painter.add(Shape::line(wave(-0.44, 0.44, 0.2, 1.5, 0.9, 0.1), thin));
        }
        BlockChoice::Compressor => {
            // The transfer curve: level out against level in, rising, then flattening
            // above the threshold.
            painter.line_segment([at(-0.4, -0.4), at(-0.4, 0.4)], thin);
            painter.line_segment([at(-0.4, 0.4), at(0.4, 0.4)], thin);
            let curve = vec![at(-0.4, 0.4), at(0.0, 0.0), at(0.4, -0.14)];
            painter.add(Shape::line(curve, stroke));
            painter.circle_filled(at(0.0, 0.0), size * 0.06, color);
        }
        BlockChoice::Eq => {
            // Three faders at different heights, as on an equalizer.
            for (index, knob) in [0.14_f32, -0.2, 0.06].into_iter().enumerate() {
                let x = -0.28 + index as f32 * 0.28;
                painter.line_segment([at(x, -0.4), at(x, 0.4)], thin);
                let handle = Rect::from_center_size(at(x, knob), vec2(size * 0.2, size * 0.12));
                painter.rect_filled(handle, size * 0.03, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every block's symbol, light and dark, at the board's size and larger.
    #[test]
    fn the_block_symbols_match_their_snapshot() {
        let choices = [
            BlockChoice::Drive,
            BlockChoice::Amp,
            BlockChoice::Cabinet,
            BlockChoice::Eq,
            BlockChoice::Compressor,
            BlockChoice::Chorus,
            BlockChoice::Delay,
            BlockChoice::Reverb,
            BlockChoice::Room,
        ];
        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(choices.len() as f32 * 64.0 + 16.0, 2.0 * 84.0))
            .wgpu()
            .build_ui(move |ui| {
                for (ground, ink) in [
                    (Color32::WHITE, Color32::from_gray(30)),
                    (Color32::from_gray(36), Color32::from_gray(225)),
                ] {
                    let (row, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 80.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(row, 0.0, ground);
                    for (index, choice) in choices.into_iter().enumerate() {
                        let x = row.left() + 8.0 + index as f32 * 64.0;
                        let large = Rect::from_min_size(pos2(x, row.top() + 4.0), vec2(44.0, 44.0));
                        paint(ui.painter(), large, choice, ink);
                        let small =
                            Rect::from_min_size(pos2(x + 12.0, row.top() + 54.0), vec2(20.0, 20.0));
                        paint(ui.painter(), small, choice, ink);
                    }
                }
            });
        harness.run();
        harness.snapshot("block_symbols");
    }
}
