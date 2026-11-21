//! The practice view's header and controls. Each control is drawn into a rectangle of its
//! own, so a row of them shares one centre line whatever their sizes.
//!
//! How they are arranged depends on the screen ([`Shape`]): a phone held upright has the
//! header at the top and two rows at the bottom, with a caption under the switches; a
//! phone on its side keeps the score as tall as it can (no header, one row); a tablet
//! held upright has the header and one row; a wide screen puts everything in the header.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2, vec2};

use super::{PLAY, PracticeChoice, SPEED_STEP, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::{self, Icon};
use crate::app::style;
use crate::playback::{SPEED_RANGE, SpeedTrainer};

/// Space between two controls of a group, and between two groups.
const GAP: f32 = 6.0;
const GROUP_GAP: f32 = 18.0;
/// The speed's pill: − and + at its ends, the speed between them.
const SPEED_WIDTH: f32 = 2.0 * TARGET + 66.0;
/// The position (or, on a phone held sideways, where playback is and the loop).
const TIME_WIDTH: f32 = 56.0;
const STATUS_WIDTH: f32 = 150.0;
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
        self != Shape::Bar
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
            Shape::Bar => {}
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

    /// The song's title over where playback is, in `rect`.
    fn practice_title_block(&self, ui: &mut egui::Ui, rect: Rect) {
        let tokens = style::tokens(ui);
        let (title, detail) = self.practice_title();
        let painter = ui.painter().with_clip_rect(rect);
        let bold = FontId::new(15.0, FontFamily::Name(crate::fonts::BOLD.into()));
        painter.text(
            Pos2::new(rect.left(), rect.center().y - 1.0),
            Align2::LEFT_BOTTOM,
            title,
            bold,
            tokens.text,
        );
        painter.text(
            Pos2::new(rect.left(), rect.center().y + 1.0),
            Align2::LEFT_TOP,
            detail,
            FontId::proportional(12.0),
            tokens.text_weak,
        );
    }

    /// The header's two lines: the song's title, and where playback (or the cursor) is,
    /// with the loop or the bar waiting for a second tap.
    fn practice_title(&self) -> (String, String) {
        let Some(tab) = self.tabs.get(self.active) else {
            return (
                "GuitarScore".to_owned(),
                "Open a song to practise".to_owned(),
            );
        };
        let song = tab.document.song();
        let measure = self.playback.current_measure().unwrap_or_else(|| {
            tab.document
                .editor()
                .cursor()
                .measure
                .get()
                .saturating_add(1)
        });
        let mut detail = format!("Bar {measure} of {}", song.measure_count());
        if self.playback.loop_selection()
            && let Some(selection) = tab.loop_measures()
        {
            let (first, last) = (selection.start() + 1, selection.end() + 1);
            if first == last {
                detail.push_str(&format!(" · loop bar {first}"));
            } else {
                detail.push_str(&format!(" · loop {first}–{last}"));
            }
        } else if let Some(first) = self.practice_pending() {
            detail.push_str(&format!(" · tap a bar to loop from {}", first + 1));
        }
        (tab.document.display_title(), detail)
    }

    /// How far playback (or the cursor) is into the song, "01:23".
    fn practice_time(&mut self) -> String {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return String::new();
        };
        let document = &mut tab.document;
        let Some(timeline) = document.timeline() else {
            return String::new();
        };
        let state = self.playback.state();
        let tick = if state != gscore_play::PlayState::Stopped || self.playback.count_in_pending() {
            self.playback.position()
        } else {
            timeline
                .tick_of(document.song(), document.editor().cursor().position())
                .unwrap_or(0)
        };
        let (elapsed, _) = crate::app::toolbar::playback_times(&timeline, tick, None, false);
        crate::app::toolbar::time_text(elapsed)
    }

    /// The shown track: its colour and name; a tap lists the song's tracks.
    fn practice_track_chip(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let tracks: Vec<(String, Color32)> = song
            .tracks()
            .map(|track| {
                let (red, green, blue) = track.color().parts();
                (track.name().to_owned(), Color32::from_rgb(red, green, blue))
            })
            .collect();
        let Some(shown) = tracks.len().checked_sub(1).map(|last| tab.track.min(last)) else {
            return;
        };
        let Some((name, color)) = tracks.get(shown).cloned() else {
            return;
        };
        let response = ui.interact(rect, ui.scope_id().with("practice_chip"), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Track: {name}"))
        });
        let tokens = style::tokens(ui);
        let visuals = ui.style().interact(&response);
        let painter = ui.painter();
        painter.rect_filled(rect, rect.height() / 2.0, visuals.weak_bg_fill);
        let dot = Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            vec2(9.0, 9.0),
        );
        painter.rect_filled(dot, 2.0, color);
        let arrow_x = rect.right() - 14.0;
        let text_room = Rect::from_min_max(
            Pos2::new(dot.right() + 7.0, rect.top()),
            Pos2::new(arrow_x - 8.0, rect.bottom()),
        );
        painter.with_clip_rect(text_room).text(
            Pos2::new(text_room.left(), rect.center().y),
            Align2::LEFT_CENTER,
            &name,
            FontId::proportional(13.0),
            tokens.text,
        );
        let c = Pos2::new(arrow_x, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ],
            tokens.text,
            Stroke::NONE,
        ));
        let mut chosen = None;
        egui::Popup::menu(&response).show(|ui| {
            ui.set_min_width(220.0);
            for (index, (name, color)) in tracks.iter().enumerate() {
                let row = ui.add_sized(
                    vec2(ui.available_width(), TARGET),
                    egui::Button::selectable(index == shown, format!("    {name}")),
                );
                let dot = Rect::from_center_size(
                    Pos2::new(row.rect.left() + 12.0, row.rect.center().y),
                    vec2(9.0, 9.0),
                );
                ui.painter().rect_filled(dot, 2.0, *color);
                if row.clicked() {
                    chosen = Some(index);
                }
            }
        });
        if let Some(index) = chosen {
            self.set_shown_track(index);
        }
    }

    /// The menu: open a song, keep the screen on, and the way to the full interface.
    fn practice_menu(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let button = icon_button(ui, rect, Icon::More, "Menu", None);
        let ctx = ui.ctx().clone();
        let mut open = false;
        let mut full = false;
        let mut by_width = false;
        #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
        let mut keep = self.settings.keep_screen_on;
        let key = self.key_hint(gscore_edit::ActionId::TogglePracticeView);
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(260.0);
            let row = |ui: &mut egui::Ui, text: &str| {
                ui.add_sized(
                    vec2(ui.available_width(), TARGET),
                    egui::Button::new(RichText::new(text).size(15.0)).frame(false),
                )
            };
            if row(ui, "Open a song…").clicked() {
                open = true;
            }
            #[cfg(target_arch = "wasm32")]
            switch_row(ui, &mut keep, "Keep the screen on while playing");
            ui.separator();
            if row(ui, &format!("Full interface (editing){key}")).clicked() {
                full = true;
            }
            if cfg!(target_arch = "wasm32")
                && PracticeChoice::parse(&self.settings.practice_view) != PracticeChoice::Auto
                && row(ui, "Choose by screen width").clicked()
            {
                by_width = true;
            }
        });
        if by_width {
            self.set_practice_view(PracticeChoice::Auto);
        }
        self.settings.keep_screen_on = keep;
        if open {
            self.ask_open(&ctx);
        }
        if full {
            self.set_practice_view(PracticeChoice::Off);
        }
    }

    /// The speed in a pill: a step slower, the speed, a step faster.
    fn practice_speed(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let tokens = style::tokens(ui);
        let speed = self.playback.speed();
        ui.painter().rect_filled(rect, 10.0, tokens.control);
        let minus = Rect::from_min_size(rect.min, vec2(TARGET, rect.height()));
        let plus = Rect::from_min_size(
            Pos2::new(rect.right() - TARGET, rect.top()),
            vec2(TARGET, rect.height()),
        );
        let mut chosen = speed;
        if sign_button(ui, minus, false, "Slower").clicked() {
            chosen = speed - SPEED_STEP;
        }
        if sign_button(ui, plus, true, "Faster").clicked() {
            chosen = speed + SPEED_STEP;
        }
        let color = if crate::app::toolbar::off_tempo(speed) {
            tokens.accent
        } else {
            tokens.text
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("{:.0} %", speed * 100.0),
            FontId::new(16.0, FontFamily::Name(crate::fonts::BOLD.into())),
            color,
        );
        // Steps land on whole multiples of 5 %, from whatever speed was typed before.
        let chosen =
            ((chosen / SPEED_STEP).round() * SPEED_STEP).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
        if (chosen - speed).abs() > 1e-4 {
            self.set_playback_speed(chosen);
        }
    }

    /// The speed trainer: highlighted while on; a tap opens its settings, sized for a
    /// finger.
    fn practice_trainer(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let trainer = self.playback.trainer();
        let button = icon_button(
            ui,
            rect,
            Icon::SpeedTrainer,
            "Speed trainer",
            Some(trainer.enabled),
        );
        let mut changed = trainer;
        let passes = self.playback.trainer_passes();
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(280.0);
                trainer_settings(ui, &mut changed, passes);
            });
        if changed != trainer {
            self.set_speed_trainer(changed);
        }
    }
}

