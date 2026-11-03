//! Palette tiles: square buttons that show a music-font glyph, a drawn sign or a short text,
//! highlighted in the accent color when chosen (like the note-input tiles of score editors).

use egui::{Align2, Color32, FontId, Response, Sense, vec2};

use super::style;
use super::tile_faces::paint_face;
pub(crate) use super::tile_faces::{Face, music_glyph};

/// The size of a tile.
pub(crate) const TILE: egui::Vec2 = egui::Vec2::new(27.0, 32.0);

/// One tile. `label` is what screen readers (and tests) call it; `tip` the tooltip.
pub(crate) fn tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(TILE, sense);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, label));
    let tokens = style::tokens(ui);
    // Like a printed palette: the sign alone on the panel, a fill only when hovered or
    // chosen.
    let (fill, ink) = if selected {
        (tokens.accent, tokens.on_accent)
    } else if enabled && response.hovered() {
        (tokens.control_hover, tokens.text)
    } else {
        (Color32::TRANSPARENT, tokens.text)
    };
    let ink = if enabled {
        ink
    } else {
        tokens.text_weak.gamma_multiply(0.6)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    paint_face(painter, rect, face, ink);
    response.on_hover_text(tip)
}

/// A row of segment buttons ("Voice 1 | Voice 2"): the chosen one in the accent color.
/// Returns the clicked index.
pub(crate) fn segments(
    ui: &mut egui::Ui,
    items: &[(&str, &str)],
    chosen: usize,
    tip: &str,
) -> Option<usize> {
    let tokens = style::tokens(ui);
    let mut clicked = None;
    egui::Frame::new()
        .fill(tokens.control)
        .corner_radius(5)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (index, (text, label)) in items.iter().enumerate() {
                    let on = index == chosen;
                    let galley_width = 12.0 + 7.0 * text.chars().count() as f32;
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(galley_width.max(36.0), 22.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, *label)
                    });
                    let fill = if on {
                        tokens.accent
                    } else if response.hovered() {
                        tokens.control_hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *text,
                        FontId::proportional(12.0),
                        if on { tokens.on_accent } else { tokens.text },
                    );
                    if response.on_hover_text(tip).clicked() {
                        clicked = Some(index);
                    }
                }
            });
        });
    clicked
}

/// Like [`segments`], with a tooltip of its own for each item: `(text, label, tip)`.
pub(crate) fn segments_with_tips(
    ui: &mut egui::Ui,
    items: &[(&str, &str, &str)],
    chosen: usize,
) -> Option<usize> {
    let tokens = style::tokens(ui);
    let mut clicked = None;
    egui::Frame::new()
        .fill(tokens.control)
        .corner_radius(5)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (index, (text, label, tip)) in items.iter().enumerate() {
                    let on = index == chosen;
                    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 22.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, *label)
                    });
                    let fill = if on {
                        tokens.accent
                    } else if response.hovered() {
                        tokens.control_hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *text,
                        FontId::proportional(12.0),
                        if on { tokens.on_accent } else { tokens.text },
                    );
                    if response.on_hover_text(*tip).clicked() {
                        clicked = Some(index);
                    }
                }
            });
        });
    clicked
}
