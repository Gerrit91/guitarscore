//! Menus, the status bar and the notice bar (the toolbar is in `toolbar.rs`).

use egui::{Align, Layout as UiLayout};
use gscore_edit::{ActionId, Request};

use super::GuitarScoreApp;
use super::page_menu::{self, Section};
use crate::state::ThemeChoice;

impl GuitarScoreApp {
    /// A menu entry for `action`, with its keys from the keymap as the shortcut text.
    fn action_button(&self, ui: &mut egui::Ui, action: ActionId, enabled: bool) -> bool {
        // A menu shows one key, as menus do (zoom in has ctrl++ and ctrl+=, one key with
        // and without Shift); the tooltips and the shortcut list show them all.
        let key = self
            .keymap
            .keys_for(action)
            .first()
            .map(ToString::to_string)
            .unwrap_or_default();
        let button = egui::Button::new(action.label()).shortcut_text(key);
        ui.add_enabled(enabled, button).clicked()
    }

    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut choices = Vec::new();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if self.action_button(ui, ActionId::NewSong, true) {
                    self.new_song(ui.ctx());
                    ui.close();
                }
                if self.action_button(ui, ActionId::OpenFile, true) {
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
                if self.action_button(ui, ActionId::SaveAs, has_document) {
                    self.save_as_active(ui.ctx());
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
                if self.action_button(ui, ActionId::OpenSettings, true) {
                    self.open_settings_window();
                    ui.close();
                }
                // A browser tab is closed by the browser, not from the page.
                if cfg!(not(target_arch = "wasm32")) {
                    ui.separator();
                    if self.action_button(ui, ActionId::Quit, true) {
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
                if self.action_button(ui, ActionId::ShowHistory, true) {
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
                if ui
                    .button("Reset panel sizes")
                    .on_hover_text("The side panels and the song grid go back to their usual sizes")
                    .clicked()
                {
                    self.reset_panel_sizes();
                    ui.close();
                }
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
                for action in [ActionId::ZoomIn, ActionId::ZoomOut, ActionId::ZoomReset] {
                    if self.action_button(ui, action, true) {
                        self.perform_action(gscore_edit::Action::Plain(action), ui.ctx());
                    }
                }
                ui.separator();
                // The whole interface: Ctrl+Shift with +, - and 0 (read in
                // `interface_zoom_keys`, not from the keymap).
                if ui
                    .add(egui::Button::new("Larger interface").shortcut_text("ctrl+shift++"))
                    .on_hover_text("Menus, panels and icons larger")
                    .clicked()
                {
                    self.view.interface_larger();
                }
                if ui
                    .add(egui::Button::new("Smaller interface").shortcut_text("ctrl+shift+-"))
                    .on_hover_text("Menus, panels and icons smaller")
                    .clicked()
                {
                    self.view.interface_smaller();
                }
                if ui
                    .add(egui::Button::new("Normal interface size").shortcut_text("ctrl+shift+0"))
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
                        if let Some(notice) = &self.notice {
                            ui.separator();
                            notice_in_status(ui, notice);
                        }
                    } else {
                        ui.label("No file open");
                        if self.tasks.busy() {
                            ui.add(egui::Spinner::new().size(14.0))
                                .on_hover_text("Working…");
                        }
                        if let Some(notice) = &self.notice {
                            ui.separator();
                            notice_in_status(ui, notice);
                        }
                    }
                }
            }
            // The audio load in the corner, then the open document's counts, page and
            // cursor, on the right.
            let load = self.playback.audio_load();
            let rate = self.playback.device_sample_rate();
            if self.document().is_none() && load.is_some() {
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    audio_load(ui, load, rate);
                });
            }
            if let Some(document) = self.document() {
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    if audio_load(ui, load, rate) {
                        ui.separator();
                    }
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
        // A notice goes away by itself after a while (its button dismisses it sooner); the
        // status bar keeps showing it meanwhile.
        let now = ui.input(|input| input.time);
        if now - self.notice_since > NOTICE_SECONDS {
            self.notice = None;
            self.notice_shown = None;
            return;
        }
        // The button stays at the right edge and a long message wraps, so the button is
        // never pushed out of the window.
        ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
            if ui.small_button("Dismiss").clicked() {
                self.notice = None;
            }
            ui.with_layout(
                UiLayout::left_to_right(Align::Center).with_main_wrap(true),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(notice).color(ui.visuals().error_fg_color),
                        )
                        .wrap(),
                    );
                },
            );
        });
    }
}

/// How long a notice stays before it goes away by itself, in seconds.
const NOTICE_SECONDS: f64 = 15.0;

/// A notice in the status bar, beside the working spinner and the file name: red, kept to
/// one line (hover for the whole text).
fn notice_in_status(ui: &mut egui::Ui, notice: &str) {
    let response = ui.add(
        egui::Label::new(egui::RichText::new(notice).color(ui.visuals().error_fg_color)).truncate(),
    );
    response.on_hover_text(notice);
}

/// "1 track", "3 tracks".
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// From this audio load the CPU bar turns red.
const OVERLOAD: f32 = 0.9;

/// The built-in synthesizer's audio load (`load`, 1 = all of each buffer's time at
/// `rate`), if it runs: a small green bar filling from the bottom, red from [`OVERLOAD`];
/// hovering names it.
/// Returns whether it was shown.
fn audio_load(ui: &mut egui::Ui, load: Option<f32>, rate: Option<u32>) -> bool {
    let Some(load) = load.filter(|load| load.is_finite()) else {
        return false;
    };
    let size = egui::vec2(5.0, 14.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
    let share = load.clamp(0.0, 1.0);
    let filled = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - rect.height() * share),
        rect.right_bottom(),
    );
    // Red from 90 %, where the sound is close to dropping out.
    let color = if load >= OVERLOAD {
        ui.visuals().error_fg_color
    } else {
        egui::Color32::from_rgb(0x4C, 0xAF, 0x50)
    };
    painter.rect_filled(filled, 1.0, color);
    let percent = (load * 100.0).round().clamp(0.0, 999.0);
    let rate = rate.map_or_else(String::new, |rate| {
        format!(" at {} kHz", f64::from(rate) / 1000.0)
    });
    let id = response.id;
    response.on_hover_text(format!(
        "Audio CPU: {percent:.0} %{rate}. The share of each audio buffer's time the \
         built-in synthesizer needs to compute it; near 100 % the sound drops out."
    ));
    ui.ctx()
        .accesskit_node_builder(id, |node| node.set_label("Audio CPU"));
    // Keep the bar current.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(500));
    true
}
