//! Widgets of the staff dialogs: choice rows (the engraved sign, its name and what it
//! does, chosen with a click), the feel notation, and a large engraved preview.

use egui::{Align2, Color32, FontFamily, FontId, Painter, Rect, Response, Sense, Stroke, vec2};

use super::style;
use super::tile_faces::music_glyph;
use crate::fonts::BOLD;
use gscore_model::Feel;

/// Height of a choice row.
const ROW_HEIGHT: f32 = 46.0;
/// Width of the sign at the left of a row.
const SIGN_WIDTH: f32 = 72.0;

/// One choice: the sign drawn by `paint` into its box, the choice's `name` (also what
/// screen readers and tests call it) and a line on what it does. Returns the row's
/// response; the caller selects on a click.
pub(super) fn choice(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    detail: &str,
    paint: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    choice_with_sign(ui, selected, name, detail, SIGN_WIDTH, paint)
}

/// A [`choice`] whose sign takes `sign_width` (for wide notation such as a feel).
pub(super) fn choice_with_sign(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    detail: &str,
    sign_width: f32,
    paint: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    let tokens = style::tokens(ui);
    let painter = ui.painter();
    let fill = if selected {
        tokens.accent.gamma_multiply(0.28)
    } else if response.hovered() {
        tokens.control_hover
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 6.0, fill);
    if selected {
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.5, tokens.accent),
            egui::StrokeKind::Inside,
        );
    }
    let sign = Rect::from_min_size(rect.min, vec2(sign_width, ROW_HEIGHT));
    paint(painter, sign, tokens.text);
    let text_x = sign.right() + 6.0;
    painter.text(
        egui::pos2(text_x, rect.top() + 8.0),
        Align2::LEFT_TOP,
        name,
        FontId::new(13.5, FontFamily::Name(BOLD.into())),
        tokens.text,
    );
    let detail_font = FontId::proportional(11.5);
    let galley = painter.layout(
        detail.to_owned(),
        detail_font,
        tokens.text_weak,
        rect.right() - text_x - 8.0,
    );
    painter.galley(
        egui::pos2(text_x, rect.top() + 26.0),
        galley,
        tokens.text_weak,
    );
    response
}

/// Paints a glyph string centered in `rect` with its baseline `baseline` below the center.
pub(super) fn glyph_in(
    painter: &Painter,
    rect: Rect,
    text: &str,
    size: f32,
    baseline: f32,
    ink: Color32,
) {
    music_glyph(
        painter,
        rect.center() + vec2(0.0, baseline),
        text,
        size,
        ink,
    );
}

/// The written pair of a feel: two beamed eighths or sixteenths (Bravura's text notes).
fn written_pair(feel: Feel) -> &'static str {
    if feel.is_sixteenth() {
        "\u{E1F1}\u{E1FA}\u{E1F5}"
    } else {
        "\u{E1F1}\u{E1F8}\u{E1F3}"
    }
}

/// How the feel plays the pair, in Bravura's text notes (empty for straight).
fn played_pair(feel: Feel) -> &'static str {
    match feel {
        Feel::None => "",
        Feel::Eighth => "\u{E201}\u{E1F1}\u{E202}\u{E1F3}\u{E203}",
        Feel::Sixteenth => "\u{E201}\u{E1F1}\u{E1F8}\u{E202}\u{E1F5}\u{E203}",
        Feel::DottedEighth => "\u{E1F1}\u{E1FC}\u{E1F8}\u{E1F5}",
        Feel::DottedSixteenth => "\u{E1F1}\u{E1FC}\u{E1FA}\u{E1F6}",
        Feel::ScottishEighth => "\u{ECA9}\u{ECA7}\u{ECB7}",
        Feel::ScottishSixteenth => "\u{ECAB}\u{ECA9}\u{ECB7}",
    }
}

/// Paints the feel's notation into `rect`: "♫" alone for straight, else "♫ = ⌐3¬♩♪".
pub(super) fn paint_feel(painter: &Painter, rect: Rect, feel: Feel, size: f32, ink: Color32) {
    let baseline = rect.center().y + size * 0.3;
    let written = written_pair(feel);
    let played = played_pair(feel);
    if played.is_empty() {
        music_glyph(
            painter,
            egui::pos2(rect.center().x, baseline),
            written,
            size,
            ink,
        );
        return;
    }
    let quarter = rect.width() / 4.0;
    music_glyph(
        painter,
        egui::pos2(rect.left() + quarter * 0.9, baseline),
        written,
        size,
        ink,
    );
    painter.text(
        egui::pos2(rect.center().x - quarter * 0.1, baseline - size * 0.22),
        Align2::CENTER_CENTER,
        "=",
        FontId::new(size * 0.55, FontFamily::Name(BOLD.into())),
        ink,
    );
    // The flagged metronome notes stand lower than the text notes; lift them to match.
    let lift = if matches!(feel, Feel::ScottishEighth | Feel::ScottishSixteenth) {
        -size * 0.08
    } else {
        0.0
    };
    music_glyph(
        painter,
        egui::pos2(rect.right() - quarter * 1.1, baseline + lift),
        played,
        if lift == 0.0 { size } else { size * 1.15 },
        ink,
    );
}

/// What a feel does, in a line.
pub(super) fn feel_detail(feel: Feel) -> &'static str {
    match feel {
        Feel::None => "Even eighths and sixteenths, as written.",
        Feel::Eighth => "Eighth pairs swing long-short, 2:1 (jazz, blues, shuffle).",
        Feel::Sixteenth => "Sixteenth pairs swing 2:1 (funk, hip-hop grooves).",
        Feel::DottedEighth => "Eighth pairs play dotted, 3:1: a hard swing.",
        Feel::DottedSixteenth => "Sixteenth pairs play dotted, 3:1.",
        Feel::ScottishEighth => "Eighth pairs snap short-long, 1:3 (Scotch snap).",
        Feel::ScottishSixteenth => "Sixteenth pairs snap short-long, 1:3.",
    }
}

/// A big stepper: "−", the value painted by `paint`, "+". Returns the change (-1, 0, 1).
pub(super) fn stepper(ui: &mut egui::Ui, label: &str, height: f32) -> (Rect, i32) {
    let mut step = 0;
    let tokens = style::tokens(ui);
    let mut middle = Rect::NOTHING;
    ui.horizontal(|ui| {
        let button = |ui: &mut egui::Ui, text: &str, what: &str| {
            let response = ui.add(
                egui::Button::new(egui::RichText::new(text).size(16.0))
                    .min_size(vec2(28.0, height))
                    .fill(tokens.control),
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, what));
            response.clicked()
        };
        if button(ui, "−", &format!("Fewer: {label}")) {
            step = -1;
        }
        let (rect, _) = ui.allocate_exact_size(vec2(64.0, height), Sense::hover());
        middle = rect;
        if button(ui, "+", &format!("More: {label}")) {
            step = 1;
        }
    });
    (middle, step)
}
