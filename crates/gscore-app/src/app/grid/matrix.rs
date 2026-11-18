//! The measure matrix of the song grid: the ruler with measure numbers, section markers,
//! repeats and alternative endings, and one row of squares per track. Clicking puts the
//! cursor on a bar's first beat; dragging selects measures; the playing measure is highlighted.

use egui::{Align2, FontId, Rect, Response, Sense, Stroke, pos2, vec2};

use crate::app::style;
use crate::document::GridData;
use crate::state::MeasureSelection;

use super::super::page::SELECTION_ALPHA;
use super::rows::{TrackRow, color32};
use super::{CELL_PADDING, GridAction, GridView, MEASURE_WIDTH, ROW_HEIGHT, RULER_HEIGHT, mixer};

/// The width of the lines between the grid's tiles.
const GRID_LINE_WIDTH: f32 = 1.0;

/// How strongly the playing measure is tinted in the score's playing cursor color.
const PLAYING_ALPHA: f32 = 0.25;

/// The measure ruler above the tracks: measure numbers, and the sections below them (the
/// repeats and alternative endings are in the master row). It stays at the top while the tracks scroll under it, and follows
/// the matrix sideways by `scroll_x`, and is as wide as the measures the matrix shows
/// (`width`, when known), so it ends where the matrix's vertical scroll bar starts.
pub(super) fn ruler(
    ui: &mut egui::Ui,
    grid: &GridData,
    view: GridView,
    scroll_x: f32,
    width: Option<f32>,
    actions: &mut Vec<GridAction>,
) {
    let mw = MEASURE_WIDTH * view.zoom;
    // Never wider than the panel, and never as wide as the song: a ruler wider than the
    // window would widen the panel's contents, and the matrix below with them, past its
    // scroll bar.
    let available = ui.available_width();
    let width = width.map_or(available, |width| width.min(available));
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, RULER_HEIGHT), Sense::click_and_drag());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label("Measure ruler");
        node.set_role(egui::Role::Group);
    });
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 0.0, style::tokens(ui).grid);
    // The measures the strip shows: the matrix's sideways scroll decides where it starts.
    let left = rect.left() - scroll_x;
    let first = (scroll_x / mw).floor().max(0.0) as usize;
    let last = (((scroll_x + rect.width()) / mw).ceil() as usize).min(view.measure_count);
    let x_of = |measure: usize| left + measure as f32 * mw;
    let column = |measure: usize| {
        Rect::from_min_max(
            pos2(x_of(measure), rect.top()),
            pos2(x_of(measure) + mw, rect.bottom()),
        )
    };
    // The playing measure and the selection (in the score's color and shade), as in the
    // matrix below.
    if let Some(playing) = view.playing
        && playing >= first
        && playing < last
    {
        painter.rect_filled(
            column(playing),
            0.0,
            view.cursor_color.gamma_multiply(PLAYING_ALPHA),
        );
    }
    if let Some(selection) = view.shown_selection()
        && last > first
    {
        let start = selection.start().max(first);
        let end = selection.end().min(last.saturating_sub(1));
        for measure in start..=end {
            painter.rect_filled(
                column(measure),
                0.0,
                view.selection_color.gamma_multiply(SELECTION_ALPHA),
            );
        }
    }
    let number_font = FontId::monospace(9.0);
    for measure in first..last {
        if measure % 4 == 0 || view.measure_count <= 64 {
            painter.text(
                pos2(x_of(measure) + 2.0, rect.top() + 2.0),
                Align2::LEFT_TOP,
                (measure.saturating_add(1)).to_string(),
                number_font.clone(),
                visuals.weak_text_color(),
            );
        }
    }
    let band = Rect::from_min_max(pos2(rect.left(), rect.top() + NUMBERS_HEIGHT), rect.max);
    sections(ui, &painter, &response, band, left, grid, view, actions);
    // A click on the measure numbers (or a bar before the first section) puts the cursor on
    // the bar (in the cursor's track); shift-click selects whole bars across every track,
    // growing the selection (or from the cursor's bar).
    let at_section = |pointer: egui::Pos2| {
        pointer.y >= band.top()
            && measure_at(pointer.x - left, mw, view.measure_count)
                .and_then(|measure| section_span(grid, view.measure_count, measure))
                .is_some()
    };
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && !at_section(pointer)
        && let Some(measure) = measure_at(pointer.x - left, mw, view.measure_count)
    {
        if ui.input(|input| input.modifiers.shift) {
            let mut selection = view
                .selection
                .unwrap_or_else(|| MeasureSelection::new(view.cursor_measure))
                .extend_to(measure);
            selection.track = None;
            actions.push(GridAction::Select(selection));
        } else {
            actions.push(GridAction::Place {
                track: view.cursor_track,
                measure,
            });
        }
    }
}

