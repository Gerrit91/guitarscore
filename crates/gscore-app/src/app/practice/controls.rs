//! The practice view's header and controls: the song, the shown track and the menu at
//! the top; play, the loop, the speed and speed trainer, the count-in and the metronome at
//! the bottom, every target at least [`TARGET`] points.

use egui::{Align, Align2, Color32, FontFamily, FontId, Layout, Rect, RichText, Sense, vec2};

use super::{PLAY, PracticeChoice, SPEED_STEP, TARGET};
use crate::app::GuitarScoreApp;
use crate::app::icons::{self, Icon};
use crate::app::style;
use crate::playback::{SPEED_RANGE, SpeedTrainer};

impl GuitarScoreApp {
    /// The header: the song, where playback is (and the loop), the shown track and the
    /// menu.
    pub(super) fn practice_header(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        ui.horizontal(|ui| {
            ui.set_min_height(TARGET);
            ui.add_space(6.0);
            let (title, detail) = self.practice_title();
            let right = 2.0 * TARGET + 120.0;
            ui.allocate_ui_with_layout(
                vec2((ui.available_width() - right).max(60.0), TARGET),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(title)
                                .font(FontId::new(
                                    15.0,
                                    FontFamily::Name(crate::fonts::BOLD.into()),
                                ))
                                .color(tokens.text),
                        )
                        .truncate(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(detail).size(12.0).color(tokens.text_weak))
                            .truncate(),
                    );
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                self.practice_menu(ui);
                self.practice_track_chip(ui);
            });
        });
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

    /// The shown track, as a chip that lists the song's tracks (only with more than one).
    fn practice_track_chip(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        if song.track_count() < 2 {
            return;
        }
        let names: Vec<String> = (0..song.track_count())
            .map(|index| {
                song.track(gscore_model::TrackIndex::new(index))
                    .map_or_else(
                        || format!("Track {}", index + 1),
                        |track| track.name().to_owned(),
                    )
            })
            .collect();
        let shown = tab.track.min(names.len().saturating_sub(1));
        let label = names.get(shown).cloned().unwrap_or_default();
        let chip = ui.add(
            egui::Button::new(RichText::new(format!("{label} ▾")).size(13.0))
                .corner_radius(TARGET / 2.0)
                .min_size(vec2(0.0, 36.0))
                .truncate(),
        );
        ui.ctx()
            .accesskit_node_builder(chip.id, |node| node.set_label(format!("Track: {label}")));
        let mut chosen = None;
        egui::Popup::menu(&chip).show(|ui| {
            ui.set_min_width(200.0);
            for (index, name) in names.iter().enumerate() {
                let row = ui.add_sized(
                    vec2(ui.available_width(), TARGET),
                    egui::Button::selectable(index == shown, name.as_str()),
                );
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
    fn practice_menu(&mut self, ui: &mut egui::Ui) {
        let button = practice_button(ui, Icon::Settings, "Menu", false);
        let ctx = ui.ctx().clone();
        let mut open = false;
        let mut full = false;
        let mut by_width = false;
        #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
        let mut keep = self.settings.keep_screen_on;
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
            ui.add_sized(
                vec2(ui.available_width(), TARGET),
                egui::Checkbox::new(&mut keep, "Keep the screen on while playing"),
            );
            ui.separator();
            if row(ui, "Full interface (editing)").clicked() {
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

    /// The controls: speed, trainer, count-in and metronome in the first row; the loop,
    /// bar back, play and bar forward in the second.
    pub(super) fn practice_controls(&mut self, ui: &mut egui::Ui) {
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        let width = ui.available_width();
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        // Wide enough (a phone on its side, a tablet): everything on one row.
        let one_row = width >= 640.0;
        ui.horizontal(|ui| {
            if one_row {
                self.practice_transport(ui, playing);
                ui.add_space(12.0);
            }
            self.practice_speed(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let metronome = self.playback.metronome();
                if practice_button(ui, Icon::Metronome, "Metronome", metronome).clicked() {
                    self.set_metronome(!metronome);
                }
                let count_in = self.playback.count_in();
                if practice_button(ui, Icon::CountIn, "Count-in", count_in).clicked() {
                    self.set_count_in(!count_in);
                }
                self.practice_trainer(ui);
            });
        });
        if !one_row {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // Centered under the first row.
                let used = 4.0 * TARGET + PLAY + 4.0 * 6.0 + 2.0 * 14.0;
                ui.add_space(((ui.available_width() - used) / 2.0).max(0.0));
                self.practice_transport(ui, playing);
            });
        }
        if playing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// The loop, bar back, play/pause, bar forward, and stop.
    fn practice_transport(&mut self, ui: &mut egui::Ui, playing: bool) {
        let looping = self.playback.loop_selection();
        if practice_button(ui, Icon::Loop, "Loop", looping).clicked() {
            self.set_loop_selection(!looping);
            if looping {
                self.practice.pending = None;
            }
        }
        if practice_button(ui, Icon::PreviousBar, "Previous bar", false).clicked() {
            self.previous_bar();
        }
        ui.add_space(8.0);
        let (icon, label) = if playing {
            (Icon::Pause, "Pause")
        } else {
            (Icon::Play, "Play")
        };
        if play_button(ui, icon, label).clicked() {
            self.play_pause();
        }
        ui.add_space(8.0);
        if practice_button(ui, Icon::NextBar, "Next bar", false).clicked() {
            self.next_bar();
        }
        if practice_button(ui, Icon::Stop, "Stop", false).clicked() {
            self.stop_playback();
            self.practice.pending = None;
        }
    }

    /// The speed: a step slower, the speed, a step faster.
    fn practice_speed(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let speed = self.playback.speed();
        let mut chosen = speed;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if text_button(ui, "−", "Slower").clicked() {
                chosen = speed - SPEED_STEP;
            }
            let color = if crate::app::toolbar::off_tempo(speed) {
                tokens.accent
            } else {
                tokens.text
            };
            let (rect, response) = ui.allocate_exact_size(vec2(62.0, TARGET), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("{:.0} %", speed * 100.0),
                FontId::new(16.0, FontFamily::Name(crate::fonts::BOLD.into())),
                color,
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Label,
                    true,
                    format!("Speed {:.0} %", speed * 100.0),
                )
            });
            if text_button(ui, "+", "Faster").clicked() {
                chosen = speed + SPEED_STEP;
            }
        });
        // Steps land on whole multiples of 5 %, from whatever speed was typed before.
        let chosen =
            ((chosen / SPEED_STEP).round() * SPEED_STEP).clamp(SPEED_RANGE.0, SPEED_RANGE.1);
        if (chosen - speed).abs() > 1e-4 {
            self.set_playback_speed(chosen);
        }
    }

    /// The speed trainer: highlighted while on; a tap opens its settings, sized for a
    /// finger.
    fn practice_trainer(&mut self, ui: &mut egui::Ui) {
        let trainer = self.playback.trainer();
        let button = practice_button(ui, Icon::SpeedTrainer, "Speed trainer", trainer.enabled);
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
    ui.add_sized(
        vec2(ui.available_width(), TARGET),
        egui::Checkbox::new(&mut trainer.enabled, "Raise the speed while a loop plays"),
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
    ui.horizontal(|ui| {
        ui.set_min_height(TARGET);
        ui.label(RichText::new(label).size(14.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if text_button(ui, "+", &format!("{label}: more")).clicked() {
                step(true);
            }
            let (rect, _) = ui.allocate_exact_size(vec2(64.0, TARGET), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                value,
                FontId::proportional(15.0),
                style::tokens(ui).text,
            );
            if text_button(ui, "−", &format!("{label}: less")).clicked() {
                step(false);
            }
        });
    });
}

/// A touch-sized button showing `text` (the speed's − and +).
fn text_button(ui: &mut egui::Ui, text: &str, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(TARGET, TARGET), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 8.0, visuals.weak_bg_fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(22.0),
        visuals.fg_stroke.color,
    );
    response
}

/// A touch-sized icon button, highlighted while `on`.
fn practice_button(ui: &mut egui::Ui, icon: Icon, label: &str, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(TARGET, TARGET), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
    let visuals = ui.style().interact(&response);
    let fill = if on {
        ui.visuals().selection.bg_fill.gamma_multiply(0.45)
    } else {
        visuals.weak_bg_fill
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    icons::paint(
        ui.painter(),
        rect.shrink(11.0),
        visuals.fg_stroke.color,
        icon,
    );
    response
}

/// The round play/pause button in the accent color.
fn play_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let tokens = style::tokens(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(PLAY, PLAY), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let fill = if response.is_pointer_button_down_on() {
        tokens.accent.gamma_multiply(0.8)
    } else {
        tokens.accent
    };
    ui.painter().circle_filled(rect.center(), PLAY / 2.0, fill);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon_rect, Color32::WHITE, icon);
    response
}
