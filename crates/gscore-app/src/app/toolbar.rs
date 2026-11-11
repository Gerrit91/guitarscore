//! The toolbar: grouped controls on the chrome layer. On the left the file and edit groups
//! and the panel toggles; in the middle the player (transport, position and tempo); on the
//! right zoom and the settings. The output is chosen in the settings window, the shown
//! track in the song grid.

use egui::{Align, FontFamily, FontId, Layout, Rect, RichText, Sense, UiBuilder, vec2};
use gscore_edit::{Action, ActionId, Request};
use gscore_model::{BeatPosition, MeasureIndex};
use gscore_play::Timeline;

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;
use super::tiles;
use crate::state::MeasureSelection;

/// The player's width before it was first measured.
const PLAYER_WIDTH: f32 = 220.0;

impl GuitarScoreApp {
    /// " (ctrl+z)": the keys bound to `action`, for a tooltip.
    pub(super) fn key_hint(&self, action: ActionId) -> String {
        key_hint(&self.keymap, action)
    }

    /// An icon button for an editor action, with its keys in the tooltip.
    fn action_icon(
        &mut self,
        ui: &mut egui::Ui,
        icon: Icon,
        action: ActionId,
        enabled: bool,
    ) -> bool {
        let label = action.label();
        let tip = format!("{label}{}", self.key_hint(action));
        ui.add_enabled_ui(enabled, |ui| icons::toolbar_button(ui, icon, label, &tip))
            .inner
            .clicked()
    }

    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let bar = ui.max_rect();
        let height = ui.spacing().interact_size.y + 4.0;
        let row = Rect::from_min_size(bar.min, vec2(bar.width(), height));

