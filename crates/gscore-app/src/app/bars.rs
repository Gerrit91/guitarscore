//! Menus, the status bar and the notice bar (the toolbar is in `toolbar.rs`).

use egui::{Align, Layout as UiLayout};
use gscore_edit::{ActionId, Request};

use super::GuitarScoreApp;
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
                ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light score");
                ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark score");
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
