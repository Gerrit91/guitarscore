//! The history window: the undo/redo history as a table, newest first. Any row's icon brings
//! the song to that point in time (undoing or redoing as many steps as needed), so the edit
//! history is visible instead of hidden behind two shortcuts.

use crate::{SystemTime, UNIX_EPOCH};

use egui::{Align2, FontId, Margin, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2};
use gscore_edit::{Action, ActionId};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;

/// The history window's default size.
const HISTORY_SIZE: egui::Vec2 = vec2(820.0, 560.0);
/// Height of the header and of each row.
const ROW_HEIGHT: f32 = 24.0;
/// Width of the leading column with the go-to icon.
const GOTO_WIDTH: f32 = 34.0;
/// Space at the left of each text cell.
const CELL_PAD: f32 = 6.0;
/// Narrowest a resizable column may be dragged.
const MIN_COLUMN: f32 = 40.0;
/// Narrowest the Details column (which takes the remaining width) gets.
const MIN_DETAILS: f32 = 80.0;
/// The resizable columns' default widths: time, change, state.
const DEFAULT_WIDTHS: [f32; 3] = [76.0, 170.0, 74.0];
/// The column titles after the go-to column.
const TITLES: [&str; 4] = ["Time", "Change", "State", "Details"];

/// The open history window.
#[derive(Debug)]
pub(super) struct HistoryPanel {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// The widths of the time, change and state columns, as dragged.
    widths: [f32; 3],
}

impl Default for HistoryPanel {
    fn default() -> Self {
        HistoryPanel {
            pos: None,
            widths: DEFAULT_WIDTHS,
        }
    }
}

/// Where a row is relative to the song's current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The song is at this point now.
    Current,
    /// An applied change before the current one: going there undoes this many steps.
    Earlier(usize),
    /// An undone change: going there redoes this many steps.
    Later(usize),
}

/// One row of the table.
struct Row {
    label: &'static str,
    details: String,
    stamp: Option<SystemTime>,
    place: Place,
    /// Still applied (false for an undone change).
    done: bool,
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
        let pos = *panel
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, HISTORY_SIZE));
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rows = self.history_rows(tab);
        let mut close = false;
        let mut target = None;
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
                close |= style::dialog_body(ui, Margin::symmetric(14, 10), |ui| {
                    ui.label(
                        RichText::new(
                            "Newest first (times are UTC). The clock on a row brings the \
                             song back to that point; every change is also recorded in the \
                             recovery journal.",
                        )
                        .weak(),
                    );
                    ui.add_space(6.0);
                    target = table(ui, &mut panel.widths, &rows);
                });
            });
        if close {
            return;
        }
        self.history = Some(panel);
        match target {
            Some(Place::Earlier(steps)) => {
                for _ in 0..steps {
                    self.perform_action(Action::Plain(ActionId::Undo), ctx);
                }
            }
            Some(Place::Later(steps)) => {
                for _ in 0..steps {
                    self.perform_action(Action::Plain(ActionId::Redo), ctx);
                }
            }
            Some(Place::Current) | None => {}
        }
    }

    /// The table's rows, newest first: the undone changes (the furthest one first), the
    /// applied ones, then the song as it was opened.
    fn history_rows(&self, tab: &super::tab::Tab) -> Vec<Row> {
        let undo = tab.document.editor().undo_history();
        let redo = tab.document.editor().redo_history();
        let mut rows = Vec::with_capacity(undo.len().saturating_add(redo.len()).saturating_add(1));
        for (index, entry) in redo.iter().enumerate().rev() {
            rows.push(Row {
                label: entry.label,
                details: entry.details.clone(),
                stamp: tab.history_stamps.get(&entry.id).copied(),
                place: Place::Later(index.saturating_add(1)),
                done: false,
            });
        }
        let newest = undo.len().saturating_sub(1);
        for (index, entry) in undo.iter().enumerate().rev() {
            let steps = newest.saturating_sub(index);
            rows.push(Row {
                label: entry.label,
                details: entry.details.clone(),
                stamp: tab.history_stamps.get(&entry.id).copied(),
                place: if steps == 0 {
                    Place::Current
                } else {
                    Place::Earlier(steps)
                },
                done: true,
            });
        }
        rows.push(Row {
            label: "Start",
            details: "The song as it was opened (or as far back as the history goes)".to_owned(),
            stamp: None,
            place: if undo.is_empty() {
                Place::Current
            } else {
                Place::Earlier(undo.len())
            },
            done: true,
        });
        rows
    }
}

/// The x ranges of the go-to column and the four text columns of a row at `row`.
fn columns(row: Rect, widths: &[f32; 3]) -> [Rect; 5] {
    let mut x = row.left();
    let mut cell = |width: f32| {
        let rect = Rect::from_min_max(pos2(x, row.top()), pos2(x + width, row.bottom()));
        x += width;
        rect
    };
    let [time_width, change_width, state_width] = *widths;
    let goto = cell(GOTO_WIDTH);
    let time = cell(time_width);
    let change = cell(change_width);
    let state = cell(state_width);
    let details = Rect::from_min_max(
        pos2(x, row.top()),
        pos2(row.right().max(x + MIN_DETAILS), row.bottom()),
    );
    [goto, time, change, state, details]
}