        // Left: file, edit, panels.
        let left = ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                self.file_and_edit(ui);
                self.panel_toggles(ui);
            },
        );
        let left_end = left.response.rect.right();

        // Right: settings rightmost, zoom before it (laid out right to left).
        let right = ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if icons::toolbar_button(ui, Icon::Settings, "Settings", "Settings…").clicked() {
                    self.open_settings_window();
                }
                if icons::toolbar_button(ui, Icon::Tuner, "Tuner", "Tuner: a reference tone…")
                    .clicked()
                {
                    self.open_tuner();
                }
                if icons::toolbar_button(ui, Icon::History, "History", "Edit history…").clicked()
                {
                    self.open_history();
                }
                self.zoom(ui);
            },
        );
        let right_start = right.response.rect.left();

        // Middle: the player, centered on the whole bar. Its width comes from the last
        // frame (egui lays out in one pass).
        let id = ui.scope_id().with("player_width");
        let width = ui
            .data(|data| data.get_temp::<f32>(id))
            .unwrap_or(PLAYER_WIDTH);
        // Centered on the whole bar where there is room; otherwise moved clear of the side
        // groups (the left one wins when even that does not fit).
        let gap = 8.0;
        let centered = row.center().x - width / 2.0;
        let x = centered.min(right_start - gap - width).max(left_end + gap);
        let player = Rect::from_min_size(
            egui::pos2(x, row.center().y - height / 2.0),
            vec2(width, height),
        );
        let drawn = ui.scope_builder(
            UiBuilder::new()
                .max_rect(player)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| self.transport(ui),
        );
        let measured = drawn.inner;
        if (measured - width).abs() > 0.5 {
            ui.data_mut(|data| data.insert_temp(id, measured));
            ui.ctx().request_repaint();
        }
    }

    /// Open, save, undo and redo.
    fn file_and_edit(&mut self, ui: &mut egui::Ui) {
        style::toolbar_group(ui, |ui| {
            if icons::toolbar_button(ui, Icon::NewSong, "New song", "Start a new score").clicked() {
                self.new_song(ui.ctx());
            }
            if icons::toolbar_button(ui, Icon::Open, "Open", "Open a Guitar Pro file…").clicked()
            {
                self.ask_open(ui.ctx());
            }
            let tip = format!("Save the file{}", self.key_hint(ActionId::Save));
            if icons::toolbar_button(ui, Icon::Save, "Save", &tip).clicked() {
                self.request(Request::Save, ui.ctx());
            }
        });
        let (can_undo, can_redo) = self.document().map_or((false, false), |d| {
            (d.editor().can_undo(), d.editor().can_redo())
        });
        style::toolbar_group(ui, |ui| {
            if self.action_icon(ui, Icon::Undo, ActionId::Undo, can_undo) {
                self.perform_action(Action::Plain(ActionId::Undo), ui.ctx());
            }
            if self.action_icon(ui, Icon::Redo, ActionId::Redo, can_redo) {
                self.perform_action(Action::Plain(ActionId::Redo), ui.ctx());
            }
        });
    }

    /// Zoom out, the zoom level (a click goes back to 100 %), zoom in. Called inside a
    /// right-to-left layout, so the parts are added in reverse.
    fn zoom(&mut self, ui: &mut egui::Ui) {
        style::toolbar_group(ui, |ui| {
            if self.action_icon(ui, Icon::ZoomIn, ActionId::ZoomIn, true) {
                self.view.zoom_in();
            }
            let zoom = format!("{:.0} %", self.view.zoom() * 100.0);
            let response = ui
                .add(
                    egui::Button::new(RichText::new(&zoom).size(12.0))
                        .frame(false)
                        .min_size(vec2(44.0, 22.0)),
                )
                .on_hover_text(format!(
                    "Back to 100 %{}",
                    self.key_hint(ActionId::ZoomReset)
                ));
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label(zoom));
            if response.clicked() {
                self.view.set_zoom(1.0);
            }
            if self.action_icon(ui, Icon::ZoomOut, ActionId::ZoomOut, true) {
                self.view.zoom_out();
            }
        });
    }

    /// Show or hide the docked panels: the palette, the song grid and the track panel.
    fn panel_toggles(&mut self, ui: &mut egui::Ui) {
        style::toolbar_group(ui, |ui| {
            let view = &mut self.view;
            for (icon, label, tip, shown) in [
                (
                    Icon::PanelLeft,
                    "Show palette",
                    "Show or hide the edit palette",
                    &mut view.show_edit_panel,
                ),
                (
                    Icon::PanelBottom,
                    "Show song grid",
                    "Show or hide the tracks and song grid",
                    &mut view.show_track_grid,
                ),
                (
                    Icon::PanelRight,
                    "Show track panel",
                    "Show or hide the track panel",
                    &mut view.show_track_panel,
                ),
            ] {
                if icons::toggle_button(ui, icon, label, tip, *shown).clicked() {
                    *shown = !*shown;
                }
            }
        });
    }

    /// The player: play/pause, stop, where playback (or the cursor) is, and the tempo
    /// there. Returns the width it took, for centering it next frame.
    fn transport(&mut self, ui: &mut egui::Ui) -> f32 {
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        let has_song = self.document().is_some();
        let (metronome, count_in) = (self.playback.metronome(), self.playback.count_in());
        let looping = self.playback.loop_selection();
        // Looping needs no selection: the whole song repeats then.
        let can_loop = has_song;
        let (mut toggle_metronome, mut toggle_count_in, mut toggle_loop) = (false, false, false);
        let (mut to_start, mut previous, mut next, mut to_end) = (false, false, false, false);
        let start = ui.cursor().left();
        style::toolbar_group(ui, |ui| {
            if ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toolbar_button(ui, Icon::ToStart, "Go to start", "Top of the score")
                })
                .inner
                .clicked()
            {
                to_start = true;
            }
            if ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toolbar_button(ui, Icon::PreviousBar, "Previous bar", "One bar back")
                })
                .inner
                .clicked()
            {
                previous = true;
            }
            let (icon, label) = if playing {
                (Icon::Pause, "Pause")
            } else {
                (Icon::Play, "Play")
            };
            let tip = format!("{label}{}", self.key_hint(ActionId::PlayPause));
            let play = ui
                .add_enabled_ui(has_song, |ui| icons::toolbar_button(ui, icon, label, &tip))
                .inner;
            if play.clicked() {
                self.play_pause();
            }
            let tip = format!("Stop{}", self.key_hint(ActionId::Stop));
            let stop = ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toolbar_button(ui, Icon::Stop, "Stop", &tip)
                })
                .inner;
            if stop.clicked() {
                self.stop_playback();
            }
            if ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toolbar_button(ui, Icon::NextBar, "Next bar", "One bar forward")
                })
                .inner
                .clicked()
            {
                next = true;
            }
            if ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toolbar_button(ui, Icon::ToEnd, "Go to end", "End of the score")
                })
                .inner
                .clicked()
            {
                to_end = true;
            }
            let metronome_clicked = ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toggle_button(
                        ui,
                        Icon::Metronome,
                        "Metronome",
                        "Click on every beat while playing",
                        metronome,
                    )
                })
                .inner
                .clicked();
            if metronome_clicked {
                toggle_metronome = true;
            }
            let count_in_clicked = ui
                .add_enabled_ui(has_song, |ui| {
                    icons::toggle_button(
                        ui,
                        Icon::CountIn,
                        "Count-in",
                        "Clicks before playback starts (length in Settings › Metronome)",
                        count_in,
                    )
                })
                .inner
                .clicked();
            if count_in_clicked {
                toggle_count_in = true;
            }
            if ui
                .add_enabled_ui(can_loop, |ui| {
                    icons::toggle_button(
                        ui,
                        Icon::Loop,
                        "Loop selection",
                        "Loop the selected bars (the whole song when nothing is selected)",
                        looping,
                    )
                })
                .inner
                .clicked()
            {
                toggle_loop = true;
            }
            self.position_readout(ui);
            ui.add_space(4.0);
            self.speed_trainer_button(ui);
            self.speed_control(ui);
        });
        if to_start {
            self.go_to_start();
        }
        if previous {
            self.previous_bar();
        }
        if next {
            self.next_bar();
        }
        if to_end {
            self.go_to_end();
        }
        if toggle_metronome {
            self.set_metronome(!metronome);
        }
        if toggle_count_in {
            self.set_count_in(!count_in);
        }
        if toggle_loop {
            self.set_loop_selection(!looping);
        }
        if self.playback.is_loading() {
            ui.label("Loading sounds…");
            // A browser reports real bytes; the desktop's synthesizer has no byte count.
            let (fraction, text) = match self.playback.load_progress() {
                Some((done, total)) if total > 0 => {
                    let percent =
                        u16::try_from(done.saturating_mul(100).checked_div(total).unwrap_or(0))
                            .unwrap_or(0)
                            .min(100);
                    (f32::from(percent) / 100.0, format!("{percent}%"))
                }
                _ => (0.0, String::new()),
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(text)
                    .animate(fraction <= 0.0)
                    .desired_width(80.0),
            );
            if ui.small_button("Cancel").clicked() {
                self.playback.cancel_loading();
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        if playing {
            // Keep the position moving.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        ui.min_rect().right() - start
    }

    /// The speed trainer's button, left of the speed: highlighted while the trainer is
    /// on; a click opens its settings.
    fn speed_trainer_button(&mut self, ui: &mut egui::Ui) {
        let trainer = self.playback.trainer();
        let response = icons::toggle_button(
            ui,
            Icon::SpeedTrainer,
            "Speed trainer",
            "Speed trainer: play a loop faster every few passes",
            trainer.enabled,
        );
        let mut changed = trainer;
        egui::Popup::menu(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(240.0);
                ui.set_max_width(240.0);
                speed_trainer_ui(ui, &mut changed, self.playback.trainer_passes());
            });
        if changed != trainer {
            self.set_speed_trainer(changed);
        }
    }

    /// The playback speed (25 % to 150 % of the written tempo: typed, or one of the usual
    /// steps), and whether the speed trainer is on.
    fn speed_control(&mut self, ui: &mut egui::Ui) {
        let mut speed = self.playback.speed();
        let chosen = speed;
        let trainer = self.playback.trainer();
        let response = egui::ComboBox::from_id_salt("playback_speed")
            .selected_text(format!("{:.0} %", speed * 100.0))
            .width(66.0)
            // Typing a speed must not close the list.
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Speed");
                    let typed = ui.add(percent_value(&mut speed));
                    ui.ctx().accesskit_node_builder(typed.id, |node| {
                        node.set_label("Speed in percent")
                    });
                });
                ui.separator();
                for choice in crate::playback::SPEED_CHOICES {
                    ui.selectable_value(&mut speed, choice, format!("{:.0} %", choice * 100.0));
                }
            })
            .response;
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Playback speed"));
        let mut tip = "Playback speed; the written tempo stays as it is".to_owned();
        if trainer.enabled {
            tip.push_str(&format!(
                "\nSpeed trainer: +{} % every {} passes, up to {:.0} %",
                trainer.step,
                trainer.every,
                trainer.target * 100.0
            ));
        }
        response.on_hover_text(tip);
        if (speed - chosen).abs() > f32::EPSILON {
            self.set_playback_speed(speed);
        }
    }

    /// The time (or the selection's time), the measure being played (or the cursor's), how
    /// full it is and its tempo.
    fn position_readout(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &mut tab.document;
        let timeline = document.timeline();
        let selection = tab.selection;
        let state = self.playback.state();
        let song = document.song();
        let cursor = document.editor().cursor();
        // Playing, paused or counting in: where the player is. Stopped: the cursor's beat,
        // which is where Play starts (the measure readout shows the cursor's measure too).
        let tick = timeline.as_deref().map(|timeline| {
            if state != gscore_play::PlayState::Stopped || self.playback.count_in_pending() {
                self.playback.position()
            } else {
                let at = BeatPosition {
                    track: cursor.track,
                    measure: cursor.measure,
                    voice: cursor.voice,
                    beat: cursor.beat,
                };
                timeline.tick_of(song, at).unwrap_or(0)
            }
        });
        let (elapsed, total) = match (timeline.as_deref(), tick) {
            (Some(timeline), Some(tick)) => playback_times(
                timeline,
                tick,
                selection,
                state == gscore_play::PlayState::Playing,
            ),
            _ => (0, 0),
        };
        let measure = self
            .playback
            .current_measure()
            .unwrap_or_else(|| document.editor().cursor().measure.get().saturating_add(1));
        let tokens = style::tokens(ui);
        ui.add_space(4.0);
        time_readout(ui, elapsed, total);
        ui.add_space(10.0);
        // Painted, not a Label, so screen readers get "Measure 3" rather than "3 / 12".
        let label = format!("Measure {measure}");
        let text = format!("{measure} / {}", song.measure_count());
        let font = FontId::new(13.0, FontFamily::Name(crate::fonts::BOLD.into()));
        let galley = ui.painter().layout_no_wrap(text, font, tokens.text);
        let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
        ui.painter().galley(rect.min, galley, tokens.text);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
        response.on_hover_text("The measure being played, or the cursor's");
        // How full that bar is in the cursor's track and voice.
        if let Some(fill) = bar_fill(
            song,
            cursor.track,
            MeasureIndex::new(measure.saturating_sub(1)),
            cursor.voice,
        ) {
            ui.add_space(10.0);
            fill_readout(ui, &fill);
        }
        // The tempo in effect there, including changes on a beat (as playback has them).
        let tempo = match (timeline.as_deref(), tick) {
            (Some(timeline), Some(tick)) => timeline.bpm_at(tick),
            _ => song.tempo(),
        };
        ui.add_space(10.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
        // A quarter note from the music font (SMuFL `metNoteQuarterUp`).
        tiles::music_glyph(
            ui.painter(),
            egui::pos2(rect.center().x, rect.bottom() - 5.0),
            "\u{ECA5}",
            15.0,
            tokens.text_weak,
        );
        ui.label(
            RichText::new(format!("= {tempo}"))
                .size(12.0)
                .color(tokens.text_weak),
        )
        .on_hover_text("Tempo at this point");
        ui.add_space(6.0);
    }
}

