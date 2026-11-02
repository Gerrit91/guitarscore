//! Menus, toolbar (track, zoom, playback), status bar and the notice bar.

use egui::{Align, Layout as UiLayout};
use gscore_edit::{ActionId, Request};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use crate::playback::Output;
use crate::state::ThemeChoice;

impl GuitarScoreApp {
    /// A menu entry for `action`, with its keys from the keymap as the shortcut text.
    fn action_button(&self, ui: &mut egui::Ui, action: ActionId, enabled: bool) -> bool {
        let keys: Vec<String> = self
            .keymap
            .keys_for(action)
            .iter()
            .map(ToString::to_string)
            .collect();
        let button = egui::Button::new(action.label()).shortcut_text(keys.join(", "));
        ui.add_enabled(enabled, button).clicked()
    }

    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open…").clicked() {
                    self.ask_open(ui.ctx());
                }
                let mut chosen = None;
                let mut clear = false;
                ui.menu_button("Open Recent", |ui| {
                    if self.settings.recent_files.is_empty() {
                        ui.label("No recent files");
                    }
                    for path in &self.settings.recent_files {
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            chosen = Some(path.clone());
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            !self.settings.recent_files.is_empty(),
                            egui::Button::new("Clear recent files"),
                        )
                        .clicked()
                    {
                        clear = true;
                        ui.close();
                    }
                });
                if clear {
                    self.settings.recent_files.clear();
                    self.save_settings();
                }
                if let Some(path) = chosen {
                    self.open(path, ui.ctx());
                }
                let has_document = self.document.is_some();
                if self.action_button(ui, ActionId::Save, has_document) {
                    self.request(Request::Save, ui.ctx());
                }
                if ui
                    .add_enabled(has_document, egui::Button::new("Save As…"))
                    .clicked()
                {
                    self.save_as(ui.ctx());
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                let (can_undo, can_redo) = self.document.as_ref().map_or((false, false), |d| {
                    (d.editor().can_undo(), d.editor().can_redo())
                });
                for (action, enabled) in [(ActionId::Undo, can_undo), (ActionId::Redo, can_redo)] {
                    if self.action_button(ui, action, enabled) {
                        self.perform_action(gscore_edit::Action::Plain(action), ui.ctx());
                    }
                }
                ui.separator();
                let has_document = self.document.is_some();
                for action in [
                    ActionId::InsertBeat,
                    ActionId::DeleteBeat,
                    ActionId::InsertMeasure,
                    ActionId::DeleteMeasure,
                    ActionId::ClearNote,
                    ActionId::DurationShorter,
                    ActionId::DurationLonger,
                ] {
                    if self.action_button(ui, action, has_document) {
                        self.perform_action(gscore_edit::Action::Plain(action), ui.ctx());
                    }
                }
                ui.separator();
                if ui.button("Settings…").clicked() {
                    self.open_settings_window();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.view.show_track_grid, "Track grid");
                ui.checkbox(&mut self.view.show_track_panel, "Track panel");
                ui.checkbox(&mut self.view.show_edit_panel, "Edit palette");
                ui.separator();
                ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light theme");
                ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark theme");
                ui.separator();
                if ui.button("Zoom in").clicked() {
                    self.view.zoom_in();
                }
                if ui.button("Zoom out").clicked() {
                    self.view.zoom_out();
                }
                if ui.button("Actual size").clicked() {
                    self.view.set_zoom(1.0);
                }
            });
        });
    }

    /// The keys bound to `action`, as a tooltip suffix: " (ctrl+s)".
    fn key_hint(&self, action: ActionId) -> String {
        let keys: Vec<String> = self
            .keymap
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

    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::Open, "Open", "Open a Guitar Pro file…").clicked() {
                self.ask_open(ui.ctx());
            }
            if icons::button(
                ui,
                Icon::Save,
                "Save",
                &format!("Save the file{}", self.key_hint(ActionId::Save)),
            )
            .clicked()
            {
                self.request(Request::Save, ui.ctx());
            }
            ui.separator();
            let (can_undo, can_redo) = self.document.as_ref().map_or((false, false), |d| {
                (d.editor().can_undo(), d.editor().can_redo())
            });
            let undo = ui
                .add_enabled_ui(can_undo, |ui| {
                    icons::button(
                        ui,
                        Icon::Undo,
                        "Undo",
                        &format!("Undo{}", self.key_hint(ActionId::Undo)),
                    )
                })
                .inner;
            if undo.clicked() {
                self.perform_action(gscore_edit::Action::Plain(ActionId::Undo), ui.ctx());
            }
            let redo = ui
                .add_enabled_ui(can_redo, |ui| {
                    icons::button(
                        ui,
                        Icon::Redo,
                        "Redo",
                        &format!("Redo{}", self.key_hint(ActionId::Redo)),
                    )
                })
                .inner;
            if redo.clicked() {
                self.perform_action(gscore_edit::Action::Plain(ActionId::Redo), ui.ctx());
            }
            ui.separator();
            if icons::button(
                ui,
                Icon::ZoomOut,
                "Zoom out",
                &format!("Zoom out{}", self.key_hint(ActionId::ZoomOut)),
            )
            .clicked()
            {
                self.view.zoom_out();
            }
            ui.label(format!("{:.0} %", self.view.zoom() * 100.0));
            if icons::button(
                ui,
                Icon::ZoomIn,
                "Zoom in",
                &format!("Zoom in{}", self.key_hint(ActionId::ZoomIn)),
            )
            .clicked()
            {
                self.view.zoom_in();
            }
            ui.separator();
            self.playback_controls(ui);
            ui.separator();
            if icons::button(ui, Icon::Settings, "Settings", "Settings…").clicked() {
                self.open_settings_window();
            }
            if let Some(document) = &self.document {
                let song = document.song();
                let name = |index: usize| {
                    song.track(gscore_model::TrackIndex::new(index))
                        .map_or_else(String::new, |t| format!("{}: {}", index + 1, t.name()))
                };
                egui::ComboBox::from_label("Track")
                    .selected_text(name(self.view.track))
                    .show_ui(ui, |ui| {
                        for index in 0..song.track_count() {
                            ui.selectable_value(&mut self.view.track, index, name(index));
                        }
                    });
            }
            if self.tasks.busy() {
                ui.spinner();
            }
        });
    }

    /// The playback controls: play/pause, stop, the current measure and the output port.
    fn playback_controls(&mut self, ui: &mut egui::Ui) {
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        let has_song = self.document.is_some();
        let (icon, label) = if playing {
            (Icon::Pause, "Pause")
        } else {
            (Icon::Play, "Play")
        };
        let play = ui
            .add_enabled_ui(has_song, |ui| {
                icons::button(
                    ui,
                    icon,
                    label,
                    &format!("{label}{}", self.key_hint(ActionId::PlayPause)),
                )
            })
            .inner;
        if play.clicked() {
            self.play_pause();
        }
        let stop = ui
            .add_enabled_ui(has_song, |ui| {
                icons::button(
                    ui,
                    Icon::Stop,
                    "Stop",
                    &format!("Stop{}", self.key_hint(ActionId::Stop)),
                )
            })
            .inner;
        if stop.clicked() {
            self.stop_playback();
        }
        if let Some(measure) = self.playback.current_measure() {
            ui.label(format!("Measure {measure}"));
        }
        let mut chosen: Option<Output> = None;
        let mut pick_soundfont = false;
        let combo = egui::ComboBox::from_label("Output")
            .selected_text(self.playback.output_label())
            .show_ui(ui, |ui| {
                if let Some(soundfont) = self.playback.soundfont() {
                    let synth = Output::Synth(soundfont.to_path_buf());
                    let name = soundfont
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if ui
                        .selectable_label(
                            *self.playback.output() == synth,
                            format!("Synthesizer ({name})"),
                        )
                        .on_hover_text(soundfont.display().to_string())
                        .clicked()
                    {
                        chosen = Some(synth);
                    }
                }
                if ui
                    .selectable_label(false, "Choose SoundFont…")
                    .on_hover_text("Play with the built-in synthesizer and a SoundFont (.sf2)")
                    .clicked()
                {
                    pick_soundfont = true;
                }
                ui.separator();
                let midi = Output::Midi(None);
                if ui
                    .selectable_label(*self.playback.output() == midi, "MIDI (automatic)")
                    .clicked()
                {
                    chosen = Some(midi);
                }
                for port in self.playback.ports() {
                    let midi = Output::Midi(Some(port.clone()));
                    if ui
                        .selectable_label(*self.playback.output() == midi, format!("MIDI: {port}"))
                        .clicked()
                    {
                        chosen = Some(midi);
                    }
                }
            });
        // List the ports each time the menu opens, so newly started synths show up.
        if combo.response.clicked()
            && let Err(error) = self.playback.refresh_ports()
        {
            self.notice = Some(error);
        }
        if let Some(output) = chosen {
            self.playback.choose_output(output);
        }
        if pick_soundfont && let Err(error) = self.tasks.pick_soundfont(ui.ctx()) {
            self.notice = Some(error);
        }
        if self.playback.is_loading() {
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
                    .desired_width(100.0),
            );
            ui.label("Loading sounds…");
            if ui.button("Cancel").clicked() {
                self.playback.cancel_loading();
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        if playing {
            // Keep the measure display moving.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(super) fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.document {
            Some(document) => {
                // The file name only (screenshots stay the same wherever the file is); the
                // full path on hover.
                let path = document.path();
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let name = if document.is_dirty() {
                    format!("{name} (changed)")
                } else {
                    name
                };
                ui.label(name).on_hover_text(path.display().to_string());
                if let Some(status) = &self.status {
                    ui.separator();
                    ui.label(status);
                }
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    let song = document.song();
                    ui.label(format!(
                        "{}, {}",
                        count(song.measure_count(), "measure"),
                        count(song.track_count(), "track")
                    ));
                    ui.separator();
                    let cursor = document.editor().cursor();
                    ui.label(format!(
                        "Measure {}, beat {}, string {}",
                        cursor.measure.get() + 1,
                        cursor.beat.get() + 1,
                        cursor.string
                    ));
                });
            }
            None => {
                ui.label("No file open");
            }
        });
    }

    pub(super) fn notice_bar(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = self.notice.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, notice);
            if ui.small_button("Dismiss").clicked() {
                self.notice = None;
            }
        });
    }
}

/// "1 track", "3 tracks".
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}
