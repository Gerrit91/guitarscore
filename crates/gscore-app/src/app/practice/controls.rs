//! The practice view's header and controls. Each control is drawn into a rectangle of its
//! own, so a row of them shares one centre line whatever their sizes.
//!
//! How they are arranged depends on the screen ([`Shape`]): a phone held upright has the
//! header at the top and two rows at the bottom, with a caption under the switches; a
//! phone on its side keeps the score as tall as it can (no header, one row); a tablet
//! held upright has the header and one row; a wide screen puts everything in the header.

use egui::{Align2, FontId, Pos2, Rect, Sense, Vec2, vec2};

use super::{PLAY, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;
use crate::app::style;

use super::widgets::{icon_button, play_button, readout};

pub(super) const GAP: f32 = 6.0;
pub(super) const GROUP_GAP: f32 = 18.0;
/// The speed's pill: − and + at its ends, the speed between them.
const SPEED_WIDTH: f32 = 2.0 * TARGET + 66.0;
/// The position (or, on a phone held sideways, where playback is and the loop).
const TIME_WIDTH: f32 = 56.0;
const STATUS_WIDTH: f32 = 150.0;
/// The height of the bottom bar's handle strip.
pub(super) const HANDLE: f32 = 16.0;
/// The height of a caption under a control.
const CAPTION: f32 = 14.0;
/// Space between two captioned switches (their captions are wider than they are).
const CAPTION_GAP: f32 = 16.0;

/// How the practice view is arranged on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A phone held upright: header, and two rows of controls with captions.
    Narrow,
    /// A tablet held upright: header, and one row of controls.
    Row,
    /// A phone on its side: no header, one row of controls with the menu.
    Short,
    /// A wide screen: everything in the header.
    Bar,
}

impl Shape {
    /// The arrangement for a screen of `size` points.
    pub(crate) fn of(size: Vec2) -> Shape {
        if size.y < 500.0 && size.x > size.y {
            Shape::Short
        } else if size.x >= 1000.0 {
            Shape::Bar
        } else if size.x >= 700.0 {
            Shape::Row
        } else {
            Shape::Narrow
        }
    }

    /// Whether this arrangement has a header.
    pub(crate) fn has_header(self) -> bool {
        self != Shape::Short
    }

    /// Whether the controls sit in a bar at the bottom.
    pub(crate) fn has_bottom(self) -> bool {
        true
    }
}

/// One control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Loop,
    Previous,
    Play,
    Next,
    Speed,
    Trainer,
    CountIn,
    Click,
    Time,
    Status,
    Chip,
    Menu,
}

impl Control {
    fn width(self) -> f32 {
        match self {
            Control::Play => PLAY,
            Control::Speed => SPEED_WIDTH,
            Control::Time => TIME_WIDTH,
            Control::Status => STATUS_WIDTH,
            Control::Chip => CHIP_WIDTH,
            _ => TARGET,
        }
    }

    /// The caption under it on a phone held upright, if it has one.
    fn caption(self) -> Option<&'static str> {
        match self {
            Control::Speed => Some("SPEED"),
            Control::Trainer => Some("TRAINER"),
            Control::CountIn => Some("COUNT-IN"),
            Control::Click => Some("CLICK"),
            _ => None,
        }
    }
}

/// The track chip's width: the colour, the name and the arrow.
const CHIP_WIDTH: f32 = 140.0;

const TRANSPORT: &[Control] = &[
    Control::Loop,
    Control::Previous,
    Control::Play,
    Control::Next,
];
const SWITCHES: &[Control] = &[Control::Trainer, Control::CountIn, Control::Click];

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

