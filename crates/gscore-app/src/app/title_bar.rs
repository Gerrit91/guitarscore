//! The window's own title bar (the window has no system decorations): the menus on the
//! left, the song's title in the middle, and minimize, maximize/restore and close on the
//! right. Dragging the bar moves the window and a double click maximizes or restores it.
//! Without system decorations the window also needs its own resize edges.

use egui::viewport::ResizeDirection;
use egui::{
    Align2, Color32, CursorIcon, FontFamily, FontId, Rect, Sense, ViewportCommand, pos2, vec2,
};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;
use crate::fonts::BOLD;

/// The title bar's height.
pub(super) const TITLE_BAR_HEIGHT: f32 = 34.0;
/// Width of a window button.
const WINDOW_BUTTON: f32 = 44.0;
/// The window buttons' glyphs, and the practice view's way back to the full interface
/// (a picture of the interface, which needs more room to be told apart).
const GLYPH: f32 = 10.0;
const BACK_ICON: f32 = 18.0;
/// How close to the window's edge a press starts resizing.
const RESIZE_MARGIN: f32 = 5.0;

/// Whether the bar has window buttons and moves the window. A browser tab has its own,
/// and the page cannot be minimized, maximized or closed from inside.
pub(super) const WINDOW_CONTROLS: bool = cfg!(not(target_arch = "wasm32"));

impl GuitarScoreApp {
    /// The title bar; `menus` is whether it holds the menus (the practice view's has a
    /// button back to the full interface in their place, the title and the window
    /// buttons).
    pub(super) fn title_bar(&mut self, ui: &mut egui::Ui, menus: bool) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        // The background first, so the menus and buttons drawn later win the pointer.
        let background = ui.interact(
            rect,
            ui.scope_id().with("title_drag"),
            Sense::click_and_drag(),
        );
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        if !WINDOW_CONTROLS {
            // Nothing to drag or maximize in a browser tab.
        } else if background.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if background.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        // The song's title, centered on the whole bar.
        let tokens = style::tokens(ui);
        let title = self.document().map_or_else(
            || "GuitarScore".to_owned(),
            |document| {
                let changed = if document.is_dirty() { " •" } else { "" };
                format!("{}{changed}", document.display_title())
            },
        );
        let buttons = if WINDOW_CONTROLS {
            3.0 * WINDOW_BUTTON
        } else {
            0.0
        };
        let mut menus_end = rect.left();
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            if menus {
                menus_end = self.menu_bar(ui);
            } else {
                // The practice view's way back to the full interface, where the menus are.
                let back = window_button(
                    ui,
                    Icon::FullInterface,
                    "Back to the full interface",
                    false,
                    BACK_ICON,
                );
                menus_end = back.rect.right();
                if back.clicked() {
                    self.set_practice_view(super::PracticeChoice::Off);
                }
            }
            if !WINDOW_CONTROLS {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if window_button(ui, Icon::WindowClose, "Close window", true, GLYPH).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                let (icon, label) = if maximized {
                    (Icon::WindowRestore, "Restore window")
                } else {
                    (Icon::WindowMaximize, "Maximize window")
                };
                if window_button(ui, icon, label, false, GLYPH).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if window_button(ui, Icon::WindowMinimize, "Minimize window", false, GLYPH)
                    .clicked()
                {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
        });

        // The title goes where it clears the menus: centered on the bar when there is
        // room, else in the space right of the menus, and not at all on a phone held
        // upright (the menus fill the bar there).
        let font = FontId::new(13.0, FontFamily::Name(BOLD.into()));
        let galley = ui
            .painter()
            .layout_no_wrap(title.clone(), font, tokens.text);
        let free = Rect::from_min_max(
            pos2(menus_end + 16.0, rect.top()),
            pos2(rect.right() - buttons - 16.0, rect.bottom()),
        );
        let width = galley.size().x;
        let centered = rect.center().x - width / 2.0;
        let place = if centered >= free.left() && centered + width <= free.right() {
            Some((rect.center(), Align2::CENTER_CENTER))
        } else if free.width() >= 80.0 {
            Some((pos2(free.left(), rect.center().y), Align2::LEFT_CENTER))
        } else {
            None
        };
        if let Some((at, align)) = place {
            ui.painter().with_clip_rect(free).text(
                at,
                align,
                &title,
                FontId::new(13.0, FontFamily::Name(BOLD.into())),
                tokens.text,
            );
        }
    }
}

/// A window button: the full bar height, an icon, a hover fill (red for close).
/// A button of the bar, its icon drawn `size` points across (the window's own buttons'
/// glyphs are small, as the systems draw them).
fn window_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    close: bool,
    size: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(WINDOW_BUTTON, TITLE_BAR_HEIGHT), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Button);
    });
    let tokens = style::tokens(ui);
    let hovered = response.hovered();
    if hovered {
        let fill = if close {
            style::CLOSE_RED
        } else {
            tokens.control_hover
        };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let ink = if hovered { Color32::WHITE } else { tokens.text };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(size, size)),
        ink,
        icon,
    );
    response.on_hover_text(label)
}

/// Lets the window be resized at its edges and corners (it has no system frame). Call once
/// per frame after the panels, so the edges are checked over everything.
pub(super) fn resize_edges(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        return;
    }
    let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let screen = ctx.content_rect();
    let left = pos.x - screen.left() <= RESIZE_MARGIN;
    let right = screen.right() - pos.x <= RESIZE_MARGIN;
    let top = pos.y - screen.top() <= RESIZE_MARGIN;
    let bottom = screen.bottom() - pos.y <= RESIZE_MARGIN;
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (.., true) => ResizeDirection::South,
        _ => return,
    };
    ctx.set_cursor_icon(match direction {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    });
    if ctx.input(|input| input.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}
