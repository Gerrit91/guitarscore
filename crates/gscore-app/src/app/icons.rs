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
    /// Metronome.
    Metronome,
    /// Count-in (a metronome followed by a play sign).
    CountIn,
    /// Loop (a circular arrow with a play sign).
    Loop,
    /// Go to the top of the score (a bar and a left-pointing triangle).
    ToStart,
    /// One bar back (two left-pointing triangles).
    PreviousBar,
    /// One bar forward (two right-pointing triangles).
    NextBar,
    /// Go to the end of the score (a right-pointing triangle and a bar).
    ToEnd,
    /// Save (a floppy disk).
    Save,
    /// A new song (a page with a plus).
    NewSong,
    /// Restore a default (a counter-clockwise arrow).
    Revert,
    /// The edit history (a clock).
    History,
    /// The mixer view (fader strips).
    Mixer,
    /// The measure grid (a 2x2 grid).
    Grid,
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
    /// Minimize the window (a line at the bottom).
    WindowMinimize,
    /// Maximize the window (a square).
    WindowMaximize,
    /// Restore the window (two overlapping squares).
    WindowRestore,
    /// Close the window (a cross).
    WindowClose,
    /// The left panel (a frame with its left part filled).
    PanelLeft,
    /// The bottom panel.
    PanelBottom,
    /// The right panel.
    PanelRight,
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