/// The height of the ruler's measure numbers, above the sections' band.
const NUMBERS_HEIGHT: f32 = 12.0;

/// The measure `x` (from the first measure's left edge) falls in, if any.
fn measure_at(x: f32, mw: f32, measure_count: usize) -> Option<usize> {
    if x < 0.0 {
        return None;
    }
    let measure = (x / mw) as usize;
    (measure < measure_count).then_some(measure)
}

/// The sections in the ruler's lower `band`: each one's start marker, a line along its
/// bars and its title. A click on a section selects its bars in every track (as the track
/// panel's section rows do); dragging it onto another section swaps their bars.
#[allow(clippy::too_many_arguments)]
fn sections(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &Response,
    band: Rect,
    left: f32,
    grid: &GridData,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let mw = MEASURE_WIDTH * view.zoom;
    let x_of = |measure: usize| left + measure as f32 * mw;
    let first = measure_at(band.left() - left, mw, view.measure_count).unwrap_or(0);
    let last = measure_at(band.right() - left, mw, view.measure_count)
        .map_or(view.measure_count, |measure| measure.saturating_add(1));

    // While a section is dragged, it and the section under the pointer are marked:
    // dropping it there swaps the two sections' bars.
    let drag_id = ui.make_persistent_id("section_drag");
    let drag: Option<(usize, Option<usize>)> = ui.ctx().data(|data| data.get_temp(drag_id));
    if let Some((from, target)) = drag {
        for (start, emphasis) in [(from, true), (target.unwrap_or(from), false)] {
            if !emphasis && target.is_none() {
                continue;
            }
            let Some((first, last)) = section_span(grid, view.measure_count, start) else {
                continue;
            };
            let area = Rect::from_min_max(
                pos2(x_of(first), band.top()),
                pos2(x_of(last) + mw, band.bottom()),
            );
            painter.rect_filled(
                area,
                2.0,
                view.selection_color
                    .gamma_multiply(if emphasis { 0.3 } else { 0.15 }),
            );
            painter.rect_stroke(
                area,
                2.0,
                Stroke::new(1.5, view.selection_color),
                egui::StrokeKind::Inside,
            );
        }
    }

    // A section lasts until the next one starts (or the song ends), so it also covers
    // measures on screen when its own start has scrolled off the left edge: such a
    // half-visible section keeps its line there and shows its title at the edge.
    let starts: Vec<usize> = (0..view.measure_count)
        .filter(|&measure| grid.sections.get(measure).is_some_and(Option::is_some))
        .collect();
    for (index, &measure) in starts.iter().enumerate() {
        let Some(Some(section)) = grid.sections.get(measure) else {
            continue;
        };
        let next = starts
            .get(index.saturating_add(1))
            .copied()
            .unwrap_or(view.measure_count);
        if next <= first || measure >= last {
            continue;
        }
        let x = x_of(measure);
        let color = color32(section.color);
        let room = next.saturating_sub(measure) as f32 * mw;
        // The title may run into the empty area after the last measure; it stops at the
        // next section's marker.
        let end_x = if next >= view.measure_count {
            band.right()
        } else {
            x + room
        };
        let title_x = x.max(band.left());
        painter.line_segment(
            [pos2(x, band.top()), pos2(x, band.bottom() - 2.0)],
            Stroke::new(2.0, color),
        );
        painter.line_segment(
            [
                pos2(x, band.bottom() - 2.0),
                pos2(x + room, band.bottom() - 2.0),
            ],
            Stroke::new(1.5, color),
        );
        if end_x - title_x >= mw {
            // Clip the title to the measures before the next section, so two markers
            // never write over each other.
            let clip = Rect::from_min_max(
                pos2(title_x + 3.0, band.top()),
                pos2(end_x - 2.0, band.bottom()),
            );
            painter.with_clip_rect(clip.intersect(band)).text(
                pos2(title_x + 4.0, band.center().y - 1.0),
                Align2::LEFT_CENTER,
                &section.title,
                FontId::proportional(10.0),
                color,
            );
        }
    }

    let at = response
        .interact_pointer_pos()
        .and_then(|pointer| measure_at(pointer.x - left, mw, view.measure_count));
    let in_band = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos())
        .is_some_and(|pointer| band.contains(pointer));
    let pressed_in_band = ui
        .input(|input| input.pointer.press_origin())
        .is_some_and(|origin| band.contains(origin));
    let section_at =
        |measure: usize| section_span(grid, view.measure_count, measure).map(|(start, _)| start);

    if in_band
        && response.clicked()
        && let Some(measure) = at
        && let Some((start, end)) = section_span(grid, view.measure_count, measure)
    {
        // Shift-click grows the selection to take in the whole section.
        let selection = if ui.input(|input| input.modifiers.shift) {
            let mut grown = view
                .selection
                .unwrap_or_else(|| MeasureSelection::new(start))
                .extend_to(start)
                .extend_to(end);
            grown.track = None;
            grown
        } else {
            MeasureSelection {
                anchor: start,
                focus: end,
                track: None,
            }
        };
        actions.push(GridAction::Select(selection));
        actions.push(GridAction::MoveCursor {
            track: view.cursor_track,
            measure: start,
        });
    }
    let hovered = response
        .hover_pos()
        .filter(|pointer| band.contains(*pointer))
        .and_then(|pointer| measure_at(pointer.x - left, mw, view.measure_count));
    if !response.dragged() && hovered.and_then(section_at).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        response.clone().on_hover_text(
            "Click to select the section (every track); drag it onto another section to \
             swap their bars",
        );
    }
    // The drag may start only after the pointer has left the section's name, so the
    // section is the one under the press, not under the pointer now.
    let pressed_at = ui
        .input(|input| input.pointer.press_origin())
        .and_then(|origin| measure_at(origin.x - left, mw, view.measure_count));
    if pressed_in_band
        && response.drag_started()
        && let Some(from) = pressed_at.and_then(section_at)
    {
        // Only the section being dragged is marked: the selection goes.
        actions.push(GridAction::ClearSelection);
        ui.ctx()
            .data_mut(|data| data.insert_temp(drag_id, (from, None::<usize>)));
    }
    if let Some((from, _)) = drag {
        let target = at.and_then(section_at).filter(|start| *start != from);
        if response.dragged() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(drag_id, (from, target)));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if response.drag_stopped() {
            if let Some(to) = target {
                actions.push(GridAction::SwapSections { from, to });
            }
            ui.ctx()
                .data_mut(|data| data.remove_temp::<(usize, Option<usize>)>(drag_id));
        }
    }
}