/// The speed trainer's settings with buttons a finger can hit: on or off, how many
/// passes between steps, how much faster each step is, and the top speed.
fn trainer_settings(ui: &mut egui::Ui, trainer: &mut SpeedTrainer, passes: u32) {
    switch_row(
        ui,
        &mut trainer.enabled,
        "Raise the speed while a loop plays",
    );
    ui.add_enabled_ui(trainer.enabled, |ui| {
        stepper_row(ui, "Passes per step", &trainer.every.to_string(), |up| {
            trainer.every = if up {
                trainer.every.saturating_add(1).min(32)
            } else {
                trainer.every.saturating_sub(1).max(1)
            };
        });
        stepper_row(ui, "Step", &format!("+{} %", trainer.step), |up| {
            trainer.step = if up {
                trainer.step.saturating_add(1).min(20)
            } else {
                trainer.step.saturating_sub(1).max(1)
            };
        });
        stepper_row(
            ui,
            "Up to",
            &format!("{:.0} %", trainer.target * 100.0),
            |up| {
                let step = if up { SPEED_STEP } else { -SPEED_STEP };
                trainer.target = ((trainer.target + step) / SPEED_STEP).round() * SPEED_STEP;
                trainer.target = trainer.target.clamp(SPEED_RANGE.0, SPEED_RANGE.1);
            },
        );
        ui.label(
            RichText::new(format!(
                "Starts at the speed beside the trainer. A pass is one round of the loop. \
                 {passes} since the last step."
            ))
            .weak(),
        );
    });
}