/// `controls` spread across `row` with equal space between them, centred on its middle.
fn spread(row: Rect, controls: &[Control]) -> Vec<(Control, Rect)> {
    let used: f32 = controls.iter().map(|control| control.width()).sum();
    let gaps = controls.len().saturating_sub(1).max(1) as f32;
    let gap = ((row.width() - used) / gaps).max(GAP);
    let mut x = row.left();
    controls
        .iter()
        .map(|&control| {
            let size = vec2(control.width(), control_height(control));
            let rect = Rect::from_center_size(Pos2::new(x + size.x / 2.0, row.center().y), size);
            x += size.x + gap;
            (control, rect)
        })
        .collect()
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
            right.extend([TRANSPORT, &[Control::Speed], SWITCHES]);
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
            self.practice_control(ui, control, rect, false);
        }
    }

    /// The controls at the bottom.
    pub(super) fn practice_controls(&mut self, ui: &mut egui::Ui, shape: Shape) {
        self.practice_handle(ui);
        let width = ui.available_width();
        match shape {
            Shape::Narrow => {
                // Speed on the left, the switches on the right, captions under them.
                let (top, _) =
                    ui.allocate_exact_size(vec2(width, TARGET + CAPTION), Sense::hover());
                let centre = top.top() + TARGET / 2.0;
                let mut placed = pack(top.left(), centre, &[&[Control::Speed]]);
                // The switches a little apart, so their captions do not run together.
                let step = TARGET + CAPTION_GAP;
                for (index, &control) in SWITCHES.iter().enumerate() {
                    let right = top.right() - (SWITCHES.len() - 1 - index) as f32 * step;
                    placed.push((
                        control,
                        Rect::from_center_size(
                            Pos2::new(right - TARGET / 2.0, centre),
                            vec2(TARGET, TARGET),
                        ),
                    ));
                }
                for (control, rect) in placed {
                    self.practice_control(ui, control, rect, true);
                }
                ui.add_space(4.0);
                // The transport spread across, the play button in the middle.
                let (bottom, _) = ui.allocate_exact_size(vec2(width, PLAY), Sense::hover());
                let row = bottom.shrink2(vec2(8.0, 0.0));
                for (control, rect) in spread(
                    row,
                    &[
                        Control::Loop,
                        Control::Previous,
                        Control::Play,
                        Control::Next,
                        Control::Time,
                    ],
                ) {
                    self.practice_control(ui, control, rect, false);
                }
            }
            Shape::Row | Shape::Short => {
                let mut groups: Vec<&[Control]> = vec![TRANSPORT, &[Control::Speed], SWITCHES];
                if shape == Shape::Short {
                    groups.push(&[Control::Status]);
                    groups.push(&[Control::Menu]);
                } else {
                    groups.push(&[Control::Time]);
                }
                let (row, _) = ui.allocate_exact_size(vec2(width, PLAY), Sense::hover());
                let used = packed_width(&groups);
                let left = row.left() + ((row.width() - used) / 2.0).max(0.0);
                for (control, rect) in pack(left, row.center().y, &groups) {
                    self.practice_control(ui, control, rect, false);
                }
            }
            // The controls are in the header; the bar holds only the handle.
            Shape::Bar => {}
        }
        if self.practice.drawer {
            self.practice_drawer(ui);
        }
        if self.playback.state() == gscore_play::PlayState::Playing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// Draws `control` in `rect` and does what it was asked to; `caption` puts its name
    /// under it.
    fn practice_control(&mut self, ui: &mut egui::Ui, control: Control, rect: Rect, caption: bool) {
        if caption && let Some(text) = control.caption() {
            ui.painter().text(
                Pos2::new(rect.center().x, rect.bottom() + 2.0),
                Align2::CENTER_TOP,
                text,
                FontId::proportional(10.0),
                style::tokens(ui).text_weak,
            );
        }
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        match control {
            Control::Loop => {
                let looping = self.playback.loop_selection();
                if icon_button(ui, rect, Icon::Loop, "Loop", Some(looping)).clicked() {
                    self.set_loop_selection(!looping);
                    if looping {
                        self.practice_forget_tap();
                    }
                }
            }
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
            Control::Speed => self.practice_speed(ui, rect),
            Control::Trainer => self.practice_trainer(ui, rect),
            Control::CountIn => {
                let count_in = self.playback.count_in();
                if icon_button(ui, rect, Icon::CountIn, "Count-in", Some(count_in)).clicked() {
                    self.set_count_in(!count_in);
                }
            }
            Control::Click => {
                let metronome = self.playback.metronome();
                if icon_button(ui, rect, Icon::Metronome, "Metronome", Some(metronome)).clicked() {
                    self.set_metronome(!metronome);
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
        assert_eq!(Shape::of(vec2(820.0, 1180.0)), Shape::Row);
        assert_eq!(Shape::of(vec2(1180.0, 820.0)), Shape::Bar);
    }

    #[test]
    fn a_packed_row_shares_one_centre_line() {
        let placed = pack(0.0, 30.0, &[TRANSPORT, &[Control::Speed], SWITCHES]);
        assert!(
            placed
                .iter()
                .all(|(_, rect)| (rect.center().y - 30.0).abs() < 1e-4)
        );
        let last = placed.last().map_or(0.0, |(_, rect)| rect.right());
        assert!((last - packed_width(&[TRANSPORT, &[Control::Speed], SWITCHES])).abs() < 1e-3);
        // Nothing overlaps.
        for pair in placed.windows(2) {
            assert!(pair[0].1.right() <= pair[1].1.left());
        }
    }
}
