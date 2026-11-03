//! The history window: the undo/redo history as a table, newest first. A row can be undone
//! or redone from here, so the edit history is visible instead of hidden behind two
//! shortcuts.

use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Margin, RichText};
use gscore_edit::{Action, ActionId};

use super::GuitarScoreApp;
use super::style;

/// The history window's default size.
const HISTORY_SIZE: egui::Vec2 = egui::vec2(820.0, 560.0);

/// The open history window.
#[derive(Debug, Default)]
pub(super) struct HistoryPanel {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
}

/// One row of the table.
struct Row {
    label: &'static str,
    details: String,
    stamp: Option<SystemTime>,
    /// The entry is still applied (undo it) or undone (redo it).
    undone: bool,
    /// How many undo steps reach this row.
    undo_steps: usize,
    /// How many redo steps reach this row.
    redo_steps: usize,
}

impl GuitarScoreApp {
    /// Opens (or focuses) the edit history (also used by the snapshot example).
    pub fn open_history(&mut self) {
        self.history.get_or_insert_with(HistoryPanel::default);
    }

    /// The history window, if it is open.
    pub(super) fn show_history(&mut self, ctx: &egui::Context) {
        let Some(mut panel) = self.history.take() else {
            return;
        };
        if panel.pos.is_none() {
            panel.pos = Some(style::centered_window_pos(ctx, HISTORY_SIZE));
        }
        let pos = panel
            .pos
            .unwrap_or_else(|| style::centered_window_pos(ctx, HISTORY_SIZE));
        let Some(document) = &self.document else {
            self.history = None;
            return;
        };
        let undo = document.editor().undo_history();
        let redo = document.editor().redo_history();
        let mut rows: Vec<Row> = Vec::with_capacity(undo.len().saturating_add(redo.len()));
        for (index, entry) in undo.iter().enumerate() {
            rows.push(Row {
                label: entry.label,
                details: ellipsize(&entry.details, 64),
                stamp: self.history_stamps.get(&entry.id).copied(),
                undone: false,
                undo_steps: undo.len().saturating_sub(index),
                redo_steps: 0,
            });
        }
        for (index, entry) in redo.iter().enumerate() {
            rows.push(Row {
                label: entry.label,
                details: ellipsize(&entry.details, 64),
                stamp: self.history_stamps.get(&entry.id).copied(),
                undone: true,
                undo_steps: 0,
                redo_steps: index.saturating_add(1),
            });
        }
        // The most recent action first; unknown times go last.
        rows.sort_by_key(|row| std::cmp::Reverse(row.stamp));
        let mut close = false;
        let mut undo_steps = 0_usize;
        let mut redo_steps = 0_usize;
        egui::Window::new("History")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(HISTORY_SIZE)
            .min_size([520.0, 320.0])
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "History", &mut panel.pos) {
                    close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Newest first (times are UTC). Undo or redo from any row; \
                                 every change is also recorded in the recovery journal.",
                            )
                            .weak(),
                        );
                        if rows.is_empty() {
                            ui.add_space(12.0);
                            ui.label("No changes yet.");
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(430.0)
                            .show(ui, |ui| {
                                egui::Grid::new("history_table")
                                    .num_columns(5)
                                    .spacing([12.0, 4.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.strong("Time");
                                        ui.strong("Change");
                                        ui.strong("Details");
                                        ui.strong("State");
                                        ui.label("");
                                        ui.end_row();
                                        for (index, row) in rows.iter().enumerate() {
                                            ui.label(stamp_text(row.stamp));
                                            ui.label(row.label);
                                            ui.add(egui::Label::new(&row.details).truncate());
                                            if row.undone {
                                                ui.label(RichText::new("undone").weak());
                                            } else {
                                                ui.label("done");
                                            }
                                            if row.undone {
                                                let response = ui.button("Redo");
                                                ui.ctx().accesskit_node_builder(
                                                    response.id,
                                                    |node| {
                                                        node.set_label(format!(
                                                            "Redo change {}: {}",
                                                            index.saturating_add(1),
                                                            row.label
                                                        ));
                                                    },
                                                );
                                                if response.clicked() {
                                                    redo_steps = row.redo_steps;
                                                }
                                            } else {
                                                let response = ui.button("Undo");
                                                ui.ctx().accesskit_node_builder(
                                                    response.id,
                                                    |node| {
                                                        node.set_label(format!(
                                                            "Undo change {}: {}",
                                                            index.saturating_add(1),
                                                            row.label
                                                        ));
                                                    },
                                                );
                                                if response.clicked() {
                                                    undo_steps = row.undo_steps;
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
            });
        if close {
            self.history = None;
            return;
        }
        self.history = Some(panel);
        for _ in 0..undo_steps {
            self.perform_action(Action::Plain(ActionId::Undo), ctx);
        }
        for _ in 0..redo_steps {
            self.perform_action(Action::Plain(ActionId::Redo), ctx);
        }
    }
}

/// `text`, cut to at most `max` characters with an ellipsis.
fn ellipsize(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// `HH:MM:SS` in UTC, or a dash when the entry's time is unknown.
fn stamp_text(stamp: Option<SystemTime>) -> String {
    let Some(stamp) = stamp else {
        return "—".to_owned();
    };
    let seconds = stamp
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let day = seconds % 86_400;
    format!("{:02}:{:02}:{:02}", day / 3_600, day % 3_600 / 60, day % 60)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn timestamps_are_written_as_utc_clock_time() {
        let epoch = UNIX_EPOCH + Duration::from_secs(3_661);
        assert_eq!(stamp_text(Some(epoch)), "01:01:01");
        assert_eq!(stamp_text(None), "—");
    }
}
