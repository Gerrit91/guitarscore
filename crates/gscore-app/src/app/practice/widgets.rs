//! The practice view's touch-sized widgets: icon and play buttons, the speed's pill and
//! its − and +, switches and steppers, and the speed trainer's settings.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, vec2};

use super::{SPEED_STEP, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::{self, Icon};
use crate::app::style;
use crate::playback::{SPEED_RANGE, SpeedTrainer};

impl GuitarScoreApp {
    /// The playback speed in a pill: a step slower, the speed over what it makes of the
    /// tempo written there ("Speed · 180 bpm", so a step is seen even while stopped), a
    /// step faster.
    pub(super) fn practice_speed(&mut self, ui: &mut egui::Ui, rect: Rect) {
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
        let caption = match self.practice_tempo() {
            Some(bpm) => format!("Speed · {:.0} bpm", bpm as f32 * speed),
            None => "Speed".to_owned(),
        };
        let middle = Rect::from_min_max(
            Pos2::new(minus.right(), rect.top()),
            Pos2::new(plus.left(), rect.bottom()),
        );
        let painter = ui.painter().with_clip_rect(middle);
        painter.text(
            Pos2::new(middle.center().x, middle.center().y + 2.0),
            Align2::CENTER_BOTTOM,
            format!("{:.0} %", speed * 100.0),
            FontId::new(16.0, FontFamily::Name(crate::fonts::BOLD.into())),
            color,
        );
        painter.text(
            Pos2::new(middle.center().x, middle.center().y + 4.0),
            Align2::CENTER_TOP,
            caption,
            FontId::proportional(11.0),
            tokens.text_weak,
        );
        // Steps land on whole multiples of 5 %, from whatever speed was typed before.
        let chosen =
            ((chosen / SPEED_STEP).round() * SPEED_STEP).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
        if (chosen - speed).abs() > 1e-4 {
            self.set_playback_speed(chosen);
        }
    }
}

/// The speed trainer's settings with buttons a finger can hit: on or off, how many
/// passes between steps, how much faster each step is, and the top speed.
pub(super) fn trainer_settings(ui: &mut egui::Ui, trainer: &mut SpeedTrainer, passes: u32) {
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
pub(super) fn stepper_row(ui: &mut egui::Ui, label: &str, value: &str, mut step: impl FnMut(bool)) {
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
pub(super) fn switch_row(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
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
pub(super) fn sign_button(
    ui: &mut egui::Ui,
    rect: Rect,
    plus: bool,
    label: &str,
) -> egui::Response {
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
pub(super) fn icon_button(
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
pub(super) fn play_button(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: Icon,
    label: &str,
) -> egui::Response {
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
pub(super) fn readout(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32) {
    ui.painter().with_clip_rect(rect).text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        style::tokens(ui).text_weak,
    );
}
