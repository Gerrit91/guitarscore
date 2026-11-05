//! Small monochrome icons drawn with the painter (GuitarScore's own work, no third-party
//! assets or fonts): toolbar actions, so the app does not depend on an icon set.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2, pos2, vec2};

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
    /// Loop (two arrows going round a circle).
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
    /// Rename (a pencil).
    Rename,
    /// A tuner (a tuning fork).
    Tuner,
}

/// The size of the compact icon buttons in the panels and the toolbar.
const BUTTON: Vec2 = vec2(26.0, 22.0);
/// How much the toolbar's icons are shrunk inside their button: less than the panels', so
/// the drawing is a little larger without making the toolbar wider (a wider toolbar pushes
/// the side groups into the player at narrow window sizes).
const TOOLBAR_SHRINK: f32 = 3.5;

/// A square icon button with a tooltip and an accessibility label.
pub(crate) fn button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> Response {
    sized_button(ui, icon, label, tip, BUTTON, 6.0)
}

/// A [`button`] for the toolbar, with a larger drawn icon but the same layout size.
pub(crate) fn toolbar_button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> Response {
    sized_button(ui, icon, label, tip, BUTTON, TOOLBAR_SHRINK)
}

fn sized_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tip: &str,
    size: Vec2,
    shrink: f32,
) -> Response {
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
        rect.shrink(shrink),
        visuals.fg_stroke.color,
        icon,
    );
    response.on_hover_text(tip)
}

/// An icon button that shows whether something is on (a panel shown): highlighted while on.
/// The panel toggles live in the toolbar, so they draw the larger icon.
pub(crate) fn toggle_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tip: &str,
    on: bool,
) -> Response {
    sized_toggle(ui, icon, label, tip, on, BUTTON, TOOLBAR_SHRINK)
}

