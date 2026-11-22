//! The drawer's touch-sized pieces: a tile (an icon, filled while it is on, a value in a
//! badge on its corner), the drawer button's chevron, the way back from a tile's page,
//! and the count-in's length as a row of choices.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, vec2};

use super::TARGET;
use crate::app::icons::{self, Icon};
use crate::app::style;

/// Room above a row of tiles for the badges that stick out of their corners.
pub(super) const BADGE_ROOM: f32 = 8.0;

/// One tile's look: switched on (filled with its colour, the icon in `ink`), and the
/// value in its badge.
#[derive(Default)]
pub(super) struct Tile {
    pub(super) on: Option<(Color32, Color32)>,
    pub(super) badge: Option<String>,
}

impl Tile {
    /// A switch: filled with the accent while `on`.
    pub(super) fn switch(ui: &egui::Ui, on: bool) -> Tile {
        let tokens = style::tokens(ui);
        Tile {
            on: on.then_some((tokens.accent, tokens.on_accent)),
            badge: None,
        }
    }

    /// The same tile with `badge` on its corner.
    pub(super) fn badge(self, badge: Option<String>) -> Tile {
        Tile { badge, ..self }
    }
}

/// One tile: its icon in the middle, filled while it is on, and its badge on the top
/// right corner.
pub(super) fn tile_button(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: Icon,
    label: &str,
    look: &Tile,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("practice_tile", label)),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, look.on.is_some(), label)
    });
    let tokens = style::tokens(ui);
    let ink = ui.style().interact(&response).fg_stroke.color;
    let (fill, ink) = match look.on {
        Some((fill, ink)) => (fill, ink),
        None if response.is_pointer_button_down_on() => (tokens.control_active, ink),
        None if response.hovered() => (tokens.control_hover, ink),
        None => (tokens.control, ink),
    };
    ui.painter().rect_filled(rect, 10.0, fill);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(20.0, 20.0));
    icons::paint(ui.painter(), icon_rect, ink, icon);
    if let Some(badge) = &look.badge {
        let font = FontId::new(11.0, FontFamily::Name(crate::fonts::BOLD.into()));
        let text = ui
            .painter()
            .layout_no_wrap(badge.clone(), font, tokens.on_accent);
        let size = vec2((text.size().x + 10.0).max(18.0), 18.0);
        let pill = Rect::from_min_size(
            Pos2::new(rect.right() - size.x + 4.0, rect.top() - BADGE_ROOM),
            size,
        );
        ui.painter().rect_filled(pill, size.y / 2.0, tokens.accent);
        ui.painter()
            .galley(pill.center() - text.size() / 2.0, text, tokens.on_accent);
    }
    response
}

/// The drawer's button in `rect`, at the end of the player's row: a chevron pointing up
/// while the drawer is shut (a tap or a swipe up opens it) and down while it is open (a
/// tap or a swipe down shuts it), filled while it is open. Returns whether the drawer
/// should be open now.
pub(super) fn handle(ui: &mut egui::Ui, rect: Rect, open: bool) -> bool {
    let response = ui.interact(
        rect,
        ui.scope_id().with("practice_handle"),
        Sense::click_and_drag(),
    );
    let label = if open {
        "Fewer controls"
    } else {
        "More controls"
    };
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, label));
    let mut now = open;
    if response.clicked() {
        now = !open;
    } else if response.dragged() {
        let moved = response.drag_delta().y;
        if moved < -2.0 {
            now = true;
        } else if moved > 2.0 {
            now = false;
        }
    }
    let visuals = ui.style().interact(&response);
    if open || response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 8.0, visuals.weak_bg_fill);
    }
    let color = visuals.fg_stroke.color;
    let (half, rise) = (8.0, if open { 4.0 } else { -4.0 });
    let centre = rect.center();
    ui.painter().add(egui::Shape::line(
        vec![
            centre + vec2(-half, -rise),
            centre + vec2(0.0, rise),
            centre + vec2(half, -rise),
        ],
        Stroke::new(2.5, color),
    ));
    now
}

/// The way back from a tile's page to the tiles, in `rect`: a chevron pointing left.
pub(super) fn back_button(ui: &mut egui::Ui, rect: Rect) -> egui::Response {
    let response = ui.interact(rect, ui.scope_id().with("practice_back"), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Back"));
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 8.0, visuals.weak_bg_fill);
    }
    let centre = rect.center();
    ui.painter().add(egui::Shape::line(
        vec![
            centre + vec2(3.0, -8.0),
            centre + vec2(-4.0, 0.0),
            centre + vec2(3.0, 8.0),
        ],
        Stroke::new(2.5, visuals.fg_stroke.color),
    ));
    response
}

/// The count-in's length, one to four bars, as a row of choices; returns the one tapped.
pub(super) fn count_in_row(ui: &mut egui::Ui, bars: u8) -> Option<u8> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
    let tokens = style::tokens(ui);
    let text = if ui.is_enabled() {
        tokens.text
    } else {
        tokens.text_weak
    };
    ui.painter().text(
        Pos2::new(row.left(), row.center().y),
        Align2::LEFT_CENTER,
        "Bars",
        FontId::proportional(14.0),
        text,
    );
    let count = gscore_play::MAX_COUNT_IN_BARS;
    let mut chosen = None;
    for choice in 1..=count {
        let from_right = f32::from(count - choice);
        let rect = Rect::from_min_size(
            Pos2::new(row.right() - (from_right + 1.0) * TARGET, row.top()),
            vec2(TARGET, TARGET),
        );
        let response = ui.interact(
            rect,
            ui.scope_id().with(("practice_count_in", choice)),
            Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                true,
                choice == bars,
                format!("{choice} bars of count-in"),
            )
        });
        let (fill, ink) = if choice == bars {
            (tokens.accent, tokens.on_accent)
        } else {
            (ui.style().interact(&response).weak_bg_fill, text)
        };
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, fill);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            choice.to_string(),
            FontId::proportional(15.0),
            ink,
        );
        if response.clicked() {
            chosen = Some(choice);
        }
    }
    chosen
}
