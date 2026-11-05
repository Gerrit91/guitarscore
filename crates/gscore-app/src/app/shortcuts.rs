//! The shortcut editor: a window with one row per action, its keys and buttons to change
//! or restore them. Clicking Change listens for the next key combination; a conflict asks
//! whether the other action's shortcut should be replaced. Shortcuts live in the app
//! settings (no keymap text file needed).

use egui::Margin;
use gscore_edit::{ACTIONS, Action, ActionId, Key, KeyCombo, Keymap, NamedKey};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;
use crate::keys::KeyReader;

/// The shortcut window's default size.
const SHORTCUT_SIZE: egui::Vec2 = egui::vec2(700.0, 580.0);

/// A captured shortcut waiting for the user to allow replacing another action's binding.
#[derive(Debug, Clone, Copy)]
struct PendingShortcut {
    action: ActionId,
    combo: KeyCombo,
    other: ActionId,
}

/// The open shortcut editor.
#[derive(Debug, Default)]
pub(super) struct ShortcutPanel {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// The table's filter text.
    filter: String,
    /// The row whose next key combination is being captured.
    capturing: Option<ActionId>,
    /// A captured key that another action already uses.
    conflict: Option<PendingShortcut>,
    /// Why the last typed shortcut was not accepted.
    error: Option<String>,
    /// Turns key events into key combinations (characters, named keys, modifiers).
    reader: KeyReader,
}

impl GuitarScoreApp {
    /// Opens (or focuses) the shortcut editor.
    pub(super) fn open_shortcuts(&mut self) {
        self.shortcuts.get_or_insert_with(ShortcutPanel::default);
    }

    /// The shortcut editor, if it is open.
    pub(super) fn show_shortcuts(&mut self, ctx: &egui::Context) {
        let Some(mut panel) = self.shortcuts.take() else {
            return;
        };
        if panel.pos.is_none() {
            panel.pos = Some(style::centered_window_pos(ctx, SHORTCUT_SIZE));
        }
        let pos = panel
            .pos
            .unwrap_or_else(|| style::centered_window_pos(ctx, SHORTCUT_SIZE));
        let defaults = Keymap::default();
        let mut close = false;
        let mut changed = false;
        let mut reset = false;
        let mut revert = None;
        let mut replace = false;
        egui::Window::new("Shortcuts")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(SHORTCUT_SIZE)
            .min_size([500.0, 340.0])
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "Shortcuts", &mut panel.pos) {
                    close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Filter");
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut panel.filter)
                                    .hint_text("command or key")
                                    .desired_width(200.0),
                            );
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_label("Filter shortcuts");
                            });
                            if ui.button("Reset all shortcuts").clicked() {
                                reset = true;
                            }
                        });
                        if let Some(action) = panel.capturing {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Press the new shortcut for “{}” (Escape cancels)…",
                                    action.label()
                                ))
                                .strong(),
                            );
                        }
                        if let Some(conflict) = panel.conflict {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0xF2, 0x55, 0x4D),
                                    format!(
                                        "“{combo}” is already used by “{other}”.",
                                        combo = conflict.combo,
                                        other = conflict.other.label()
                                    ),
                                );
                                if ui.button("Replace it").clicked() {
                                    replace = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    panel.conflict = None;
                                }
                            });
                        }
                        if let Some(error) = &panel.error {
                            ui.colored_label(egui::Color32::from_rgb(0xF2, 0x55, 0x4D), error);
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(440.0)
                            .show(ui, |ui| {
                                egui::Grid::new("shortcut_table")
                                    .num_columns(3)
                                    .spacing([14.0, 4.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.strong("Command");
                                        ui.strong("Shortcut");
                                        ui.label("");
                                        ui.end_row();
                                        let filter = panel.filter.to_lowercase();
                                        for info in ACTIONS {
                                            let action = info.action;
                                            let keys = self.keymap.keys_for(action);
                                            let key_text = keys
                                                .iter()
                                                .map(ToString::to_string)
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            let text = if action == ActionId::EnterFret {
                                                "0–9".to_owned()
                                            } else if key_text.is_empty() {
                                                "—".to_owned()
                                            } else {
                                                key_text.clone()
                                            };
                                            let haystack =
                                                format!("{} {} {}", info.label, info.id, key_text)
                                                    .to_lowercase();
                                            if !filter.is_empty() && !haystack.contains(&filter) {
                                                continue;
                                            }
                                            ui.label(info.label).on_hover_text(info.id);
                                            if panel.capturing == Some(action) {
                                                ui.label(
                                                    egui::RichText::new("Press a key…").weak(),
                                                );
                                            } else {
                                                ui.label(text);
                                            }
                                            ui.horizontal(|ui| {
                                                if panel.capturing == Some(action) {
                                                    if ui.button("Cancel").clicked() {
                                                        panel.capturing = None;
                                                    }
                                                } else if action != ActionId::EnterFret
                                                    && ui.button("Change").clicked()
                                                {
                                                    panel.capturing = Some(action);
                                                    panel.conflict = None;
                                                    panel.error = None;
                                                }
                                                if self.keymap.keys_for(action)
                                                    != defaults.keys_for(action)
                                                    && icons::button(
                                                        ui,
                                                        Icon::Revert,
                                                        "Revert shortcut",
                                                        "Restore the built-in shortcut",
                                                    )
                                                    .clicked()
                                                {
                                                    revert = Some(action);
                                                }
                                            });
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
            });
        if reset {
            self.keymap = Keymap::default();
            panel.capturing = None;
            panel.conflict = None;
            changed = true;
        }
        if let Some(action) = revert {
            self.keymap.revert_action(action);
            panel.capturing = None;
            panel.conflict = None;
            changed = true;
        }
        if replace && let Some(conflict) = panel.conflict.take() {
            match self.keymap.bind(conflict.action, conflict.combo) {
                Ok(()) => {
                    panel.error = None;
                    changed = true;
                }
                Err(error) => panel.error = Some(error),
            }
        }
        let was_capturing = panel.capturing.is_some();
        if !was_capturing && !close && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if let Some(action) = panel.capturing {
            let events = ctx.input(|input| input.events.clone());
            for event in events {
                let Some(combo) = panel.reader.combo(&event) else {
                    continue;
                };
                if matches!(combo.key(), Key::Named(NamedKey::Escape)) {
                    panel.capturing = None;
                    break;
                }
                let other = bound_action(&self.keymap, combo).filter(|bound| *bound != action);
                if let Some(other) = other {
                    panel.conflict = Some(PendingShortcut {
                        action,
                        combo,
                        other,
                    });
                    panel.capturing = None;
                } else {
                    match self.keymap.bind(action, combo) {
                        Ok(()) => {
                            panel.capturing = None;
                            panel.error = None;
                            changed = true;
                        }
                        Err(error) => {
                            panel.capturing = None;
                            panel.error = Some(error);
                        }
                    }
                }
                break;
            }
        }
        if changed {
            self.save_settings();
        }
        if close {
            self.shortcuts = None;
            return;
        }
        self.shortcuts = Some(panel);
    }
}

/// The action a key combination is currently bound to, if any.
fn bound_action(keymap: &Keymap, combo: KeyCombo) -> Option<ActionId> {
    match keymap.action(combo)? {
        Action::Plain(action) => Some(action),
        Action::Digit(_) => Some(ActionId::EnterFret),
    }
}
