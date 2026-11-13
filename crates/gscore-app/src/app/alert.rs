//! Questions the app asks in a modal window (unsaved changes, recovery after a crash,
//! what a format cannot keep, updates): the app's own title bar with its close button, a large
//! icon, a headline, a few lines of text and the answers, the main one in the accent
//! color.

use egui::{Align, Color32, Layout, Margin, Painter, Pos2, Rect, RichText, Shape, Stroke, vec2};

use super::style;

/// The picture beside the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AlertIcon {
    /// A warning triangle with an exclamation mark: something may be lost.
    Warning,
    /// A circular arrow round a clock: work can be brought back.
    Restore,
    /// An arrow down into a tray, in a ring: a new version.
    Update,
}

/// One answer: its text and whether it is the main one (accent color, Enter).
#[derive(Debug, Clone, Copy)]
pub(super) struct Answer<'a> {
    pub(super) text: &'a str,
    pub(super) main: bool,
}

/// What happened to the question this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reply {
    /// Still open.
    Open,
    /// The answer at this index was chosen.
    Answer(usize),
    /// Closed with the title bar's button or Escape.
    Closed,
}

/// Width of the window.
const WIDTH: f32 = 460.0;
/// Size of the icon.
const ICON: f32 = 56.0;
/// The warning sign's amber.
const AMBER: Color32 = Color32::from_rgb(0xF2, 0xB2, 0x3E);

/// Shows the question as a modal window. `body` draws the text under the headline.
pub(super) fn alert(
    ctx: &egui::Context,
    title: &str,
    icon: AlertIcon,
    headline: &str,
    body: impl FnOnce(&mut egui::Ui),
    answers: &[Answer<'_>],
) -> Reply {
    let mut reply = Reply::Open;
    let modal =
        egui::Modal::new(egui::Id::unique(("gscore-alert", title))).frame(style::window_frame(ctx));
    modal.show(ctx, |ui| {
        ui.set_width(WIDTH);
        // The modal stays centered: the title bar does not move it.
        let mut no_drag = None;
        if style::window_title_bar(ui, title, &mut no_drag) {
            reply = Reply::Closed;
        }
        egui::Frame::new()
            .inner_margin(Margin::symmetric(20, 16))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(ICON, ICON), egui::Sense::hover());
                    paint_icon(ui.painter(), rect, icon, style::tokens(ui).accent);
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.set_width(WIDTH - ICON - 14.0 - 40.0);
                        ui.label(RichText::new(headline).strong().size(16.0));
                        ui.add_space(4.0);
                        body(ui);
                    });
                });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 30.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        let tokens = style::tokens(ui);
                        // Right to left: the last answer is drawn first, at the right edge.
                        for (index, answer) in answers.iter().enumerate().rev() {
                            let button = if answer.main {
                                egui::Button::new(
                                    RichText::new(answer.text).color(tokens.on_accent),
                                )
                                .fill(tokens.accent)
                            } else {
                                egui::Button::new(answer.text)
                            };
                            if ui.add(button.min_size(vec2(96.0, 28.0))).clicked() {
                                reply = Reply::Answer(index);
                            }
                        }
                    },
                );
            });
        if reply == Reply::Open {
            ui.input(|input| {
                if input.key_pressed(egui::Key::Escape) {
                    reply = Reply::Closed;
                } else if input.key_pressed(egui::Key::Enter)
                    && let Some(main) = answers.iter().position(|answer| answer.main)
                {
                    reply = Reply::Answer(main);
                }
            });
        }
    });
    reply
}

/// Paints `icon` into `rect`; the restore icon is drawn in `accent`.
fn paint_icon(painter: &Painter, rect: Rect, icon: AlertIcon, accent: Color32) {
    match icon {
        AlertIcon::Warning => paint_warning(painter, rect),
        AlertIcon::Restore => paint_restore(painter, rect, accent),
        AlertIcon::Update => paint_update(painter, rect, accent),
    }
}