/// The position and total length, in microseconds, for the time readout at `tick` (the
/// player's position, or the cursor's while stopped). Both come from the same timeline (the
/// sequence's ticks and tempo are the same). With a measure selection the total is the
/// selection's length and the position is measured from its first measure — but only while
/// playback is inside it. Once playback leaves the selection the whole song's time is shown,
/// so the clock never freezes at the selection's end.
pub(super) fn playback_times(
    timeline: &Timeline,
    tick: u64,
    selection: Option<MeasureSelection>,
    playing: bool,
) -> (u64, u64) {
    let played = timeline.micros_at(tick);
    let whole = (played, timeline.micros());
    let Some(selection) = selection else {
        return whole;
    };
    let Some((start_tick, end_tick)) = timeline.measure_range(selection.start(), selection.end())
    else {
        return whole;
    };
    let start = timeline.micros_at(start_tick);
    let total = timeline.micros_at(end_tick).saturating_sub(start);
    // A stale selection must not stop the clock when playback goes on past it, or starts
    // before it.
    if playing && (played < start || played >= start.saturating_add(total)) {
        return whole;
    }
    (played.saturating_sub(start).min(total), total)
}

/// "00:00" (or "103:20" for very long songs) from microseconds.
pub(super) fn time_text(micros: u64) -> String {
    let seconds = micros / 1_000_000;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

/// The time position and total, painted (with one accessible label).
fn time_readout(ui: &mut egui::Ui, elapsed: u64, total: u64) {
    let tokens = style::tokens(ui);
    let elapsed_text = time_text(elapsed);
    let total_text = time_text(total);
    let text = format!("{elapsed_text} / {total_text}");
    let label = format!("Time {elapsed_text} of {total_text}");
    let font = FontId::new(12.0, FontFamily::Name(crate::fonts::BOLD.into()));
    let galley = ui.painter().layout_no_wrap(text, font, tokens.text_weak);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, tokens.text_weak);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    response.on_hover_text("Position and total length; a measure selection sets the total");
}