/// A label, then − value + with touch-sized buttons; `step(true)` goes up.
fn stepper_row(ui: &mut egui::Ui, label: &str, value: &str, mut step: impl FnMut(bool)) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
    let tokens = style::tokens(ui);
    ui.painter().text(
        Pos2::new(row.left(), row.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        tokens.text,
    );
    let plus = Rect::from_min_size(
        Pos2::new(row.right() - TARGET, row.top()),
        vec2(TARGET, TARGET),
    );
    let value_rect =
        Rect::from_min_size(Pos2::new(plus.left() - 64.0, row.top()), vec2(64.0, TARGET));
    let minus = Rect::from_min_size(
        Pos2::new(value_rect.left() - TARGET, row.top()),
        vec2(TARGET, TARGET),
    );
    if sign_button(ui, minus, false, &format!("{label}: less")).clicked() {
        step(false);
    }
    if sign_button(ui, plus, true, &format!("{label}: more")).clicked() {
        step(true);
    }
    ui.painter().text(
        value_rect.center(),
        Align2::CENTER_CENTER,
        value,
        FontId::proportional(15.0),
        tokens.text,
    );
}

/// A row as tall as a finger: `label` on the left, an on/off switch on the right. A tap
/// anywhere on the row flips it (a checkbox's small box is hard to hit on a phone).
fn switch_row(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let (row, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(200.0), TARGET),
        Sense::click(),
    );
    if response.clicked() {
        *on = !*on;
    }
    let state = *on;
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, state, label));
    let tokens = style::tokens(ui);
    let enabled = ui.is_enabled();
    let text = if enabled {
        tokens.text
    } else {
        tokens.text_weak
    };
    let size = vec2(46.0, 28.0);
    let track = Rect::from_min_size(
        Pos2::new(row.right() - size.x, row.center().y - size.y / 2.0),
        size,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            row.min,
            Pos2::new(track.left() - 8.0, row.bottom()),
        ))
        .text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.0),
            text,
        );
    // The knob glides to its side.
    let how_on = ui.ctx().animate_bool(response.id, state);
    let fill = if state {
        tokens.accent
    } else {
        tokens.control_hover
    };
    ui.painter().rect_filled(track, size.y / 2.0, fill);
    let radius = size.y / 2.0 - 3.0;
    let x = egui::lerp(
        (track.left() + 3.0 + radius)..=(track.right() - 3.0 - radius),
        how_on,
    );
    let knob = if enabled {
        Color32::WHITE
    } else {
        tokens.text_weak
    };
    ui.painter()
        .circle_filled(Pos2::new(x, track.center().y), radius, knob);
    response
}

