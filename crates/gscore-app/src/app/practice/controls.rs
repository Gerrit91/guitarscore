//! The practice view's header and player. Each control is drawn into a rectangle of its
//! own, so a row of them shares one centre line whatever their sizes. Everything else
//! (the loop, the speed and its trainer, the count-in, the click, …) is in the drawer.
//!
//! How they are arranged depends on the screen ([`Shape`]): a phone held upright has the
//! header at the top and the player at the bottom; a phone on its side keeps the score as
//! tall as it can (no header, the player with where playback is and the menu); a tablet
//! or a wider screen puts the player in the header and has no bar at the bottom.

use egui::{Pos2, Rect, Sense, Vec2, vec2};

use super::{PLAY, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;

use super::widgets::{icon_button, play_button, readout};

pub(super) const GAP: f32 = 6.0;
pub(super) const GROUP_GAP: f32 = 18.0;
/// The position (or, on a phone held sideways, where playback is and the loop).
const TIME_WIDTH: f32 = 56.0;
const STATUS_WIDTH: f32 = 150.0;

/// How the practice view is arranged on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A phone held upright: header, and the player at the bottom.
    Narrow,
    /// A phone on its side: no header; the player with the menu at the bottom.
    Short,
    /// A tablet or a wider screen: the player in the header, no bar at the bottom.
    Bar,
}

impl Shape {
    /// The arrangement for a screen of `size` points.
    pub(crate) fn of(size: Vec2) -> Shape {
        if size.y < 500.0 && size.x > size.y {
            Shape::Short
        } else if size.x >= 700.0 {
            Shape::Bar
        } else {
            Shape::Narrow
        }
    }

    /// Whether this arrangement has a header.
    pub(crate) fn has_header(self) -> bool {
        self != Shape::Short
    }

    /// Whether the player sits in a bar at the bottom.
    pub(crate) fn has_bottom(self) -> bool {
        self != Shape::Bar
    }
}

/// One control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Previous,
    Play,
    Next,
    Time,
    Status,
    Chip,
    Menu,
}

impl Control {
    fn width(self) -> f32 {
        match self {
            Control::Play => PLAY,
            Control::Time => TIME_WIDTH,
            Control::Status => STATUS_WIDTH,
            Control::Chip => CHIP_WIDTH,
            _ => TARGET,
        }
    }
}

/// The track chip's width: the colour, the name and the arrow.
const CHIP_WIDTH: f32 = 140.0;

/// The player: a bar back, play or pause, a bar on.
const PLAYER: &[Control] = &[Control::Previous, Control::Play, Control::Next];

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
        Control::Chip => 36.0,
        _ => TARGET,
    }
}

impl GuitarScoreApp {
    /// The header: the song and where playback is, then (on a wide screen) the controls,
    /// the shown track and the menu.
    pub(super) fn practice_header(&mut self, ui: &mut egui::Ui, shape: Shape, has_song: bool) {
        let height = if shape == Shape::Bar { PLAY } else { TARGET };
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let mut right: Vec<&[Control]> = Vec::new();
        if has_song {
            right.push(&[Control::Chip]);
        }
        if shape == Shape::Bar {
            right.push(PLAYER);
        }
        right.push(&[Control::Menu]);
        let width = packed_width(&right);
        let placed = pack(row.right() - width, row.center().y, &right);
        // The song and where playback is, in what is left on the left.
        let title_room = Rect::from_min_max(
            row.left_top() + vec2(6.0, 0.0),
            Pos2::new(row.right() - width - GROUP_GAP, row.bottom()),
        );
        self.practice_title_block(ui, title_room);
        for (control, rect) in placed {
            self.practice_control(ui, control, rect);
        }
    }

    /// The bar at the bottom: the drawer's handle and tiles when they do not float, then
    /// the player in the middle, with where playback is on the right (and, on a phone on
    /// its side, what the header would say on the left and the menu).
    pub(super) fn practice_controls(&mut self, ui: &mut egui::Ui, shape: Shape, floating: bool) {
        if !floating {
            self.practice_drawer_inline(ui);
        }
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(width, PLAY), Sense::hover());
        let row = row.shrink2(vec2(4.0, 0.0));
        let centre = row.center().y;
        let mut placed = pack(
            row.center().x - packed_width(&[PLAYER]) / 2.0,
            centre,
            &[PLAYER],
        );
        let right: &[&[Control]] = if shape == Shape::Short {
            &[&[Control::Time], &[Control::Menu]]
        } else {
            &[&[Control::Time]]
        };
        placed.extend(pack(row.right() - packed_width(right), centre, right));
        if shape == Shape::Short {
            placed.extend(pack(row.left(), centre, &[&[Control::Status]]));
        }
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
            Control::Status => {
                let (_, detail) = self.practice_title();
                readout(ui, rect, &detail, 12.0);
            }
            Control::Chip => self.practice_track_chip(ui, rect),
            Control::Menu => self.practice_menu(ui, rect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_screen_gets_its_arrangement() {
        assert_eq!(Shape::of(vec2(390.0, 844.0)), Shape::Narrow);
        assert_eq!(Shape::of(vec2(844.0, 390.0)), Shape::Short);
        assert_eq!(Shape::of(vec2(820.0, 1180.0)), Shape::Bar);
        assert_eq!(Shape::of(vec2(1180.0, 820.0)), Shape::Bar);
    }

    #[test]
    fn a_packed_row_shares_one_centre_line() {
        let placed = pack(0.0, 30.0, &[PLAYER, &[Control::Time], &[Control::Menu]]);
        assert!(
            placed
                .iter()
                .all(|(_, rect)| (rect.center().y - 30.0).abs() < 1e-4)
        );
        let last = placed.last().map_or(0.0, |(_, rect)| rect.right());
        assert!((last - packed_width(&[PLAYER, &[Control::Time], &[Control::Menu]])).abs() < 1e-3);
        // Nothing overlaps.
        for pair in placed.windows(2) {
            assert!(pair[0].1.right() <= pair[1].1.left());
        }
    }
}
