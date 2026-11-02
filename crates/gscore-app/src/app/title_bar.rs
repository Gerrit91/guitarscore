//! The window's own title bar (the window has no system decorations): the menus on the
//! left, the song's title in the middle, and minimize, maximize/restore and close on the
//! right. Dragging the bar moves the window and a double click maximizes or restores it.
//! Without system decorations the window also needs its own resize edges.

use egui::viewport::ResizeDirection;
use egui::{Align2, Color32, CursorIcon, FontFamily, FontId, Rect, Sense, ViewportCommand, vec2};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;
use crate::fonts::BOLD;

/// The title bar's height.
pub(super) const TITLE_BAR_HEIGHT: f32 = 34.0;
/// Width of a window button.
const WINDOW_BUTTON: f32 = 44.0;
/// How close to the window's edge a press starts resizing.
const RESIZE_MARGIN: f32 = 5.0;
/// The close button's hover color.
const CLOSE_RED: Color32 = Color32::from_rgb(0xD9, 0x3A, 0x3A);

impl GuitarScoreApp {
    /// The title bar.
    pub(super) fn title_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        // The background first, so the menus and buttons drawn later win the pointer.
        let background = ui.interact(
            rect,
            ui.scope_id().with("title_drag"),
            Sense::click_and_drag(),
        );
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        if background.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if background.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        // The song's title, centered on the whole bar.
        let tokens = style::tokens(ui);
        let title = self.document.as_ref().map_or_else(
            || "GuitarScore".to_owned(),
            |document| {
                let changed = if document.is_dirty() { " •" } else { "" };
                format!("{}{changed}", document.display_title())
            },
        );
        let buttons = 3.0 * WINDOW_BUTTON;
        let title_room = Rect::from_center_size(
            rect.center(),
            vec2(
                (rect.width() - 2.0 * (buttons + 160.0)).max(80.0),
                rect.height(),
            ),
        );
        ui.painter().with_clip_rect(title_room).text(
            rect.center(),
            Align2::CENTER_CENTER,
            &title,
            FontId::new(13.0, FontFamily::Name(BOLD.into())),
            tokens.text,
        );

        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            self.menu_bar(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if window_button(ui, Icon::WindowClose, "Close window", true).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                let (icon, label) = if maximized {
                    (Icon::WindowRestore, "Restore window")
                } else {
                    (Icon::WindowMaximize, "Maximize window")
                };
                if window_button(ui, icon, label, false).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if window_button(ui, Icon::WindowMinimize, "Minimize window", false).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
        });
    }
}

/// A window button: the full bar height, an icon, a hover fill (red for close).
fn window_button(ui: &mut egui::Ui, icon: Icon, label: &str, close: bool) -> egui::Response {
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
            CLOSE_RED
        } else {
            tokens.control_hover
        };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let ink = if hovered { Color32::WHITE } else { tokens.text };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(10.0, 10.0)),
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
