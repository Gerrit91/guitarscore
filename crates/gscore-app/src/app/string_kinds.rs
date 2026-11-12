//! The Sound chain's Kind tab: which strings play the track, as four tiles (nylon, steel and
//! electric guitar, bass), each an icon with its name. Bare on the panel, filled when hovered
//! or chosen, as the palette's tiles.
//!
//! The icons are generic, not instruments (the owner found guitar pictures too busy): a few
//! strings lying across, flat in one colour, showing what tells the kinds apart. Nylon's
//! are smooth and plain, steel's thinner with the lower ones wound, the electric's cross a
//! pickup with its pole pieces, the bass has two heavy wound strings.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, pos2, vec2};
use gscore_play::StringKind;

use super::style;

/// One tile (width, height), and the icon inside it.
const TILE: (f32, f32) = (100.0, 70.0);
const PICTURE: f32 = 36.0;

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
            let (rect, response) = ui.allocate_exact_size(vec2(TILE.0, TILE.1), Sense::click());
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
                rect.center_top() + vec2(0.0, 8.0 + PICTURE / 2.0),
                vec2(PICTURE, PICTURE),
            );
            paint(ui.painter(), picture, kind, ink, paper);
            ui.painter().text(
                rect.center_bottom() - vec2(0.0, 6.0),
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

/// Draws in an icon's own coordinates: -0.5 to 0.5 across and down, y down.
struct Draw<'a> {
    painter: &'a Painter,
    center: Pos2,
    size: f32,
    ink: Color32,
    paper: Color32,
}

impl Draw<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.center.x + x * self.size, self.center.y + y * self.size)
    }

    /// A straight line, `width` thick (at least a crisp pixel), in `color`.
    fn line(&self, from: (f32, f32), to: (f32, f32), width: f32, color: Color32) {
        self.painter.line_segment(
            [self.at(from.0, from.1), self.at(to.0, to.1)],
            Stroke::new((width * self.size).max(1.0), color),
        );
    }

    /// A rounded rectangle from `(x0, y0)` to `(x1, y1)`.
    fn block(&self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color32) {
        self.painter.rect_filled(
            Rect::from_min_max(self.at(x0, y0), self.at(x1, y1)),
            0.08 * self.size,
            color,
        );
    }

    /// A string across the icon at `y`, `width` thick; a wound one shows its winding as
    /// fine slanted cuts.
    fn string(&self, y: f32, width: f32, wound: bool) {
        self.line((-0.45, y), (0.45, y), width, self.ink);
        if wound && width * self.size >= 3.0 {
            let half = width / 2.0;
            let mut x = -0.42;
            while x < 0.42 {
                self.line(
                    (x, y + half * 0.8),
                    (x + half, y - half * 0.8),
                    0.018,
                    self.paper,
                );
                x += 0.07;
            }
        }
    }

    fn dot(&self, x: f32, y: f32, radius: f32, color: Color32) {
        self.painter
            .circle_filled(self.at(x, y), (radius * self.size).max(1.0), color);
    }
}

/// Draws `kind`'s icon in `rect` in `ink`, its cut-outs in `paper`.
fn paint(painter: &Painter, rect: Rect, kind: StringKind, ink: Color32, paper: Color32) {
    let d = Draw {
        painter,
        center: rect.center(),
        size: rect.width().min(rect.height()),
        ink,
        paper,
    };
    match kind {
        // Plain nylon: smooth, fairly thick, alike.
        StringKind::Nylon => {
            for y in [-0.25, 0.0, 0.25] {
                d.string(y, 0.1, false);
            }
        }
        // Steel: thinner, heavier downwards, the lower two wound.
        StringKind::Steel => {
            d.string(-0.25, 0.045, false);
            d.string(0.0, 0.08, true);
            d.string(0.25, 0.11, true);
        }
        // Electric: thin strings over a pickup and its pole pieces.
        StringKind::Electric => {
            d.block(-0.15, -0.42, 0.15, 0.42, ink);
            for y in [-0.25, 0.0, 0.25] {
                d.string(y, 0.045, false);
                d.dot(0.0, y, 0.055, paper);
            }
        }
        // Bass: two heavy wound strings.
        StringKind::Bass => {
            d.string(-0.17, 0.15, true);
            d.string(0.17, 0.17, true);
        }
    }
}