/// A touch-sized − or + (`plus`), drawn as lines so it sits exactly in the middle.
fn sign_button(ui: &mut egui::Ui, rect: Rect, plus: bool, label: &str) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("practice_sign", label)),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), 8.0, visuals.weak_bg_fill);
    }
    let centre = rect.center();
    let stroke = Stroke::new(2.0, visuals.fg_stroke.color);
    let arm = 7.0;
    ui.painter()
        .line_segment([centre - vec2(arm, 0.0), centre + vec2(arm, 0.0)], stroke);
    if plus {
        ui.painter()
            .line_segment([centre - vec2(0.0, arm), centre + vec2(0.0, arm)], stroke);
    }
    response
}

/// A touch-sized icon button: a switch (`Some(on)`, filled, highlighted while on) or a
/// plain action (`None`, filled only under the finger).
fn icon_button(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: Icon,
    label: &str,
    on: Option<bool>,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("practice_button", label)),
        Sense::click(),
    );
    response.widget_info(|| match on {
        Some(on) => egui::WidgetInfo::selected(egui::Role::Button, true, on, label),
        None => egui::WidgetInfo::labeled(egui::Role::Button, true, label),
    });
    let visuals = ui.style().interact(&response);
    let fill = match on {
        Some(true) => Some(ui.visuals().selection.bg_fill.gamma_multiply(0.45)),
        Some(false) => Some(visuals.weak_bg_fill),
        None => (response.hovered() || response.is_pointer_button_down_on())
            .then_some(visuals.weak_bg_fill),
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 8.0, fill);
    }
    let icon_rect = Rect::from_center_size(rect.center(), vec2(22.0, 22.0));
    icons::paint(ui.painter(), icon_rect, visuals.fg_stroke.color, icon);
    response
}

/// The round play/pause button in the accent colour.
fn play_button(ui: &mut egui::Ui, rect: Rect, icon: Icon, label: &str) -> egui::Response {
    let tokens = style::tokens(ui);
    let response = ui.interact(rect, ui.scope_id().with("practice_play"), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let fill = if response.is_pointer_button_down_on() {
        tokens.accent.gamma_multiply(0.8)
    } else {
        tokens.accent
    };
    ui.painter()
        .circle_filled(rect.center(), rect.width().min(rect.height()) / 2.0, fill);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon_rect, Color32::WHITE, icon);
    response
}

/// Weak text centred in `rect`, cut at its edges.
fn readout(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32) {
    ui.painter().with_clip_rect(rect).text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        style::tokens(ui).text_weak,
    );
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