/// The header with its column handles and the rows below it. Returns where the user asked
/// to go.
fn table(ui: &mut egui::Ui, widths: &mut [f32; 3], rows: &[Row]) -> Option<Place> {
    let visuals = ui.visuals().clone();
    let width = ui.available_width();
    let (header, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let cells = columns(header, widths);
    for (cell, title) in cells.iter().skip(1).zip(TITLES) {
        ui.painter().text(
            pos2(cell.left() + CELL_PAD, cell.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(12.0),
            visuals.strong_text_color(),
        );
    }
    // A drag handle at the right edge of each resizable column; Details takes the rest.
    let line = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4));
    for (index, cell) in cells.iter().skip(1).take(3).enumerate() {
        let handle = Rect::from_center_size(
            pos2(cell.right(), header.center().y),
            vec2(8.0, header.height()),
        );
        let response = ui.interact(
            handle,
            ui.scope_id().with(("history_column", index)),
            Sense::drag(),
        );
        let active = response.hovered() || response.dragged();
        if active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        }
        if response.dragged()
            && let Some(column) = widths.get_mut(index)
        {
            *column = (*column + response.drag_delta().x).max(MIN_COLUMN);
        }
        let stroke = if active {
            Stroke::new(1.5, visuals.selection.bg_fill)
        } else {
            line
        };
        ui.painter().line_segment(
            [
                pos2(cell.right(), header.top() + 4.0),
                pos2(cell.right(), header.bottom() - 4.0),
            ],
            stroke,
        );
        response.on_hover_text("Drag to resize the column");
    }
    ui.painter().line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.6)),
    );

    let mut target = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
            for index in range {
                let Some(row) = rows.get(index) else {
                    continue;
                };
                if let Some(place) = table_row(ui, widths, index, row) {
                    target = Some(place);
                }
            }
        });
    target
}

/// One row; returns its place when its go-to icon was clicked.
fn table_row(ui: &mut egui::Ui, widths: &[f32; 3], index: usize, row: &Row) -> Option<Place> {
    let visuals = ui.visuals().clone();
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    if row.place == Place::Current {
        ui.painter()
            .rect_filled(rect, 3.0, visuals.selection.bg_fill.gamma_multiply(0.22));
    } else if index % 2 == 1 {
        ui.painter().rect_filled(rect, 0.0, visuals.faint_bg_color);
    }
    let [goto, time, change, state, details] = columns(rect, widths);
    let number = index.saturating_add(1);
    let mut clicked = None;
    if row.place == Place::Current {
        let icon = Rect::from_center_size(goto.center(), vec2(14.0, 10.0));
        icons::paint(ui.painter(), icon, visuals.selection.bg_fill, Icon::History);
        let marker = ui.interact(
            goto,
            ui.scope_id().with(("history_now", index)),
            Sense::hover(),
        );
        ui.ctx().accesskit_node_builder(marker.id, |node| {
            node.set_label(format!("Current point, change {number}: {}", row.label));
        });
        marker.on_hover_text("The song is at this point now");
    } else {
        let cell = Rect::from_center_size(goto.center(), vec2(26.0, 22.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
        let label = format!("Go to change {number}: {}", row.label);
        if icons::button(
            &mut child,
            Icon::History,
            &label,
            "Go to this point in time",
        )
        .clicked()
        {
            clicked = Some(row.place);
        }
    }
    let weak = visuals.weak_text_color();
    let text = visuals.text_color();
    let state_text = match (row.place, row.done) {
        (Place::Current, _) => "current",
        (_, true) => "done",
        (_, false) => "undone",
    };
    let undone = |color: egui::Color32| if row.done { color } else { weak };
    for (cell, content, color) in [
        (time, stamp_text(row.stamp), weak),
        (change, row.label.to_owned(), undone(text)),
        (state, state_text.to_owned(), weak),
        (details, row.details.clone(), undone(text)),
    ] {
        let clip = cell.shrink2(vec2(CELL_PAD / 2.0, 0.0));
        ui.painter().with_clip_rect(clip).text(
            pos2(cell.left() + CELL_PAD, cell.center().y),
            Align2::LEFT_CENTER,
            content,
            FontId::proportional(12.0),
            color,
        );
    }
    // The whole details, also when the column cuts them off.
    if !row.details.is_empty() && response.hovered() {
        let hover = ui.interact(
            details,
            ui.scope_id().with(("history_details", index)),
            Sense::hover(),
        );
        hover.on_hover_text(&row.details);
    }
    clicked
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

    #[test]
    fn columns_follow_the_widths_and_details_take_the_rest() {
        let row = Rect::from_min_size(pos2(10.0, 0.0), vec2(600.0, ROW_HEIGHT));
        let [goto, time, change, state, details] = columns(row, &[50.0, 100.0, 60.0]);
        assert_eq!(goto.left(), 10.0);
        assert_eq!(time.left(), goto.right());
        assert_eq!(time.width(), 50.0);
        assert_eq!(change.width(), 100.0);
        assert_eq!(state.right(), details.left());
        assert_eq!(details.right(), row.right());
        // A narrow window still leaves Details its minimum.
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, ROW_HEIGHT));
        let [.., details] = columns(narrow, &[50.0, 100.0, 60.0]);
        assert_eq!(details.width(), MIN_DETAILS);
    }
}
