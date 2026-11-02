//! Small monochrome icons drawn with the painter (GuitarScore's own work, no third-party
//! assets or fonts): toolbar actions, so the app does not depend on an icon set.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Vec2, vec2};

/// The icons the app draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    /// Play triangle.
    Play,
    /// Pause bars.
    Pause,
    /// Stop square.
    Stop,
    /// Save (a downward arrow into a tray).
    Save,
    /// Open (a folder).
    Open,
    /// Undo (an arrow curving left).
    Undo,
    /// Redo (an arrow curving right).
    Redo,
    /// Zoom in (a magnifier with a plus).
    ZoomIn,
    /// Zoom out (a magnifier with a minus).
    ZoomOut,
    /// Settings (a gear).
    Settings,
    /// Add (a plus).
    Add,
    /// Move up (an arrow pointing up).
    MoveUp,
    /// Move down (an arrow pointing down).
    MoveDown,
}

/// A square icon button with a tooltip and an accessibility label.
pub(crate) fn button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> Response {
    let size = vec2(26.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Button);
    });
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    paint(
        ui.painter(),
        rect.shrink(6.0),
        visuals.fg_stroke.color,
        icon,
    );
    response.on_hover_text(tip)
}

/// Draws `icon` inside `rect` in `color`.
pub(crate) fn paint(painter: &Painter, rect: Rect, color: Color32, icon: Icon) {
    let stroke = Stroke::new(1.5, color);
    let center = rect.center();
    let size = rect.width().min(rect.height());
    match icon {
        Icon::Play => {
            let half = size * 0.45;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + vec2(-half * 0.7, -half),
                    center + vec2(half, 0.0),
                    center + vec2(-half * 0.7, half),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            let bar = vec2(size * 0.18, size * 0.9);
            for dx in [-size * 0.22, size * 0.22] {
                painter.rect_filled(
                    Rect::from_center_size(center + vec2(dx, 0.0), bar),
                    1.0,
                    color,
                );
            }
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_center_size(center, Vec2::splat(size * 0.75)),
                2.0,
                color,
            );
        }
        Icon::Save => {
            let arrow = rect.shrink(size * 0.2);
            painter.line_segment(
                [
                    Pos2::new(center.x, arrow.top()),
                    Pos2::new(center.x, arrow.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(arrow.left(), center.y),
                    Pos2::new(center.x, arrow.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(arrow.right(), center.y),
                    Pos2::new(center.x, arrow.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(rect.left(), rect.bottom()),
                    Pos2::new(rect.right(), rect.bottom()),
                ],
                stroke,
            );
        }
        Icon::Open => {
            let folder = Rect::from_center_size(
                center + vec2(0.0, size * 0.08),
                vec2(size * 0.95, size * 0.65),
            );
            painter.rect_stroke(folder, 2.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(folder.left(), folder.top()),
                    Pos2::new(folder.left() + size * 0.35, folder.top() - size * 0.18),
                ],
                stroke,
            );
        }
        Icon::Undo | Icon::Redo => {
            let right = matches!(icon, Icon::Redo);
            let arrow = rect.shrink(size * 0.15);
            let start = Pos2::new(
                if right { arrow.left() } else { arrow.right() },
                arrow.center().y,
            );
            let tip = Pos2::new(
                if right { arrow.right() } else { arrow.left() },
                arrow.center().y - size * 0.05,
            );
            painter.line_segment([start, tip], stroke);
            let back = if right { -1.0 } else { 1.0 };
            painter.line_segment([tip, tip + vec2(back * size * 0.28, -size * 0.28)], stroke);
            painter.line_segment([tip, tip + vec2(back * size * 0.28, size * 0.28)], stroke);
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            let radius = size * 0.32;
            let lens = center + vec2(-size * 0.08, -size * 0.08);
            painter.circle_stroke(lens, radius, stroke);
            painter.line_segment(
                [
                    lens + Vec2::angled(std::f32::consts::FRAC_PI_4) * radius,
                    rect.right_bottom() - vec2(size * 0.08, size * 0.08),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    lens + vec2(-radius * 0.5, 0.0),
                    lens + vec2(radius * 0.5, 0.0),
                ],
                stroke,
            );
            if matches!(icon, Icon::ZoomIn) {
                painter.line_segment(
                    [
                        lens + vec2(0.0, -radius * 0.5),
                        lens + vec2(0.0, radius * 0.5),
                    ],
                    stroke,
                );
            }
        }
        Icon::Settings => {
            painter.circle_stroke(center, size * 0.24, stroke);
            for step in 0..8 {
                let angle = std::f32::consts::TAU * (step as f32) / 8.0;
                let direction = Vec2::angled(angle);
                painter.line_segment(
                    [
                        center + direction * (size * 0.34),
                        center + direction * (size * 0.48),
                    ],
                    stroke,
                );
            }
        }
        Icon::Add => {
            let half = size * 0.42;
            painter.line_segment([center - vec2(half, 0.0), center + vec2(half, 0.0)], stroke);
            painter.line_segment([center - vec2(0.0, half), center + vec2(0.0, half)], stroke);
        }
        Icon::MoveUp | Icon::MoveDown => {
            // Drawn pointing up, mirrored for down.
            let sign = if matches!(icon, Icon::MoveUp) {
                1.0
            } else {
                -1.0
            };
            let half = size * 0.45;
            let tip = center - vec2(0.0, half * sign);
            painter.line_segment([tip, center + vec2(0.0, half * sign)], stroke);
            for side in [-1.0, 1.0] {
                painter.line_segment(
                    [tip, tip + vec2(side * half * 0.7, half * 0.7 * sign)],
                    stroke,
                );
            }
        }
    }
}
