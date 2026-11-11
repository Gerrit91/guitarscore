//! The Sound chain's Kind tab: which strings play the track, as four large pictures (nylon,
//! steel and electric guitar, bass) with their names. Bare on the panel, filled when hovered
//! or chosen, as the palette's tiles.

use egui::{Align2, FontId, Rect, Sense, vec2};
use gscore_play::StringKind;

use super::style;

/// One tile, and the picture inside it.
const TILE: f32 = 124.0;
const PICTURE: f32 = 92.0;

/// The kinds in the order shown, with their names.
const KINDS: [(StringKind, &str); 4] = [
    (StringKind::Nylon, "Nylon guitar"),
    (StringKind::Steel, "Steel guitar"),
    (StringKind::Electric, "Electric guitar"),
    (StringKind::Bass, "Bass"),
];

/// What each kind is, for its tooltip.
fn tip(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Nylon => "Nylon strings on a classical guitar: soft, warm and round",
        StringKind::Steel => "Steel strings on an acoustic dreadnought: bright and full",
        StringKind::Electric => "An electric guitar, heard through its magnetic pickup",
        StringKind::Bass => "An electric bass: long, round and deep",
    }
}

/// The tiles; the kind clicked, if another than `chosen`.
pub(super) fn kind_tiles(ui: &mut egui::Ui, chosen: StringKind) -> Option<StringKind> {
    let tokens = style::tokens(ui);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        for (kind, name) in KINDS {
            let (rect, response) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click());
            let selected = kind == chosen;
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(name);
                node.set_role(egui::Role::Button);
                node.set_selected(selected);
            });
            let (fill, ink) = if selected {
                (Some(tokens.accent), tokens.on_accent)
            } else if response.hovered() {
                (Some(tokens.control_hover), tokens.text)
            } else {
                (None, tokens.text)
            };
            if let Some(fill) = fill {
                ui.painter().rect_filled(rect, 6.0, fill);
            }
            let paper = fill.unwrap_or(ui.visuals().window_fill);
            let picture = Rect::from_center_size(
                rect.center_top() + vec2(0.0, 4.0 + PICTURE / 2.0),
                vec2(PICTURE, PICTURE),
            );
            super::instrument::paint_strings(ui.painter(), picture, kind, ink, paper);
            ui.painter().text(
                rect.center_bottom() - vec2(0.0, 7.0),
                Align2::CENTER_BOTTOM,
                name,
                FontId::proportional(12.0),
                ink,
            );
            if response.on_hover_text(tip(kind)).clicked() && !selected {
                picked = Some(kind);
            }
        }
    });
    picked
}