/// " (ctrl+z)": the keys `keymap` binds to `action`, for a tooltip; empty if none.
pub(super) fn key_hint(keymap: &gscore_edit::Keymap, action: ActionId) -> String {
    let keys: Vec<String> = keymap
        .keys_for(action)
        .iter()
        .map(ToString::to_string)
        .collect();
    if keys.is_empty() {
        String::new()
    } else {
        format!(" ({})", keys.join(", "))
    }
}

/// How full a bar is, against its time signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BarFill {
    /// The notes' length in beats of the time signature ("5.5"), and the beats in the bar.
    pub(super) beats: String,
    pub(super) of: u8,
    pub(super) state: FillState,
}

/// Under, exactly at, or over the bar's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FillState {
    Under,
    Full,
    Over,
}

/// How full `measure` is in `track` and `voice`; `None` if there is no such voice or its
/// lengths cannot be read.
pub(super) fn bar_fill(
    song: &gscore_model::Song,
    track: gscore_model::TrackIndex,
    measure: MeasureIndex,
    voice: gscore_model::VoiceIndex,
) -> Option<BarFill> {
    let view = song.track(track)?.measure(measure)?;
    let signature = view.time_signature().ok()?;
    let total = view.voice(voice)?.total_duration().ok()?;
    let bar = signature.measure_length();
    // Compare the fractions exactly: total.n / total.d against bar.n / bar.d.
    let left = u128::from(total.numerator()) * u128::from(bar.denominator());
    let right = u128::from(bar.numerator()) * u128::from(total.denominator());
    let state = match left.cmp(&right) {
        std::cmp::Ordering::Less => FillState::Under,
        std::cmp::Ordering::Equal => FillState::Full,
        std::cmp::Ordering::Greater => FillState::Over,
    };
    // In beats of the signature: the length in whole notes times the lower number.
    let numerator = u128::from(total.numerator()) * u128::from(signature.denominator());
    let denominator = u128::from(total.denominator()).max(1);
    Some(BarFill {
        beats: beats_text(numerator, denominator),
        of: signature.numerator(),
        state,
    })
}

