//! Menus, the status bar and the notice bar (the toolbar is in `toolbar.rs`).

use egui::{Align, Layout as UiLayout};
use gscore_edit::{ActionId, Request};

use super::GuitarScoreApp;
use super::page_menu::{self, Section};
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
        let mut choices = Vec::new();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_song(ui.ctx());
                    ui.close();
                }
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
                let has_document = self.document().is_some();
                if self.action_button(ui, ActionId::Save, has_document) {
                    self.request(Request::Save, ui.ctx());
                }
                if ui
                    .add_enabled(has_document, egui::Button::new("Save As…"))
                    .clicked()
                {
                    self.save_as(ui.ctx());
                }
                let mut export = None;
                ui.add_enabled_ui(has_document, |ui| {
                    ui.menu_button("Export", |ui| {
                        for format in crate::export::ExportFormat::ALL {
                            let mut entry = ui.button(format.label());
                            if let Some(tip) = format.loses_details() {
                                entry = entry.on_hover_text(tip);
                            }
                            if entry.clicked() {
                                export = Some(format);
                                ui.close();
                            }
                        }
                    });
                });
                if let Some(format) = export {
                    self.ask_export(format, ui.ctx());
                }
                ui.separator();
                if ui.button("Settings…").clicked() {
                    self.open_settings_window();
                    ui.close();
                }
                // A browser tab is closed by the browser, not from the page.
                if cfg!(not(target_arch = "wasm32")) {
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
            ui.menu_button("Edit", |ui| {
                let (can_undo, can_redo) = self.document().map_or((false, false), |d| {
                    (d.editor().can_undo(), d.editor().can_redo())
                });
                for (action, enabled) in [(ActionId::Undo, can_undo), (ActionId::Redo, can_redo)] {
                    if self.action_button(ui, action, enabled) {
                        self.perform_action(gscore_edit::Action::Plain(action), ui.ctx());
                    }
                }
                ui.separator();
                let has_document = self.document().is_some();
                for action in [ActionId::Cut, ActionId::Copy, ActionId::Paste] {
                    if self.action_button(ui, action, has_document) {
                        self.perform_action(gscore_edit::Action::Plain(action), ui.ctx());
                    }
                }
                ui.separator();
                if ui.button("History…").clicked() {
                    self.open_history();
                    ui.close();
                }
                ui.separator();
                let has_document = self.document().is_some();
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
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.view.show_track_grid, "Track grid");
                ui.checkbox(&mut self.view.show_track_panel, "Track panel");
                ui.checkbox(&mut self.view.show_edit_panel, "Edit palette");
                ui.separator();
                ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light score");
                ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark score");
                ui.separator();
                for view in crate::state::ScoreView::ALL {
                    ui.radio_value(&mut self.view.score_view, view, view.label());
                }
                ui.separator();
                self.score_view_menu(ui);
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
                ui.separator();
                // The whole interface: Ctrl+Shift with +, - and 0 (read in
                // `interface_zoom_keys`, not from the keymap).
                if ui
                    .button("Larger interface")
                    .on_hover_text("Menus, panels and icons larger (ctrl+shift++)")
                    .clicked()
                {
                    self.view.interface_larger();
                }
                if ui
                    .button("Smaller interface")
                    .on_hover_text("Menus, panels and icons smaller (ctrl+shift+-)")
                    .clicked()
                {
                    self.view.interface_smaller();
                }
                if ui
                    .button("Normal interface size")
                    .on_hover_text("ctrl+shift+0")
                    .clicked()
                {
                    self.view.set_interface_zoom(1.0);
                }
                ui.separator();
                if self.action_button(ui, ActionId::ToggleFullscreen, true) {
                    self.perform_action(
                        gscore_edit::Action::Plain(ActionId::ToggleFullscreen),
                        ui.ctx(),
                    );
                    ui.close();
                }
            });
            // Guitar Pro's order: the track, its bars, their notes.
            for (title, section) in [
                ("Track", Section::Track),
                ("Bar", Section::Bar),
                ("Note", Section::Note),
            ] {
                ui.add_enabled_ui(self.document().is_some(), |ui| {
                    ui.menu_button(title, |ui| {
                        if let Some(tab) = self.tabs.get(self.active) {
                            let several = tab.shown_tracks().len() > 1;
                            page_menu::with_target(&tab.document, several, |target| {
                                page_menu::section(ui, section, target, &self.keymap, &mut choices);
                            });
                        }
                    });
                });
            }
            ui.menu_button("Help", |ui| {
                if ui.button("About GuitarScore").clicked() {
                    self.open_about();
                    ui.close();
                }
            });
        });
        let ctx = ui.ctx().clone();
        self.run_menu_choices(choices, &ctx);
    }

    /// The keys bound to `action`, as a tooltip suffix: " (ctrl+s)".
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        // Opening shows in the status bar (always present) rather than a panel of its own,
        // so the page does not jump when it appears.
        let progress = self.tasks.open_progress().cloned();
        let mut cancel = false;
        ui.horizontal(|ui| {
            match &progress {
                Some(progress) => {
                    ui.label("Opening…");
                    if let Some(fraction) = progress.fraction() {
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(200.0)
                                .desired_height(12.0)
                                .show_percentage(),
                        );
                    } else {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                    if progress.importing() {
                        ui.label("Importing…");
                    }
                    if ui.link("Cancel").clicked() {
                        cancel = true;
                    }
                    // Keep the progress and the spinner moving.
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
                None => {
                    if let Some(document) = self.document() {
                        // The file name only (screenshots stay the same wherever the file
                        // is); the full path on hover.
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
                        // A background job (a save, a file dialog, an export) in progress.
                        if self.tasks.busy() {
                            ui.add(egui::Spinner::new().size(14.0))
                                .on_hover_text("Working…");
                        }
                        if let Some(status) = &self.status {
                            ui.separator();
                            ui.label(status);
                        }
                    } else {
                        ui.label("No file open");
                        if self.tasks.busy() {
                            ui.add(egui::Spinner::new().size(14.0))
                                .on_hover_text("Working…");
                        }
                    }
                }
            }
            // The open document's counts, page and cursor, on the right.
            if let Some(document) = self.document() {
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    let song = document.song();
                    ui.label(format!(
                        "{}, {}",
                        count(song.measure_count(), "measure"),
                        count(song.track_count(), "track")
                    ));
                    ui.separator();
                    // Recorded by the page view; the view may have switched since.
                    if self.view.score_view.on_pages()
                        && let Some((page, pages)) =
                            self.tabs.get(self.active).and_then(|tab| tab.page_info)
                    {
                        ui.label(format!("Page {page} of {pages}"));
                        ui.separator();
                    }
                    let cursor = document.editor().cursor();
                    ui.label(format!(
                        "Measure {}, beat {}, string {}",
                        cursor.measure.get() + 1,
                        cursor.beat.get() + 1,
                        cursor.string
                    ));
                });
            }
        });
        if cancel {
            self.tasks.cancel_open();
        }
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