/// An amber triangle with rounded corners and a dark exclamation mark.
fn paint_warning(painter: &Painter, rect: Rect) {
    let top = Pos2::new(rect.center().x, rect.top() + 3.0);
    let left = Pos2::new(rect.left() + 1.0, rect.bottom() - 4.0);
    let right = Pos2::new(rect.right() - 1.0, rect.bottom() - 4.0);
    // A thick outline in the same color rounds the corners.
    painter.add(Shape::convex_polygon(
        vec![top, right, left],
        AMBER,
        Stroke::new(5.0, AMBER),
    ));
    for (a, b) in [(top, right), (right, left), (left, top)] {
        painter.line_segment([a, b], Stroke::new(5.0, AMBER));
    }
    for corner in [top, left, right] {
        painter.circle_filled(corner, 2.5, AMBER);
    }
    let ink = Color32::from_rgb(0x2A, 0x22, 0x10);
    let x = rect.center().x;
    let bar_top = rect.top() + rect.height() * 0.32;
    let bar_bottom = rect.top() + rect.height() * 0.66;
    painter.line_segment(
        [Pos2::new(x, bar_top), Pos2::new(x, bar_bottom)],
        Stroke::new(5.5, ink),
    );
    painter.circle_filled(Pos2::new(x, bar_top), 2.75, ink);
    painter.circle_filled(Pos2::new(x, bar_bottom), 2.75, ink);
    painter.circle_filled(Pos2::new(x, rect.top() + rect.height() * 0.8), 3.4, ink);
}

/// A circular arrow (counter-clockwise, like undo) round a clock face.
fn paint_restore(painter: &Painter, rect: Rect, accent: Color32) {
    let center = rect.center();
    let radius = rect.width() * 0.4;
    let stroke = Stroke::new(4.5, accent);
    // The arc leaves a gap at the upper left, where the arrowhead points.
    let start = -0.15 * std::f32::consts::PI;
    let sweep = 1.6 * std::f32::consts::PI;
    let point = |angle: f32| center + radius * vec2(angle.cos(), angle.sin());
    let points: Vec<Pos2> = (0..=40)
        .map(|step| point(start - sweep * step as f32 / 40.0))
        .collect();
    painter.add(Shape::line(points, stroke));
    // The arrowhead at the arc's end, pointing along it (counter-clockwise).
    let end_angle = start - sweep;
    let tip = point(end_angle);
    let along = vec2(end_angle.sin(), -end_angle.cos());
    let out = vec2(end_angle.cos(), end_angle.sin());
    painter.add(Shape::convex_polygon(
        vec![
            tip + along * 9.0,
            tip - out * 7.5 - along * 2.0,
            tip + out * 7.5 - along * 2.0,
        ],
        accent,
        Stroke::NONE,
    ));
    // Clock hands.
    let hands = Stroke::new(3.5, accent);
    painter.line_segment([center, center - vec2(0.0, radius * 0.55)], hands);
    painter.line_segment([center, center + vec2(radius * 0.4, radius * 0.2)], hands);
    painter.circle_filled(center, 3.0, accent);
}

/// A ring with an arrow pointing down onto a short tray: a download.
fn paint_update(painter: &Painter, rect: Rect, accent: Color32) {
    let center = rect.center();
    let radius = rect.width() * 0.4;
    painter.circle_stroke(center, radius, Stroke::new(4.5, accent));
    let stroke = Stroke::new(4.5, accent);
    let top = center - vec2(0.0, radius * 0.55);
    let tip = center + vec2(0.0, radius * 0.3);
    painter.line_segment([top, tip - vec2(0.0, 4.0)], stroke);
    painter.add(Shape::convex_polygon(
        vec![
            tip + vec2(0.0, 3.0),
            tip + vec2(-radius * 0.32, -radius * 0.3),
            tip + vec2(radius * 0.32, -radius * 0.3),
        ],
        accent,
        Stroke::NONE,
    ));
    let tray = center.y + radius * 0.55;
    painter.line_segment(
        [
            Pos2::new(center.x - radius * 0.4, tray),
            Pos2::new(center.x + radius * 0.4, tray),
        ],
        Stroke::new(3.5, accent),
    );
}