/// `numerator / denominator` as "4", "5.5" or "3.33" (at most two decimals).
fn beats_text(numerator: u128, denominator: u128) -> String {
    let whole = numerator / denominator;
    let rest = numerator % denominator;
    if rest == 0 {
        return whole.to_string();
    }
    // Hundredths, rounded half up.
    let hundredths = (rest * 200 / denominator).div_ceil(2);
    let (whole, hundredths) = if hundredths >= 100 {
        (whole + 1, hundredths - 100)
    } else {
        (whole, hundredths)
    };
    if hundredths == 0 {
        return whole.to_string();
    }
    let text = format!("{whole}.{hundredths:02}");
    text.trim_end_matches('0').to_owned()
}

/// "4 / 4" with a green check mark, "5.5 / 4" with a red cross, or "3 / 4" alone.
fn fill_readout(ui: &mut egui::Ui, fill: &BarFill) {
    let tokens = style::tokens(ui);
    let (ink, label) = match fill.state {
        FillState::Full => (
            tokens.text,
            format!("Bar full: {} of {} beats", fill.beats, fill.of),
        ),
        FillState::Over => (
            FILL_RED,
            format!("Bar too long: {} of {} beats", fill.beats, fill.of),
        ),
        FillState::Under => (
            tokens.text_weak,
            format!("Bar not full yet: {} of {} beats", fill.beats, fill.of),
        ),
    };
    let text = format!("{} / {}", fill.beats, fill.of);
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(12.0), ink);
    let icon = 12.0;
    let gap = 5.0;
    let size = vec2(galley.size().x + gap + icon, galley.size().y.max(icon));
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let text_top = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .galley(egui::pos2(rect.left(), text_top), galley.clone(), ink);
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.right() - icon / 2.0, rect.center().y),
        vec2(icon, icon),
    );
    match fill.state {
        FillState::Full => paint_check(ui.painter(), icon_rect, FILL_GREEN),
        FillState::Over => paint_cross(ui.painter(), icon_rect, FILL_RED),
        FillState::Under => {}
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    response.on_hover_text(label);
}