fn sized_toggle(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tip: &str,
    on: bool,
    size: Vec2,
    shrink: f32,
) -> Response {
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
        rect.shrink(shrink),
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
            painter.add(Shape::convex_polygon(
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
            // A full-height metronome, then a small play sign: click a bar, then start.
            paint_metronome(
                painter,
                pos2(rect.left() + size * 0.45, center.y),
                size,
                color,
            );
            paint_triangle(
                painter,
                pos2(rect.right() - size * 0.16, center.y + size * 0.1),
                size * 0.17,
                true,
                color,
            );
        }
        Icon::Loop => {
            // Two arrows chasing each other round a circle (repeat), unlike the single
            // backwards arrow of Revert.
            let radius = size * 0.4;
            let deg = |d: f32| d.to_radians();
            paint_arc_arrow(painter, center, radius, deg(195.0), deg(325.0), size, color);
            paint_arc_arrow(painter, center, radius, deg(15.0), deg(145.0), size, color);
        }
        Icon::ToStart => {
            let x = rect.left() + size * 0.22;
            painter.line_segment(
                [
                    pos2(x, rect.top() + size * 0.1),
                    pos2(x, rect.bottom() - size * 0.1),
                ],
                stroke,
            );
            paint_triangle(
                painter,
                pos2(rect.left() + size * 0.6, center.y),
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
                pos2(rect.right() - size * 0.6, center.y),
                size * 0.3,
                true,
                color,
            );
            let x = rect.right() - size * 0.22;
            painter.line_segment(
                [
                    pos2(x, rect.top() + size * 0.1),
                    pos2(x, rect.bottom() - size * 0.1),
                ],
                stroke,
            );
        }
        Icon::Save => {
            // A floppy disk: a square with its top-right corner cut, the metal shutter at
            // the top and the label at the bottom. Square, whatever the button's shape.
            let body = Rect::from_center_size(center, Vec2::splat(size)).shrink(0.75);
            let notch = size * 0.22;
            painter.add(Shape::closed_line(
                vec![
                    body.left_top(),
                    pos2(body.right() - notch, body.top()),
                    pos2(body.right(), body.top() + notch),
                    body.right_bottom(),
                    body.left_bottom(),
                ],
                stroke,
            ));
            let shutter = Rect::from_min_max(
                pos2(body.left() + size * 0.24, body.top()),
                pos2(body.right() - size * 0.3, body.top() + size * 0.3),
            );
            painter.rect_filled(shutter, 0.0, color);
            let label = Rect::from_min_max(
                pos2(body.left() + size * 0.26, body.top() + size * 0.58),
                pos2(body.right() - size * 0.26, body.bottom() - size * 0.22),
            );
            painter.rect_filled(label, 0.5, color);
        }
        Icon::NewSong => {
            // A page with a folded corner and a plus on it.
            let page = Rect::from_center_size(center, vec2(size * 0.8, size)).shrink(0.75);
            let fold = size * 0.28;
            painter.add(Shape::closed_line(
                vec![
                    page.left_top(),
                    pos2(page.right() - fold, page.top()),
                    pos2(page.right(), page.top() + fold),
                    page.right_bottom(),
                    page.left_bottom(),
                ],
                stroke,
            ));
            let plus = pos2(page.center().x, page.center().y + size * 0.1);
            let arm = size * 0.2;
            painter.line_segment([plus - vec2(arm, 0.0), plus + vec2(arm, 0.0)], stroke);
            painter.line_segment([plus - vec2(0.0, arm), plus + vec2(0.0, arm)], stroke);
        }
        Icon::Revert => {
            // One arrow turning back (counter-clockwise), ending at the top left.
            let radius = size * 0.38;
            let deg = |d: f32| d.to_radians();
            paint_arc_arrow(
                painter,
                center,
                radius,
                deg(150.0),
                deg(-150.0),
                size,
                color,
            );
        }
        Icon::History => {
            // A counter-clockwise arrow round the clock hands (back in time), open at
            // the upper left where the arrowhead sits.
            let radius = size * 0.42;
            let start = -0.2 * std::f32::consts::PI;
            let sweep = 1.55 * std::f32::consts::PI;
            let point = |angle: f32| center + radius * vec2(angle.cos(), angle.sin());
            let arc: Vec<Pos2> = (0..=24)
                .map(|step| point(start - sweep * step as f32 / 24.0))
                .collect();
            painter.add(Shape::line(arc, stroke));
            let end = start - sweep;
            let tip = point(end);
            let along = vec2(end.sin(), -end.cos());
            let out = vec2(end.cos(), end.sin());
            painter.add(Shape::convex_polygon(
                vec![
                    tip + along * size * 0.2,
                    tip - out * size * 0.17 - along * size * 0.04,
                    tip + out * size * 0.17 - along * size * 0.04,
                ],
                color,
                Stroke::NONE,
            ));
            painter.line_segment([center, center + vec2(0.0, -size * 0.24)], stroke);
            painter.line_segment([center, center + vec2(size * 0.18, size * 0.1)], stroke);
        }
        Icon::Mixer => {
            for (index, offset) in [-0.28_f32, 0.0, 0.28].into_iter().enumerate() {
                let x = center.x + offset * size;
                painter.line_segment(
                    [
                        pos2(x, rect.top() + size * 0.12),
                        pos2(x, rect.bottom() - size * 0.12),
                    ],
                    stroke,
                );
                let knob_y = match index {
                    0 => rect.top() + size * 0.34,
                    1 => rect.top() + size * 0.62,
                    _ => rect.top() + size * 0.46,
                };
                painter.rect_filled(
                    Rect::from_center_size(pos2(x, knob_y), vec2(size * 0.24, size * 0.1)),
                    1.0,
                    color,
                );
            }
        }
        Icon::Grid => {
            // A 2x2 table: a square outline and a cross through it.
            let square = Rect::from_center_size(center, Vec2::splat(size * 0.9));
            painter.rect_stroke(square, 1.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    pos2(center.x, square.top()),
                    pos2(center.x, square.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(square.left(), center.y),
                    pos2(square.right(), center.y),
                ],
                stroke,
            );
        }
        Icon::Open => {
            // An open folder: the back with its tab, and the front flap tilted forward.
            let width = (size * 1.25).min(rect.width());
            let area = Rect::from_center_size(center, vec2(width, size));
            let at = |x: f32, y: f32| {
                pos2(
                    area.left() + area.width() * x,
                    area.top() + area.height() * y,
                )
            };
            painter.add(Shape::line(
                vec![
                    at(0.06, 0.9),
                    at(0.06, 0.1),
                    at(0.38, 0.1),
                    at(0.48, 0.24),
                    at(0.84, 0.24),
                    at(0.84, 0.44),
                ],
                stroke,
            ));
            painter.add(Shape::closed_line(
                vec![at(0.06, 0.9), at(0.24, 0.44), at(0.98, 0.44), at(0.8, 0.9)],
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
                arrow.center().y,
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
        Icon::Rename => {
            // A pencil: a thin body from the lower left to the upper right, and its tip.
            let tip_at = center + vec2(-size * 0.34, size * 0.34);
            let end = center + vec2(size * 0.32, -size * 0.34);
            let direction = (end - tip_at).normalized();
            let across = vec2(-direction.y, direction.x) * (size * 0.09);
            painter.line_segment([tip_at + across, end + across], stroke);
            painter.line_segment([tip_at - across, end - across], stroke);
            painter.line_segment([end - across, end + across], stroke);
            painter.add(Shape::convex_polygon(
                vec![
                    tip_at + across,
                    tip_at - across,
                    tip_at - direction * (size * 0.18),
                ],
                stroke.color,
                Stroke::NONE,
            ));
        }
        Icon::Tuner => {
            // A tuning fork: two prongs, the U where they meet, a stem and a base.
            let half = size * 0.24;
            let top = center.y - size * 0.44;
            let bend = center.y + size * 0.02;
            painter.line_segment(
                [pos2(center.x - half, top), pos2(center.x - half, bend)],
                stroke,
            );
            painter.line_segment(
                [pos2(center.x + half, top), pos2(center.x + half, bend)],
                stroke,
            );
            let u: Vec<_> = (0..=10)
                .map(|step| {
                    let t = std::f32::consts::PI * step as f32 / 10.0;
                    pos2(center.x - t.cos() * half, bend + t.sin() * half * 0.55)
                })
                .collect();
            painter.add(Shape::line(u, stroke));
            painter.line_segment(
                [
                    pos2(center.x, bend + half * 0.55),
                    pos2(center.x, center.y + size * 0.32),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x - size * 0.14, center.y + size * 0.32),
                    pos2(center.x + size * 0.14, center.y + size * 0.32),
                ],
                stroke,
            );
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
                [pos2(rect.left(), y), pos2(rect.right(), y)],
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
                    pos2(frame.left() + frame.width() * 0.32, frame.bottom()),
                ),
                Icon::PanelRight => Rect::from_min_max(
                    pos2(frame.right() - frame.width() * 0.32, frame.top()),
                    frame.max,
                ),
                _ => Rect::from_min_max(
                    pos2(frame.left(), frame.bottom() - frame.height() * 0.38),
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
    let bottom = center.y + size * 0.44;
    let top = center.y - size * 0.44;
    let half_bottom = size * 0.36;
    let half_top = size * 0.16;
    painter.add(Shape::closed_line(
        vec![
            pos2(center.x - half_bottom, bottom),
            pos2(center.x - half_top, top),
            pos2(center.x + half_top, top),
            pos2(center.x + half_bottom, bottom),
        ],
        stroke,
    ));
    // The pendulum leans right from the foot, with a small weight on it.
    let foot = pos2(center.x, bottom - size * 0.12);
    let end = pos2(center.x + size * 0.34, top + size * 0.08);
    painter.line_segment([foot, end], Stroke::new(1.2, color));
    let weight = foot + (end - foot) * 0.62;
    painter.circle_filled(weight, size * 0.1, color);
}

/// An arc around `center` from angle `from` to `to` (radians; increasing is clockwise on
/// screen) with a filled arrowhead at `to`, pointing the way the arc runs.
fn paint_arc_arrow(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    from: f32,
    to: f32,
    size: f32,
    color: Color32,
) {
    let sign = if to >= from { 1.0 } else { -1.0 };
    // The line stops short of the tip, so the head stays sharp.
    let head = size * 0.26;
    let line_end = to - sign * (head * 0.6 / radius);
    let steps = 14;
    let points: Vec<Pos2> = (0..=steps)
        .map(|step| {
            let angle = from + (line_end - from) * (step as f32) / (steps as f32);
            center + Vec2::angled(angle) * radius
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(1.5, color)));
    let radial = Vec2::angled(to);
    let tangent = Vec2::new(-radial.y, radial.x) * sign;
    let base = center + radial * radius - tangent * (head * 0.55);
    painter.add(Shape::convex_polygon(
        vec![
            base + tangent * head,
            base + radial * (head * 0.5),
            base - radial * (head * 0.5),
        ],
        color,
        Stroke::NONE,
    ));
}

/// A filled triangle centered on `center`; `right` points it right, else left.
fn paint_triangle(painter: &Painter, center: Pos2, half: f32, right: bool, color: Color32) {
    let direction = if right { 1.0 } else { -1.0 };
    painter.add(Shape::convex_polygon(
        vec![
            center + vec2(-direction * half, -half),
            center + vec2(direction * half, 0.0),
            center + vec2(-direction * half, half),
        ],
        color,
        Stroke::NONE,
    ));
}
