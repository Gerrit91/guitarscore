//! A small table for the side panels: one dark, rounded surface with rows on it, each row
//! a name at the left and optionally a detail at its right, as the track's tone presets and
//! the song's sections show them. Rows highlight under the pointer, the chosen one stays
//! highlighted with an accent bar at its left edge, and a soft line separates every row.

use egui::{Align2, Color32, FontId, Rect, Response, ScrollArea, Sense, Stroke, pos2, vec2};

use super::style;

/// How many rows a table shows at once before it scrolls.
pub(super) const VISIBLE_ROWS: usize = 8;

/// One row of a table.
pub(super) struct Row<'a> {
    /// The row's name, at the left.
    pub label: &'a str,
    /// More text at the right, in the weak color (a section's bars).
    pub detail: Option<&'a str>,
    /// A strip of this color at the row's left (a section's color).
    pub swatch: Option<Color32>,
    /// Whether this is the current choice.
    pub selected: bool,
    /// Whether a row can be dragged (the song's sections, which swap when one is dropped
    /// on another).
    pub draggable: bool,
    /// The accessible name, when the text alone does not say what a click does.
    pub accessible: &'a str,
    /// The hover text.
    pub hint: &'a str,
}

/// The height of a table's rows.
pub(super) fn row_height(ui: &egui::Ui) -> f32 {
    ui.spacing().interact_size.y.max(18.0)
}

/// The table's surface: one dark, rounded box whose rows scroll inside, about `visible` rows
/// tall. `id_salt` keeps its scroll position apart from other tables'. Returns what
/// `add_rows` returns.
pub(super) fn surface<R>(
    ui: &mut egui::Ui,
    id_salt: &str,
    visible: usize,
    add_rows: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let height = row_height(ui) * visible.max(1) as f32;
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(3.0)
        .inner_margin(2)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt(id_salt)
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, add_rows)
                .inner
        })
        .inner
}

/// One row of the table: its background (when chosen or hovered), an accent bar when it is
/// chosen, its color strip, its name at the left (clipped before the detail) and the detail
/// at the right in the weak color, with a soft line under it. Returns its rect (to scroll it
/// into view) and its response.
pub(super) fn row(ui: &mut egui::Ui, row: &Row<'_>) -> (Rect, Response) {
    let tokens = style::tokens(ui);
    let sense = if row.draggable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), row_height(ui)), sense);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::Role::Button);
        node.set_label(row.accessible);
    });
    let fill = if row.selected {
        tokens.accent.gamma_multiply(0.35)
    } else if response.hovered() {
        tokens.control_hover
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 3.0, fill);
    }
    if row.selected {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
            1.0,
            tokens.accent,
        );
    }
    let mut left = rect.left() + 7.0;
    if let Some(color) = row.swatch {
        ui.painter().rect_filled(
            Rect::from_min_size(
                rect.min + vec2(2.0, 3.0),
                vec2(3.0, (rect.height() - 6.0).max(1.0)),
            ),
            1.0,
            color,
        );
        left = rect.left() + 9.0;
    }
    let mut right = rect.right() - 7.0;
    if let Some(detail) = row.detail {
        let font = egui::TextStyle::Small.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(detail.to_owned(), font, tokens.text_weak);
        let width = galley.rect.width();
        ui.painter().galley(
            pos2(right - width, rect.center().y - galley.rect.height() / 2.0),
            galley,
            tokens.text_weak,
        );
        right = (right - width - 6.0).max(left + 1.0);
    }
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            pos2(left, rect.top()),
            pos2(right, rect.bottom()),
        ))
        .text(
            pos2(left, rect.center().y),
            Align2::LEFT_CENTER,
            row.label,
            FontId::proportional(12.0),
            tokens.text,
        );
    // A soft line under the row, as between the tone presets.
    let line = rect.bottom() - 0.5;
    ui.painter().line_segment(
        [
            pos2(rect.left() + 7.0, line),
            pos2(rect.right() - 7.0, line),
        ],
        Stroke::new(1.0, tokens.line.gamma_multiply(0.55)),
    );
    (rect, response.on_hover_text(row.hint))
}