/// The full bar's check mark and the overfull bar's cross.
const FILL_GREEN: egui::Color32 = egui::Color32::from_rgb(0x3F, 0xB9, 0x50);
const FILL_RED: egui::Color32 = egui::Color32::from_rgb(0xF2, 0x55, 0x4D);

fn paint_check(painter: &egui::Painter, rect: Rect, ink: egui::Color32) {
    let stroke = egui::Stroke::new(2.0, ink);
    let a = egui::pos2(rect.left() + 1.0, rect.center().y + 0.5);
    let b = egui::pos2(rect.left() + rect.width() * 0.4, rect.bottom() - 1.5);
    let c = egui::pos2(rect.right() - 0.5, rect.top() + 1.5);
    painter.add(egui::Shape::line(vec![a, b, c], stroke));
}

fn paint_cross(painter: &egui::Painter, rect: Rect, ink: egui::Color32) {
    let stroke = egui::Stroke::new(2.0, ink);
    let r = rect.shrink(1.5);
    painter.line_segment([r.left_top(), r.right_bottom()], stroke);
    painter.line_segment([r.right_top(), r.left_bottom()], stroke);
}

/// A speed (1 = 100 %) typed or dragged in whole percent, within the playback speed's
/// range.
fn percent_value(speed: &mut f32) -> egui::DragValue<'_> {
    let (low, high) = crate::playback::SPEED_RANGE;
    egui::DragValue::from_get_set(move |typed| {
        if let Some(percent) = typed {
            *speed = (percent / 100.0) as f32;
        }
        (f64::from(*speed) * 100.0).round()
    })
    .range(f64::from(low) * 100.0..=f64::from(high) * 100.0)
    .speed(1.0)
    .fixed_decimals(0)
    .suffix(" %")
}

