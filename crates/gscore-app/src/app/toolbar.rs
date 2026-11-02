//! The toolbar: grouped controls on the chrome layer. On the left the file and edit groups
//! and the panel toggles; in the middle the player (transport, position and tempo); on the
//! right zoom and the settings. The output is chosen in the settings window, the shown
//! track in the song grid.

use egui::{Align, FontFamily, FontId, Layout, Rect, RichText, Sense, UiBuilder, vec2};
use gscore_edit::{Action, ActionId, Request};
use gscore_model::MeasureIndex;

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;
use super::tiles;

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
        ui.add_enabled_ui(enabled, |ui| icons::button(ui, icon, label, &tip))
            .inner
            .clicked()
    }

    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let bar = ui.max_rect();
        let height = ui.spacing().interact_size.y + 4.0;
        let row = Rect::from_min_size(bar.min, vec2(bar.width(), height));

        // Left: file, edit, panels.
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                self.file_and_edit(ui);
                self.panel_toggles(ui);
                if self.tasks.busy() {
                    ui.spinner();
                }
            },
        );

        // Right: settings rightmost, zoom before it (laid out right to left).
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if icons::button(ui, Icon::Settings, "Settings", "Settings…").clicked() {
                    self.open_settings_window();
                }
                self.zoom(ui);
            },
        );

        // Middle: the player, centered on the whole bar. Its width comes from the last
        // frame (egui lays out in one pass).
        let id = ui.scope_id().with("player_width");
        let width = ui
            .data(|data| data.get_temp::<f32>(id))
            .unwrap_or(PLAYER_WIDTH);
        let player = Rect::from_center_size(row.center(), vec2(width, height));
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
            if icons::button(ui, Icon::Open, "Open", "Open a Guitar Pro file…").clicked() {
                self.ask_open(ui.ctx());
            }
            let tip = format!("Save the file{}", self.key_hint(ActionId::Save));
            if icons::button(ui, Icon::Save, "Save", &tip).clicked() {
                self.request(Request::Save, ui.ctx());
            }
        });
        let (can_undo, can_redo) = self.document.as_ref().map_or((false, false), |d| {
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
        let has_song = self.document.is_some();
        let start = ui.cursor().left();
        style::toolbar_group(ui, |ui| {
            let (icon, label) = if playing {
                (Icon::Pause, "Pause")
            } else {
                (Icon::Play, "Play")
            };
            let tip = format!("{label}{}", self.key_hint(ActionId::PlayPause));
            let play = ui
                .add_enabled_ui(has_song, |ui| icons::button(ui, icon, label, &tip))
                .inner;
            if play.clicked() {
                self.play_pause();
            }
            let tip = format!("Stop{}", self.key_hint(ActionId::Stop));
            let stop = ui
                .add_enabled_ui(has_song, |ui| icons::button(ui, Icon::Stop, "Stop", &tip))
                .inner;
            if stop.clicked() {
                self.stop_playback();
            }
            self.position_readout(ui);
        });
        if self.playback.is_loading() {
            ui.label("Loading sounds…");
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
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

    /// "3 / 12" and "♩ = 120": the measure being played (or the cursor's) and its tempo.
    fn position_readout(&self, ui: &mut egui::Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let song = document.song();
        let measure = self
            .playback
            .current_measure()
            .unwrap_or_else(|| document.editor().cursor().measure.get().saturating_add(1));
        let tokens = style::tokens(ui);
        ui.add_space(4.0);
        // Painted, not a Label, so screen readers get "Measure 3" rather than "3 / 12".
        let label = format!("Measure {measure}");
        let text = format!("{measure} / {}", song.measure_count());
        let font = FontId::new(13.0, FontFamily::Name(crate::fonts::BOLD.into()));
        let galley = ui.painter().layout_no_wrap(text, font, tokens.text);
        let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
        ui.painter().galley(rect.min, galley, tokens.text);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
        response.on_hover_text("The measure being played, or the cursor's");
        // The tempo in effect at that measure.
        let index = measure.saturating_sub(1);
        let tempo = (0..=index)
            .rev()
            .find_map(|m| song.tempo_change(MeasureIndex::new(m)))
            .unwrap_or_else(|| song.tempo());
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
        .on_hover_text("Tempo at this measure");
        ui.add_space(6.0);
    }
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
