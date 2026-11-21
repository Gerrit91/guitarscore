//! The practice view's pull-up bar: a handle on top of the bottom bar, and what it shows
//! when pulled up (the shown track's mute and solo, the transposition, the count-in's
//! length, the score's size, and the tuner, settings, saving and opening).

use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, vec2};

use super::TARGET;
use crate::app::GuitarScoreApp;
use crate::app::style;

use super::controls::{GAP, GROUP_GAP, HANDLE};
use super::widgets::stepper_row;

impl GuitarScoreApp {
    /// The handle at the top of the bottom bar: a tap, or a swipe up, pulls the bar up to
    /// show more; a tap or a swipe down puts it back.
    pub(super) fn practice_handle(&mut self, ui: &mut egui::Ui) {
        let (strip, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), HANDLE), Sense::click_and_drag());
        let open = self.practice.drawer;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, open, "More controls")
        });
        if response.clicked() {
            self.practice.drawer = !open;
        } else if response.dragged() {
            let moved = response.drag_delta().y;
            if moved < -2.0 {
                self.practice.drawer = true;
            } else if moved > 2.0 {
                self.practice.drawer = false;
            }
        }
        let visuals = ui.style().interact(&response);
        let grip = Rect::from_center_size(strip.center(), vec2(40.0, 5.0));
        ui.painter()
            .rect_filled(grip, 2.5, visuals.fg_stroke.color.gamma_multiply(0.6));
    }

    /// What the pulled-up bar shows: the shown track's mute and solo, the transposition,
    /// the count-in's length, the score's size, and the tuner, settings, saving and
    /// opening.
    pub(super) fn practice_drawer(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let width = ui.available_width();
        if width < 640.0 {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                self.practice_drawer_sound(ui);
                self.practice_drawer_more(ui);
            });
            return;
        }
        // Wide enough (a phone on its side, a tablet): two columns, so the score keeps its
        // height.
        ui.spacing_mut().item_spacing.x = 2.0 * GROUP_GAP;
        ui.columns(2, |columns| {
            if let [sound, more] = columns {
                sound.spacing_mut().item_spacing.y = 4.0;
                more.spacing_mut().item_spacing.y = 4.0;
                self.practice_drawer_sound(sound);
                self.practice_drawer_more(more);
            }
        });
    }

    /// The drawer's first part: the shown track's mute and solo, the transposition and the
    /// count-in's length.
    pub(super) fn practice_drawer_sound(&mut self, ui: &mut egui::Ui) {
        self.practice_mute_solo(ui);
        let transposition = self
            .document()
            .map(|document| document.song().transposition());
        if let Some(value) = transposition {
            let text = match value {
                Some(0) | None => "0".to_owned(),
                Some(step) => format!("{step:+}"),
            };
            let mut step = None;
            stepper_row(ui, "Transpose (semitones)", &text, |up| step = Some(up));
            let ctx = ui.ctx().clone();
            let action = match step {
                Some(true) => Some(gscore_edit::ActionId::TransposeUpSemitone),
                Some(false) => Some(gscore_edit::ActionId::TransposeDownSemitone),
                None => None,
            };
            if let Some(action) = action {
                self.perform_action(gscore_edit::Action::Plain(action), &ctx);
            }
        }
        self.practice_count_in_length(ui);
    }

    /// The drawer's second part: the score's size, then the tuner, settings, saving and
    /// opening as wide buttons two by two.
    pub(super) fn practice_drawer_more(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let zoom = self.settings.practice_zoom;
        stepper_row(ui, "Score size", &format!("{:.0} %", zoom * 100.0), |up| {
            let factor = if up { 1.1 } else { 1.0 / 1.1 };
            self.settings.practice_zoom =
                (zoom * factor).clamp(crate::state::ZOOM_RANGE.0, crate::state::ZOOM_RANGE.1);
        });
        let ctx = ui.ctx().clone();
        let save = if cfg!(target_arch = "wasm32") {
            "Download (.gp)"
        } else {
            "Save"
        };
        let labels = ["Tuner", "Settings", save, "Open a song…"];
        let mut chosen = None;
        for pair in labels.chunks(2) {
            let (row, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
            let half = (row.width() - GAP) / 2.0;
            for (index, label) in pair.iter().enumerate() {
                let rect = Rect::from_min_size(
                    row.min + vec2(index as f32 * (half + GAP), 0.0),
                    vec2(half, TARGET),
                );
                let button = ui.put(
                    rect,
                    egui::Button::new(RichText::new(*label).size(14.0))
                        .fill(tokens.control)
                        .corner_radius(8.0),
                );
                if button.clicked() {
                    chosen = Some(*label);
                }
            }
        }
        match chosen {
            Some("Tuner") => self.open_tuner(),
            Some("Settings") => self.open_settings_window(),
            Some("Open a song…") => self.ask_open(&ctx),
            Some(_) => self.save_song(&ctx),
            None => {}
        }
    }

    /// The shown track's name with its mute and solo, as in the mixer.
    pub(super) fn practice_mute_solo(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let index = tab.track;
        let Some(track) = tab
            .document
            .song()
            .track(gscore_model::TrackIndex::new(index))
        else {
            return;
        };
        let (name, muted, soloed) = (track.name().to_owned(), track.is_muted(), track.is_soloed());
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
        ui.painter().with_clip_rect(row).text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            &name,
            FontId::proportional(14.0),
            style::tokens(ui).text,
        );
        let solo_rect = Rect::from_min_size(
            Pos2::new(row.right() - TARGET, row.top()),
            vec2(TARGET, TARGET),
        );
        let mute_rect = solo_rect.translate(vec2(-(TARGET + GAP), 0.0));
        let track = gscore_model::TrackIndex::new(index);
        if ui.put(mute_rect, style::mute_button(muted)).clicked() {
            self.apply_command(
                gscore_edit::Command::SetTrackMute {
                    track,
                    mute: !muted,
                },
                false,
            );
        }
        if ui.put(solo_rect, style::solo_button(soloed)).clicked() {
            self.apply_command(
                gscore_edit::Command::SetTrackSolo {
                    track,
                    solo: !soloed,
                },
                false,
            );
        }
    }

    /// The count-in's length: one to four bars.
    pub(super) fn practice_count_in_length(&mut self, ui: &mut egui::Ui) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
        let tokens = style::tokens(ui);
        ui.painter().text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            "Count-in (bars)",
            FontId::proportional(14.0),
            tokens.text,
        );
        let bars = self.playback.count_in_bars();
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
            let fill = if choice == bars {
                tokens.accent
            } else {
                ui.style().interact(&response).weak_bg_fill
            };
            ui.painter().rect_filled(rect.shrink(2.0), 6.0, fill);
            let text = if choice == bars {
                tokens.on_accent
            } else {
                tokens.text
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                choice.to_string(),
                FontId::proportional(15.0),
                text,
            );
            if response.clicked() {
                chosen = Some(choice);
            }
        }
        if let Some(choice) = chosen {
            self.playback.set_count_in_bars(choice);
        }
    }
}