/// The speed trainer's settings: on or off, how often and how much faster, and the top.
fn speed_trainer_ui(ui: &mut egui::Ui, trainer: &mut crate::playback::SpeedTrainer, passes: u32) {
    ui.checkbox(&mut trainer.enabled, "Raise the speed while a loop plays");
    ui.add_enabled_ui(trainer.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("Every");
            let every = ui.add(egui::DragValue::new(&mut trainer.every).range(1..=32));
            ui.ctx()
                .accesskit_node_builder(every.id, |node| node.set_label("Passes per step"));
            ui.label(if trainer.every == 1 {
                "pass:"
            } else {
                "passes:"
            });
            ui.add(
                egui::DragValue::new(&mut trainer.step)
                    .range(1..=20)
                    .prefix("+")
                    .suffix(" %"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Up to");
            let top = ui.add(percent_value(&mut trainer.target));
            ui.ctx()
                .accesskit_node_builder(top.id, |node| node.set_label("Top speed"));
        });
        ui.label(
            RichText::new(format!(
                "Starts at the speed beside this button. A pass is one round of Loop \
                 selection. {passes} since the last step."
            ))
            .weak(),
        );
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use gscore_model::testing::{self, GUITAR_6};
    use gscore_model::{BeatIndex, BeatPosition, MeasureIndex, NoteValue, TrackIndex, VoiceIndex};
    use gscore_play::{PPQ, Timeline};

    use super::{FillState, bar_fill, beats_text, playback_times, time_text};
    use crate::state::MeasureSelection;

    fn at(beat: usize) -> BeatPosition {
        BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(beat),
        }
    }

    fn fill(song: &gscore_model::Song) -> (String, u8, FillState) {
        let fill = bar_fill(
            song,
            TrackIndex::new(0),
            MeasureIndex::new(0),
            VoiceIndex::new(0),
        )
        .unwrap();
        (fill.beats, fill.of, fill.state)
    }

    #[test]
    fn a_bar_is_full_short_or_too_long() {
        // Four quarter rests in 4/4.
        let mut song = testing::song(&[GUITAR_6], 1).unwrap();
        assert_eq!(fill(&song), ("4".to_owned(), 4, FillState::Full));

        let _deleted = song.delete_beat(at(3)).unwrap();
        assert_eq!(fill(&song), ("3".to_owned(), 4, FillState::Under));

        let quarter = NoteValue::new(4, 0, None).unwrap();
        let eighth = NoteValue::new(8, 0, None).unwrap();
        let _quarter = song.insert_beat(at(3), quarter).unwrap();
        let _eighth = song.insert_beat(at(4), eighth).unwrap();
        assert_eq!(fill(&song), ("4.5".to_owned(), 4, FillState::Over));

        let dotted = NoteValue::new(4, 1, None).unwrap();
        let _longer = song.set_duration(at(4), dotted).unwrap();
        assert_eq!(fill(&song), ("5.5".to_owned(), 4, FillState::Over));
    }

    #[test]
    fn beats_are_written_with_at_most_two_decimals() {
        assert_eq!(beats_text(4, 1), "4");
        assert_eq!(beats_text(11, 2), "5.5");
        assert_eq!(beats_text(10, 3), "3.33");
        assert_eq!(beats_text(1, 4), "0.25");
        assert_eq!(beats_text(1999, 1000), "2");
    }

    #[test]
    fn there_is_no_fill_for_a_missing_voice() {
        let song = testing::song(&[GUITAR_6], 1).unwrap();
        assert_eq!(
            bar_fill(
                &song,
                TrackIndex::new(0),
                MeasureIndex::new(0),
                VoiceIndex::new(5)
            ),
            None
        );
    }

    #[test]
    fn times_are_written_as_minutes_and_seconds() {
        assert_eq!(time_text(0), "00:00");
        assert_eq!(time_text(9_000_000), "00:09");
        assert_eq!(time_text(65_000_000), "01:05");
        assert_eq!(time_text(6_000_000_000), "100:00");
    }

    #[test]
    fn a_measure_selection_sets_the_total_and_the_position_origin() {
        // Four 4/4 bars at 120 BPM: two seconds each.
        let song = testing::song(&[GUITAR_6], 4).unwrap();
        let timeline = Timeline::new(&song).unwrap();
        let selection = MeasureSelection::new(1).to(2);
        let bar = u64::from(PPQ).saturating_mul(4);

        let (elapsed, total) = playback_times(&timeline, 0, Some(selection), false);
        assert_eq!((elapsed, total), (0, 4_000_000));

        // One bar into the selection: two seconds elapsed.
        let (elapsed, total) =
            playback_times(&timeline, bar.saturating_mul(2), Some(selection), true);
        assert_eq!((elapsed, total), (2_000_000, 4_000_000));

        // After the selection the whole song's time takes over instead of freezing at the
        // selection's end.
        let (elapsed, total) =
            playback_times(&timeline, bar.saturating_mul(3), Some(selection), true);
        assert_eq!((elapsed, total), (6_000_000, 8_000_000));
        let (elapsed, total) =
            playback_times(&timeline, bar.saturating_mul(3), Some(selection), false);
        assert_eq!((elapsed, total), (4_000_000, 4_000_000));

        // Before the selection, while playing, the whole song is shown too.
        let (elapsed, total) = playback_times(&timeline, 0, Some(selection), true);
        assert_eq!((elapsed, total), (0, 8_000_000));

        let (elapsed, total) = playback_times(&timeline, bar, None, true);
        assert_eq!((elapsed, total), (2_000_000, 8_000_000));
    }
}