/// An icon button that shows whether something is on (a panel shown): highlighted while on.
pub(crate) fn toggle_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tip: &str,
    on: bool,
) -> Response {
    let size = vec2(26.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
    let visuals = ui.style().interact(&response);
    if on {
        ui.painter().rect_filled(
            rect,
            4.0,
            ui.visuals().selection.bg_fill.gamma_multiply(0.35),
        );
    } else if response.hovered() {
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
        Icon::Metronome => {
            paint_metronome(painter, center, size, color);
        }
        Icon::CountIn => {
            // A metronome, then a play sign: click a bar, then start.
            paint_metronome(
                painter,
                egui::pos2(rect.left() + size * 0.34, center.y),
                size * 0.72,
                color,
            );
            paint_triangle(
                painter,
                egui::pos2(rect.right() - size * 0.22, center.y),
                size * 0.2,
                true,
                color,
            );
        }
        Icon::Loop => {
            let radius = size * 0.44;
            let start = std::f32::consts::FRAC_PI_2;
            let sweep = std::f32::consts::PI * 1.5;
            let points: Vec<Pos2> = (0..=18)
                .map(|step| {
                    let angle = start + sweep * (step as f32) / 18.0;
                    center + Vec2::angled(angle) * radius
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            let angle = start + sweep;
            let tip = center + Vec2::angled(angle) * radius;
            let direction = Vec2::angled(angle);
            let tangent = Vec2::new(-direction.y, direction.x);
            painter.line_segment(
                [tip, tip - tangent * (size * 0.2) + direction * (size * 0.1)],
                stroke,
            );
            painter.line_segment(
                [tip, tip - tangent * (size * 0.2) - direction * (size * 0.1)],
                stroke,
            );
            // A play triangle in the middle.
            let half = size * 0.13;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + vec2(-half * 0.55, -half),
                    center + vec2(half, 0.0),
                    center + vec2(-half * 0.55, half),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::ToStart => {
            let x = rect.left() + size * 0.22;
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + size * 0.1),
                    egui::pos2(x, rect.bottom() - size * 0.1),
                ],
                stroke,
            );
            paint_triangle(
                painter,
                egui::pos2(rect.left() + size * 0.6, center.y),
                size * 0.3,
                false,
                color,
            );
        }
        Icon::PreviousBar => {
            for offset in [-size * 0.22, size * 0.22] {
                paint_triangle(
                    painter,
                    center + vec2(offset, 0.0),
                    size * 0.24,
                    false,
                    color,
                );
            }
        }
        Icon::NextBar => {
            for offset in [-size * 0.22, size * 0.22] {
                paint_triangle(
                    painter,
                    center + vec2(offset, 0.0),
                    size * 0.24,
                    true,
                    color,
                );
            }
        }
        Icon::ToEnd => {
            paint_triangle(
                painter,
                egui::pos2(rect.right() - size * 0.6, center.y),
                size * 0.3,
                true,
                color,
            );
            let x = rect.right() - size * 0.22;
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + size * 0.1),
                    egui::pos2(x, rect.bottom() - size * 0.1),
                ],
                stroke,
            );
        }
        Icon::Save => {
            // A floppy disk: body, the shutter at the top and the label at the bottom.
            let body = rect.shrink(size * 0.08);
            painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            let shutter = Rect::from_min_size(
                egui::pos2(body.left() + size * 0.22, body.top()),
                vec2(size * 0.34, size * 0.3),
            );
            painter.rect_filled(shutter, 0.0, color);
            let label = Rect::from_min_size(
                egui::pos2(body.left() + size * 0.16, body.bottom() - size * 0.34),
                vec2(size * 0.52, size * 0.28),
            );
            painter.rect_filled(label, 1.0, color);
        }
        Icon::NewSong => {
            let page = Rect::from_center_size(
                center + vec2(-size * 0.08, 0.0),
                vec2(size * 0.62, size * 0.9),
            );
            painter.rect_stroke(page, 1.0, stroke, egui::StrokeKind::Inside);
            // The folded corner.
            painter.line_segment(
                [
                    egui::pos2(page.right() - size * 0.24, page.top()),
                    egui::pos2(page.right() - size * 0.24, page.top() + size * 0.24),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(page.right() - size * 0.24, page.top() + size * 0.24),
                    egui::pos2(page.right(), page.top() + size * 0.24),
                ],
                stroke,
            );
            // A plus at the lower right.
            let plus = egui::pos2(page.right() + size * 0.16, page.bottom() - size * 0.1);
            painter.line_segment(
                [plus - vec2(size * 0.16, 0.0), plus + vec2(size * 0.16, 0.0)],
                stroke,
            );
            painter.line_segment(
                [plus - vec2(0.0, size * 0.16), plus + vec2(0.0, size * 0.16)],
                stroke,
            );
        }
        Icon::Revert => {
            let radius = size * 0.38;
            let start = -std::f32::consts::FRAC_PI_2;
            let sweep = -std::f32::consts::PI * 1.5;
            let points: Vec<Pos2> = (0..=16)
                .map(|step| {
                    let angle = start + sweep * (step as f32) / 16.0;
                    center + Vec2::angled(angle) * radius
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            let angle = start + sweep;
            let tip = center + Vec2::angled(angle) * radius;
            let direction = Vec2::angled(angle);
            let tangent = Vec2::new(-direction.y, direction.x);
            painter.line_segment(
                [tip, tip - tangent * (size * 0.2) + direction * (size * 0.1)],
                stroke,
            );
            painter.line_segment(
                [tip, tip - tangent * (size * 0.2) - direction * (size * 0.1)],
                stroke,
            );
        }
        Icon::History => {
            painter.circle_stroke(center, size * 0.42, stroke);
            // Clock hands: one to the top, one to the right.
            painter.line_segment([center, center + vec2(0.0, -size * 0.28)], stroke);
            painter.line_segment([center, center + vec2(size * 0.22, size * 0.08)], stroke);
            painter.circle_filled(center, size * 0.06, color);
        }
        Icon::Mixer => {
            for (index, offset) in [-0.28_f32, 0.0, 0.28].into_iter().enumerate() {
                let x = center.x + offset * size;
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + size * 0.12),
                        egui::pos2(x, rect.bottom() - size * 0.12),
                    ],
                    stroke,
                );
                let knob_y = match index {
                    0 => rect.top() + size * 0.34,
                    1 => rect.top() + size * 0.62,
                    _ => rect.top() + size * 0.46,
                };
                painter.rect_filled(
                    Rect::from_center_size(egui::pos2(x, knob_y), vec2(size * 0.24, size * 0.1)),
                    1.0,
                    color,
                );
            }
        }
        Icon::Grid => {
            let cell = size * 0.34;
            for dx in [-1.0_f32, 1.0] {
                for dy in [-1.0_f32, 1.0] {
                    painter.rect_stroke(
                        Rect::from_center_size(
                            center + vec2(dx * cell * 0.62, dy * cell * 0.62),
                            vec2(cell, cell),
                        ),
                        1.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        Icon::Open => {
            // A folder: its body and a tab along the top left.
            let body = Rect::from_min_max(
                egui::pos2(rect.left() + size * 0.06, center.y - size * 0.12),
                egui::pos2(rect.right() - size * 0.06, rect.bottom() - size * 0.08),
            );
            painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            let tab_top = body.top() - size * 0.14;
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(body.left(), body.top()),
                    egui::pos2(body.left(), tab_top + size * 0.06),
                    egui::pos2(body.left() + size * 0.08, tab_top),
                    egui::pos2(body.left() + size * 0.4, tab_top),
                    egui::pos2(body.left() + size * 0.48, body.top()),
                ],
                stroke,
            ));
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
        Icon::WindowMinimize => {
            let y = center.y + size * 0.1;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(1.0, color),
            );
        }
        Icon::WindowMaximize => {
            painter.rect_stroke(
                rect.shrink(0.5),
                0.0,
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
        Icon::WindowRestore => {
            let back = Rect::from_min_max(
                rect.min + vec2(size * 0.25, 0.0),
                rect.max - vec2(0.0, size * 0.25),
            );
            let front = Rect::from_min_max(
                rect.min + vec2(0.0, size * 0.25),
                rect.max - vec2(size * 0.25, 0.0),
            );
            painter.rect_stroke(
                back.shrink(0.5),
                0.0,
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(front, 0.0, Color32::TRANSPARENT);
            painter.rect_stroke(
                front.shrink(0.5),
                0.0,
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
        Icon::WindowClose => {
            let thin = Stroke::new(1.2, color);
            painter.line_segment([rect.left_top(), rect.right_bottom()], thin);
            painter.line_segment([rect.right_top(), rect.left_bottom()], thin);
        }
        Icon::PanelLeft | Icon::PanelBottom | Icon::PanelRight => {
            let frame = Rect::from_center_size(center, vec2(size, size * 0.8));
            painter.rect_stroke(
                frame,
                1.5,
                Stroke::new(1.2, color),
                egui::StrokeKind::Inside,
            );
            let part = match icon {
                Icon::PanelLeft => Rect::from_min_max(
                    frame.min,
                    egui::pos2(frame.left() + frame.width() * 0.32, frame.bottom()),
                ),
                Icon::PanelRight => Rect::from_min_max(
                    egui::pos2(frame.right() - frame.width() * 0.32, frame.top()),
                    frame.max,
                ),
                _ => Rect::from_min_max(
                    egui::pos2(frame.left(), frame.bottom() - frame.height() * 0.38),
                    frame.max,
                ),
            };
            painter.rect_filled(part, 1.5, color);
        }
    }
}

/// A metronome body with its pendulum, centered on `center` at `size`.
fn paint_metronome(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let bottom = center.y + size * 0.42;
    let top = center.y - size * 0.36;
    let half_bottom = size * 0.34;
    let half_top = size * 0.18;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(center.x - half_bottom, bottom),
            egui::pos2(center.x - half_top, top),
            egui::pos2(center.x + half_top, top),
            egui::pos2(center.x + half_bottom, bottom),
        ],
        Color32::TRANSPARENT,
        stroke,
    ));
    painter.line_segment(
        [
            egui::pos2(center.x, bottom - size * 0.04),
            egui::pos2(center.x - size * 0.2, top + size * 0.18),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center.x - size * 0.24, top + size * 0.14),
            egui::pos2(center.x - size * 0.12, top + size * 0.26),
        ],
        Stroke::new(2.5, color),
    );
}

/// A filled triangle centered on `center`; `right` points it right, else left.
fn paint_triangle(painter: &Painter, center: Pos2, half: f32, right: bool, color: Color32) {
    let direction = if right { 1.0 } else { -1.0 };
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + vec2(-direction * half, -half),
            center + vec2(direction * half, 0.0),
            center + vec2(-direction * half, half),
        ],
        color,
        Stroke::NONE,
    ));
}