/// The measure matrix: one row of squares per track and the master row's line. The ruler
/// above it is drawn by [`ruler`], which stays while these rows scroll.
pub(super) fn matrix(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    grid: &GridData,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let mw = MEASURE_WIDTH * view.zoom;
    let width = view.measure_count as f32 * mw;
    // One row more than there are tracks: the master row's line, so the matrix ends where
    // the track column does.
    let height = (rows.len() as f32 + 1.0) * ROW_HEIGHT;
    let available = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(vec2(width.max(available), height), Sense::click_and_drag());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label("Song grid");
        node.set_role(egui::Role::Group);
    });
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 0.0, style::tokens(ui).grid);

    // Paint only the measures and rows the scroll area shows.
    let shown = ui.clip_rect().intersect(rect);
    let first_row = ((shown.top() - rect.top()) / ROW_HEIGHT).floor().max(0.0) as usize;
    let last_row = (((shown.bottom() - rect.top()) / ROW_HEIGHT).ceil() as usize).min(rows.len());
    let first_measure = ((shown.left() - rect.left()) / mw).floor().max(0.0) as usize;
    let last_measure =
        (((shown.right() - rect.left()) / mw).ceil() as usize).min(view.measure_count);

    // The playing measure, behind the squares.
    if let Some(playing) = view.playing
        && playing >= first_measure
        && playing < last_measure
    {
        let column = measure_rect(rect, playing, view.zoom);
        painter.rect_filled(column, 0.0, view.cursor_color.gamma_multiply(PLAYING_ALPHA));
    }

    // Row stripes (a setting) and the squares of the bars with notes.
    for row in first_row..last_row {
        let Some(cells) = grid.cells.get(row) else {
            continue;
        };
        let row_rect = Rect::from_min_max(
            pos2(rect.left(), rect.top() + row as f32 * ROW_HEIGHT),
            pos2(
                rect.left() + rect.width(),
                rect.top() + (row as f32 + 1.0) * ROW_HEIGHT,
            ),
        );
        if view.stripes && row % 2 == 1 {
            painter.rect_filled(row_rect, 0.0, style::tokens(ui).grid_stripe);
        }
        for measure_index in first_measure..last_measure {
            if !cells.get(measure_index).copied().unwrap_or(false) {
                continue;
            }
            let cell = Rect::from_min_size(
                pos2(
                    rect.left() + measure_index as f32 * mw + CELL_PADDING,
                    row_rect.top() + CELL_PADDING,
                ),
                vec2(mw - 2.0 * CELL_PADDING, ROW_HEIGHT - 2.0 * CELL_PADDING),
            );
            let color = rows.get(row).map_or(visuals.text_color(), |track| {
                mixer::sounding_color(color32(track.color), track.audible)
            });
            painter.rect_filled(cell, 0.0, color);
        }
    }

    // The tile lines, over the squares so the bars with notes are tiles too: one dark
    // line between every two bars and every two rows, as in Guitar Pro.
    let line = Stroke::new(GRID_LINE_WIDTH, style::tokens(ui).grid_line);
    let rows_bottom = rect.top() + rows.len() as f32 * ROW_HEIGHT;
    for measure in first_measure..=last_measure {
        let x = rect.left() + measure as f32 * mw;
        painter.line_segment([pos2(x, rect.top()), pos2(x, rows_bottom)], line);
    }
    let (left, right) = (
        rect.left() + first_measure as f32 * mw,
        rect.left() + last_measure as f32 * mw,
    );
    for row in first_row..=last_row {
        let y = rect.top() + row as f32 * ROW_HEIGHT;
        painter.line_segment([pos2(left, y), pos2(right, y)], line);
    }

    // The cursor's bar, outlined in its track's row: over the squares and the measure
    // lines, so a bar with notes (a filled square) shows it as well as an empty one. A
    // selection is marked on its own (the cursor sits at its first bar).
    if view.shown_selection().is_none()
        && view.cursor_track < last_row
        && view.cursor_track >= first_row
        && view.cursor_measure < view.measure_count
    {
        let top = rect.top() + view.cursor_track as f32 * ROW_HEIGHT;
        let measure = Rect::from_min_max(
            pos2(rect.left() + view.cursor_measure as f32 * mw, top),
            pos2(
                rect.left() + (view.cursor_measure as f32 + 1.0) * mw,
                top + ROW_HEIGHT,
            ),
        );
        painter.rect_stroke(
            measure,
            0.0,
            Stroke::new(1.5, visuals.selection.bg_fill),
            egui::StrokeKind::Inside,
        );
    }

    // The selection, over the squares so it shows on the colored ones too: the score's
    // color and shade, and an outline around the selected bars (of one track's row, or of
    // every row).
    if let Some(selection) = view.shown_selection()
        && selection.start() < last_measure
        && selection.end() >= first_measure
    {
        let rows_top = rect.top();
        let span = match selection.track {
            Some(track) if track < rows.len() => Some((
                rows_top + track as f32 * ROW_HEIGHT,
                rows_top + (track as f32 + 1.0) * ROW_HEIGHT,
            )),
            Some(_) => None,
            None => Some((rows_top, rows_top + rows.len() as f32 * ROW_HEIGHT)),
        };
        if let Some((top, bottom)) = span {
            let area = Rect::from_min_max(
                pos2(rect.left() + selection.start() as f32 * mw, top),
                pos2(rect.left() + (selection.end() as f32 + 1.0) * mw, bottom),
            );
            painter.rect_filled(
                area,
                0.0,
                view.selection_color.gamma_multiply(SELECTION_ALPHA),
            );
            painter.rect_stroke(
                area,
                0.0,
                Stroke::new(1.5, view.selection_color),
                egui::StrokeKind::Inside,
            );
        }
    }

    // While the selected bars are dragged, where they would go: an outline over every
    // track (all of them move) and a line where the first of them lands.
    if let Some(drag) = BarsDrag::get(ui)
        && drag.to != drag.start
    {
        let left = rect.left() + drag.to as f32 * mw;
        let area = Rect::from_min_max(
            pos2(left, rect.top()),
            pos2(
                left + drag.count as f32 * mw,
                rect.top() + rows.len() as f32 * ROW_HEIGHT,
            ),
        );
        painter.rect_filled(area, 0.0, view.selection_color.gamma_multiply(0.18));
        painter.rect_stroke(
            area,
            0.0,
            Stroke::new(1.5, view.selection_color),
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [pos2(left, rect.top()), pos2(left, area.bottom())],
            Stroke::new(3.0, view.selection_color),
        );
    }

    // Repeats and alternative endings in the master row, below every track: an ending's
    // number at the top, the repeat signs at the bottom.
    let master_top = rect.top() + rows.len() as f32 * ROW_HEIGHT;
    let sign_font = FontId::monospace(9.0);
    for measure in first_measure..last_measure {
        let Some(repeat) = grid.repeats.get(measure) else {
            continue;
        };
        let x = rect.left() + measure as f32 * mw;
        let bottom = master_top + ROW_HEIGHT - 3.0;
        if repeat.start {
            painter.text(
                pos2(x + 2.0, bottom),
                Align2::LEFT_BOTTOM,
                "|:",
                sign_font.clone(),
                visuals.text_color(),
            );
        }
        if let Some(times) = repeat.end_repeats {
            painter.text(
                pos2(x + mw - 2.0, bottom),
                Align2::RIGHT_BOTTOM,
                ":|",
                sign_font.clone(),
                visuals.text_color(),
            );
            if times > 1 {
                painter.text(
                    pos2(x + mw - 2.0, master_top + 2.0),
                    Align2::RIGHT_TOP,
                    format!("\u{00d7}{times}"),
                    FontId::proportional(8.0),
                    visuals.weak_text_color(),
                );
            }
        }
        if repeat.endings != 0 {
            painter.text(
                pos2(x + 2.0, master_top + 2.0),
                Align2::LEFT_TOP,
                ending_label(repeat.endings),
                sign_font.clone(),
                visuals.weak_text_color(),
            );
        }
    }

    let cell = pointer_cell(&response, rect, view.measure_count, rows.len(), view.zoom);
    if let Some((track, measure)) = cell {
        let text = format!(
            "Track {}: {} — measure {}",
            track.saturating_add(1),
            rows.get(track).map_or("", |row| row.name.as_str()),
            measure.saturating_add(1)
        );
        response.clone().on_hover_text(text);
    }
    handle_matrix_clicks(ui, &response, rect, view, cell, rows.len(), actions);
}

