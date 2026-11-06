//! Palette tiles: square buttons that show a music-font glyph, a drawn sign or a short text,
//! highlighted in the accent color when chosen (like the note-input tiles of score editors).

use egui::{Align2, Color32, FontId, Response, Sense, vec2};

use super::style;
pub(crate) use super::tile_faces::{Face, music_glyph};
use super::tile_faces::{more_corner, paint_face};

/// The size of a tile.
pub(crate) const TILE: egui::Vec2 = egui::Vec2::new(27.0, 32.0);

/// A tile a little larger than the palette's, for a small window of choices.
pub(crate) fn large_tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    sized_tile(
        ui,
        vec2(32.0, 34.0),
        face,
        selected,
        enabled,
        label,
        tip,
        false,
    )
}

/// One tile. `label` is what screen readers (and tests) call it; `tip` the tooltip.
pub(crate) fn tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    sized_tile(ui, TILE, face, selected, enabled, label, tip, false)
}

/// A [`tile`] that opens a dialog: a small corner triangle says so, like the technique
/// tiles with their own editor.
pub(crate) fn dialog_tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    sized_tile(ui, TILE, face, selected, enabled, label, tip, true)
}

/// A [`tile`] wider than the others, for signs that need the room (tuplet brackets).
pub(crate) fn wide_tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    let size = vec2(TILE.x * 1.5, TILE.y);
    sized_tile(ui, size, face, selected, enabled, label, tip, false)
}

#[allow(clippy::too_many_arguments)]
fn sized_tile(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
    more: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
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
    // A chosen tile that cannot be switched off (the last staff left) still reads as on.
    let ink = if enabled || selected {
        ink
    } else {
        tokens.text_weak.gamma_multiply(0.6)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    paint_face(painter, rect, face, ink);
    if more {
        more_corner(painter, rect, ink);
    }
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
