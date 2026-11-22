//! The practice view's player, and its header while no song is open. Each control is
//! drawn into a rectangle of its own, so a row of them shares one centre line whatever
//! their sizes. Everything else (the loop, the speed and its trainer, the count-in, the
//! click, …) is in the drawer.
//!
//! With a song open there is no header: the score reaches the screen's top, the shown
//! track floats in a chip in its top right corner. A phone has the player in a bar at the
//! bottom; wherever the drawer floats (a tablet or a wider screen) the player is in it,
//! under the tiles, and there is no bar at the bottom.

use egui::{Pos2, Rect, Sense, vec2};

use super::{PLAY, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;

use super::widgets::{icon_button, play_button, readout};

pub(super) const GAP: f32 = 6.0;
pub(super) const GROUP_GAP: f32 = 18.0;
/// The position.
const TIME_WIDTH: f32 = 56.0;
/// The track chip: the colour, the instrument, the name and the arrow.
pub(super) const CHIP: egui::Vec2 = vec2(160.0, 36.0);

/// One control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Previous,
    Play,
    Next,
    Time,
    /// The drawer's button.
    Drawer,
}

impl Control {
    fn width(self) -> f32 {
        match self {
            Control::Play => PLAY,
            Control::Time => TIME_WIDTH,
            Control::Previous | Control::Next | Control::Drawer => TARGET,
        }
    }
}

/// The player: a bar back, play or pause, a bar on.
const PLAYER: &[Control] = &[Control::Previous, Control::Play, Control::Next];
/// At the player row's left end: the drawer's button.
const LEADING: &[Control] = &[Control::Drawer];
/// At its right end: where playback is.
const TRAILING: &[Control] = &[Control::Time];

/// The narrowest a player row is that keeps the player in the middle: as much room on
/// either side as the wider of its ends takes.
pub(super) fn player_width() -> f32 {
    let side = packed_width(&[LEADING]).max(packed_width(&[TRAILING]));
    // The row's own margins (it is shrunk by 4 on either side).
    packed_width(&[PLAYER]) + 2.0 * (side + GROUP_GAP) + 8.0
}

/// The width of `groups` packed side by side.
fn packed_width(groups: &[&[Control]]) -> f32 {
    let mut width = 0.0;
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            width += GROUP_GAP;
        }
        for (place, control) in group.iter().enumerate() {
            if place > 0 {
                width += GAP;
            }
            width += control.width();
        }
    }
    width
}

/// The rectangles of `groups` packed from `left`, every one centred on `centre_y`.
fn pack(left: f32, centre_y: f32, groups: &[&[Control]]) -> Vec<(Control, Rect)> {
    let mut x = left;
    let mut placed = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            x += GROUP_GAP;
        }
        for (place, &control) in group.iter().enumerate() {
            if place > 0 {
                x += GAP;
            }
            let size = vec2(control.width(), control_height(control));
            placed.push((
                control,
                Rect::from_center_size(Pos2::new(x + size.x / 2.0, centre_y), size),
            ));
            x += size.x;
        }
    }
    placed
}

fn control_height(control: Control) -> f32 {
    match control {
        Control::Play => PLAY,
        Control::Previous | Control::Next | Control::Time | Control::Drawer => TARGET,
    }
}

impl GuitarScoreApp {
    /// The header while no song is open: what the view is for.
    pub(super) fn practice_header(&mut self, ui: &mut egui::Ui) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
        self.practice_title_block(ui, row.with_min_x(row.left() + 6.0));
    }

    /// The bar at the bottom (where the drawer does not float): the drawer's tiles while
    /// it is open, then the player.
    pub(super) fn practice_controls(&mut self, ui: &mut egui::Ui) {
        self.practice_drawer_inline(ui);
        self.practice_player(ui);
    }

    /// The player across the width there is: the drawer's button on the left, a bar back,
    /// play or pause and a bar on in the middle, where playback is on the right.
    pub(super) fn practice_player(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(width, PLAY), Sense::hover());
        let row = row.shrink2(vec2(4.0, 0.0));
        let centre = row.center().y;
        let right: &[&[Control]] = &[TRAILING];
        let right_left = row.right() - packed_width(right);
        let left_right = row.left() + packed_width(&[LEADING]);
        // In the middle, or (on a small phone) where it keeps clear of both ends.
        let player_left = (row.center().x - packed_width(&[PLAYER]) / 2.0)
            .min(right_left - GROUP_GAP - packed_width(&[PLAYER]))
            .max(left_right + GROUP_GAP);
        let mut placed = pack(row.left(), centre, &[LEADING]);
        placed.extend(pack(player_left, centre, &[PLAYER]));
        placed.extend(pack(right_left, centre, right));
        for (control, rect) in placed {
            self.practice_control(ui, control, rect);
        }
        if self.playback.state() == gscore_play::PlayState::Playing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// Draws `control` in `rect` and does what it was asked to.
    fn practice_control(&mut self, ui: &mut egui::Ui, control: Control, rect: Rect) {
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        match control {
            Control::Previous => {
                if icon_button(ui, rect, Icon::PreviousBar, "Previous bar", None).clicked() {
                    self.previous_bar();
                }
            }
            Control::Next => {
                if icon_button(ui, rect, Icon::NextBar, "Next bar", None).clicked() {
                    self.next_bar();
                }
            }
            Control::Play => {
                let (icon, label) = if playing {
                    (Icon::Pause, "Pause")
                } else {
                    (Icon::Play, "Play")
                };
                if play_button(ui, rect, icon, label).clicked() {
                    self.play_pause();
                }
            }
            Control::Time => {
                let text = self.practice_time();
                readout(ui, rect, &text, 14.0);
            }
            Control::Drawer => {
                self.practice.drawer = super::tiles::handle(ui, rect, self.practice.drawer);
                // Pulled up again, it shows its tiles, not the page it was left on.
                if !self.practice.drawer {
                    self.practice.page = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_row_shares_one_centre_line() {
        let groups: &[&[Control]] = &[LEADING, PLAYER, TRAILING];
        let placed = pack(0.0, 30.0, groups);
        assert!(
            placed
                .iter()
                .all(|(_, rect)| (rect.center().y - 30.0).abs() < 1e-4)
        );
        let last = placed.last().map_or(0.0, |(_, rect)| rect.right());
        assert!((last - packed_width(groups)).abs() < 1e-3);
        // Nothing overlaps.
        for pair in placed.windows(2) {
            assert!(pair[0].1.right() <= pair[1].1.left());
        }
    }
}