/// The selected bars being dragged in the grid: the selection's first bar and length, the
/// bar it was grabbed at, and where its first bar would go now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BarsDrag {
    start: usize,
    count: usize,
    grab: usize,
    to: usize,
}

impl BarsDrag {
    fn id(ui: &egui::Ui) -> egui::Id {
        ui.make_persistent_id("bars_drag")
    }

    /// The drag in progress, if any.
    fn get(ui: &egui::Ui) -> Option<BarsDrag> {
        ui.ctx()
            .data(|data| data.get_temp::<(usize, usize, usize, usize)>(Self::id(ui)))
            .map(|(start, count, grab, to)| BarsDrag {
                start,
                count,
                grab,
                to,
            })
    }

    fn store(self, ui: &egui::Ui) {
        ui.ctx().data_mut(|data| {
            data.insert_temp(Self::id(ui), (self.start, self.count, self.grab, self.to));
        });
    }

    fn clear(ui: &egui::Ui) {
        ui.ctx()
            .data_mut(|data| data.remove_temp::<(usize, usize, usize, usize)>(Self::id(ui)));
    }
}

/// Dragging the bar selection (pressed on one of its bars) moves those bars, in every
/// track, by as many bars as the pointer moves; dropping it there moves them. Returns
/// whether such a drag is going on (then the drag selects nothing).
#[allow(clippy::too_many_arguments)]
fn drag_selected_bars(
    ui: &egui::Ui,
    response: &Response,
    rect: Rect,
    view: GridView,
    track_count: usize,
    at: Option<usize>,
    at_x: &dyn Fn(f32) -> Option<usize>,
    actions: &mut Vec<GridAction>,
) -> bool {
    let on_selection = |pos: egui::Pos2| {
        let Some(selection) = view.shown_selection() else {
            return false;
        };
        cell_at(pos, rect, view.measure_count, track_count, view.zoom).is_some_and(
            |(track, measure)| {
                (selection.start()..=selection.end()).contains(&measure)
                    && selection.track.is_none_or(|selected| selected == track)
            },
        )
    };
    if response.drag_started()
        && let Some(origin) = ui.input(|input| input.pointer.press_origin())
        && on_selection(origin)
        && let Some(selection) = view.shown_selection()
        && let Some(grab) = at_x(origin.x)
    {
        let start = selection.start();
        BarsDrag {
            start,
            count: selection.end().saturating_sub(start).saturating_add(1),
            grab,
            to: start,
        }
        .store(ui);
    }
    let Some(mut drag) = BarsDrag::get(ui) else {
        if !response.dragged() && response.hover_pos().is_some_and(on_selection) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            response
                .clone()
                .on_hover_text("Drag the selected bars to move them (in every track)");
        }
        return false;
    };
    if response.dragged() {
        if let Some(measure) = at {
            let last_start = view.measure_count.saturating_sub(drag.count);
            let shifted = drag
                .start
                .saturating_add(measure)
                .saturating_sub(drag.grab)
                .min(last_start);
            // Left of where it was grabbed: as far as the song's first bar.
            drag.to = if measure < drag.grab {
                drag.start.saturating_sub(drag.grab.saturating_sub(measure))
            } else {
                shifted
            };
            drag.store(ui);
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if response.drag_stopped() {
        if drag.to != drag.start {
            actions.push(GridAction::MoveBars {
                from: drag.start,
                count: drag.count,
                to: drag.to,
            });
        }
        BarsDrag::clear(ui);
    } else if !response.dragged() && !ui.input(|input| input.pointer.any_down()) {
        // The drag ended without the grid seeing it: forget it.
        BarsDrag::clear(ui);
    }
    true
}

/// The section whose bars include `measure`: the bar its name starts at and the bar before
/// the next section (or the song's last bar). `None` before the first section.
fn section_span(grid: &GridData, measure_count: usize, measure: usize) -> Option<(usize, usize)> {
    if measure >= measure_count {
        return None;
    }
    let start = (0..=measure)
        .rev()
        .find(|index| grid.sections.get(*index).is_some_and(Option::is_some))?;
    let end = (measure.saturating_add(1)..measure_count)
        .find(|index| grid.sections.get(*index).is_some_and(Option::is_some))
        .map_or(measure_count.saturating_sub(1), |next| {
            next.saturating_sub(1)
        });
    Some((start, end))
}

/// "1." or "1, 2." for a bit set of alternative endings.
fn ending_label(bits: u8) -> String {
    let labels: Vec<String> = (0..8_u8)
        .filter(|bit| bits & (1 << bit) != 0)
        .map(|bit| format!("{}.", bit.saturating_add(1)))
        .collect();
    labels.join(", ")
}

/// The column rectangle of `measure`.
fn measure_rect(rect: Rect, measure: usize, zoom: f32) -> Rect {
    let mw = MEASURE_WIDTH * zoom;
    Rect::from_min_max(
        pos2(rect.left() + measure as f32 * mw, rect.top()),
        pos2(rect.left() + (measure as f32 + 1.0) * mw, rect.bottom()),
    )
}

/// Which track and measure the pointer is over.
fn pointer_cell(
    response: &Response,
    rect: Rect,
    measure_count: usize,
    track_count: usize,
    zoom: f32,
) -> Option<(usize, usize)> {
    let pointer = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos())?;
    cell_at(pointer, rect, measure_count, track_count, zoom)
}

/// The track and measure at `pos`, if it is on one of the matrix's squares.
fn cell_at(
    pos: egui::Pos2,
    rect: Rect,
    measure_count: usize,
    track_count: usize,
    zoom: f32,
) -> Option<(usize, usize)> {
    if !rect.contains(pos) {
        return None;
    }
    let track = ((pos.y - rect.top()) / ROW_HEIGHT) as usize;
    let measure = ((pos.x - rect.left()) / (MEASURE_WIDTH * zoom)) as usize;
    (track < track_count && measure < measure_count).then_some((track, measure))
}

/// Click puts the cursor on the bar's first beat, drag selects measures, shift-click
/// extends the selection.
#[allow(clippy::too_many_arguments)]
fn handle_matrix_clicks(
    ui: &egui::Ui,
    response: &Response,
    rect: Rect,
    view: GridView,
    cell: Option<(usize, usize)>,
    track_count: usize,
    actions: &mut Vec<GridAction>,
) {
    // The measure under the pointer even when the drag has left the matrix vertically
    // (onto another row or the ruler): only x decides the bar.
    let mw = MEASURE_WIDTH * view.zoom;
    let at_x = |x: f32| {
        let x = x - rect.left();
        if x < 0.0 {
            return None;
        }
        let measure = (x / mw) as usize;
        (measure < view.measure_count).then_some(measure)
    };
    let at = response
        .interact_pointer_pos()
        .and_then(|pointer| at_x(pointer.x));
    // The master row under the tracks shows the repeats; it is no place for a bar
    // selection or the cursor.
    let master_top = rect.top() + track_count as f32 * ROW_HEIGHT;
    let pressed_in_master = ui
        .input(|input| input.pointer.press_origin())
        .is_some_and(|origin| origin.y >= master_top && rect.contains(origin));

    if pressed_in_master {
        return;
    }
    if drag_selected_bars(ui, response, rect, view, track_count, at, &at_x, actions) {
        return;
    }
    // A drag starts once the pointer has moved a little: at the bar and row it was
    // pressed on, which may be the one before the pointer by now.
    let pressed = ui
        .input(|input| input.pointer.press_origin())
        .filter(|_| response.drag_started());
    let started = pressed.and_then(|origin| {
        let measure = at_x(origin.x)?;
        let cell = cell_at(origin, rect, view.measure_count, track_count, view.zoom);
        Some((measure, cell.map(|(track, _)| track)))
    });
    if let Some((measure, track)) = started {
        let selection = match track {
            Some(track) => MeasureSelection::on(track, measure),
            None => MeasureSelection::new(measure),
        };
        actions.push(GridAction::Select(selection));
        if let Some(track) = track {
            actions.push(GridAction::MoveCursor { track, measure });
        }
    }
    if response.dragged()
        && let Some(measure) = at
    {
        // From the bar the drag started on: in its first frame the one just pressed (the
        // view does not show that selection yet), later the selection's own anchor.
        let (anchor, start_track) = match started {
            Some((anchor, track)) => (anchor, track),
            None => (
                view.selection.map_or(measure, |selection| selection.anchor),
                Some(view.cursor_track),
            ),
        };
        // Within the row the drag started in: that one track. Anywhere else: every track.
        let track = match cell {
            Some((track, _)) if Some(track) == start_track => Some(track),
            _ => None,
        };
        actions.push(GridAction::Select(MeasureSelection {
            anchor,
            focus: measure,
            track,
        }));
    }
    if response.clicked()
        && let Some(measure) = at
    {
        if ui.input(|input| input.modifiers.shift) {
            // Shift-click grows the selection to take in this bar (whichever end is
            // nearer it), or else selects from the cursor's bar to this one. A click in
            // another track's row than a one-track selection's takes in every track, as
            // Shift+Up/Down do.
            let selection = match (view.selection, cell) {
                (Some(selection), cell) => {
                    let mut grown = selection.extend_to(measure);
                    if selection.track.is_some() && cell.map(|(track, _)| track) != selection.track
                    {
                        grown.track = None;
                    }
                    grown
                }
                // From the cursor's tile: in its row that track, in another row every
                // track (the cursor's bar and this one, in all of them).
                (None, Some((track, _))) if track == view.cursor_track => {
                    MeasureSelection::on(track, view.cursor_measure).to(measure)
                }
                (None, _) => MeasureSelection::new(view.cursor_measure).to(measure),
            };
            actions.push(GridAction::Select(selection));
        } else if let Some((track, measure)) = cell {
            // A plain click only places the cursor on the bar's first beat.
            actions.push(GridAction::Place { track, measure });
        }
    }
}
