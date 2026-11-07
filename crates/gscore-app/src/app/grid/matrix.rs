//! The measure matrix of the song grid: the ruler with measure numbers, section markers,
//! repeats and alternative endings, and one row of squares per track. Clicking moves the
//! cursor; dragging selects measures; the playing measure is highlighted.

use egui::{Align2, Color32, FontId, Rect, Response, Sense, Stroke, pos2, vec2};

use crate::document::GridData;
use crate::state::MeasureSelection;

use super::super::page::SELECTION_ALPHA;
use super::rows::{TrackRow, color32};
use super::{CELL_PADDING, GridAction, GridView, MEASURE_WIDTH, ROW_HEIGHT, RULER_HEIGHT, mixer};

/// The measure ruler above the tracks: measure numbers, section markers, repeats and
/// alternative endings. It stays at the top while the tracks scroll under it, and follows
/// the matrix sideways by `scroll_x`.
pub(super) fn ruler(
    ui: &mut egui::Ui,
    grid: &GridData,
    view: GridView,
    scroll_x: f32,
    actions: &mut Vec<GridAction>,
) {
    let width = view.measure_count as f32 * MEASURE_WIDTH;
    let available = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(vec2(width.max(available), RULER_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
    // The measures the strip shows: the matrix's sideways scroll decides where it starts.
    let left = rect.left() - scroll_x;
    let first = (scroll_x / MEASURE_WIDTH).floor().max(0.0) as usize;
    let last =
        (((scroll_x + rect.width()) / MEASURE_WIDTH).ceil() as usize).min(view.measure_count);
    let x_of = |measure: usize| left + measure as f32 * MEASURE_WIDTH;
    let column = |measure: usize| {
        Rect::from_min_max(
            pos2(x_of(measure), rect.top()),
            pos2(x_of(measure) + MEASURE_WIDTH, rect.bottom()),
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
            Color32::from_rgb(255, 200, 40).gamma_multiply(0.25),
        );
    }
    if let Some(selection) = view.selection
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
        let x = x_of(measure);
        if measure % 4 == 0 || view.measure_count <= 64 {
            painter.text(
                pos2(x + 2.0, rect.top() + 2.0),
                Align2::LEFT_TOP,
                (measure.saturating_add(1)).to_string(),
                number_font.clone(),
                visuals.weak_text_color(),
            );
        }
        // The section's line above the ruler's bottom edge marks where it starts.
        if let Some(Some(section)) = grid.sections.get(measure) {
            painter.line_segment(
                [
                    pos2(x, rect.top() + RULER_HEIGHT - 3.0),
                    pos2(x + MEASURE_WIDTH, rect.top() + RULER_HEIGHT - 3.0),
                ],
                Stroke::new(2.0, color32(section.color)),
            );
        }
        if let Some(repeat) = grid.repeats.get(measure) {
            let y = rect.top() + RULER_HEIGHT - 6.0;

            if repeat.start {
                painter.text(
                    pos2(x + 1.0, y),
                    Align2::LEFT_BOTTOM,
                    "|:",
                    number_font.clone(),
                    visuals.text_color(),
                );
            }
            if let Some(times) = repeat.end_repeats {
                painter.text(
                    pos2(x + MEASURE_WIDTH - 1.0, y),
                    Align2::RIGHT_BOTTOM,
                    ":|",
                    number_font.clone(),
                    visuals.text_color(),
                );
                if times > 1 {
                    painter.text(
                        pos2(x + MEASURE_WIDTH - 1.0, rect.top() + 2.0),
                        Align2::RIGHT_TOP,
                        format!("\u{00d7}{times}"),
                        FontId::proportional(8.0),
                        visuals.weak_text_color(),
                    );
                }
            }
            if repeat.endings != 0 {
                painter.text(
                    pos2(x + 2.0, rect.top() + 10.0),
                    Align2::LEFT_TOP,
                    ending_label(repeat.endings),
                    number_font.clone(),
                    visuals.weak_text_color(),
                );
            }
        }
    }
    // The ruler always selects whole bars across every track.
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let x = pointer.x - left;
        if x >= 0.0 {
            let measure = (x / MEASURE_WIDTH) as usize;
            if measure < view.measure_count {
                let extend = ui.input(|input| input.modifiers.shift);
                let mut selection = if extend {
                    view.selection.map_or_else(
                        || MeasureSelection::new(measure),
                        |selection| selection.to(measure),
                    )
                } else {
                    MeasureSelection::new(measure)
                };
                selection.track = None;
                actions.push(GridAction::Select(selection));
            }
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
    let width = view.measure_count as f32 * MEASURE_WIDTH;
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
    painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);

    // Paint only the measures and rows the scroll area shows.
    let clip = ui.clip_rect().intersect(rect);
    let first_row = ((clip.top() - rect.top()) / ROW_HEIGHT).floor().max(0.0) as usize;
    let last_row = (((clip.bottom() - rect.top()) / ROW_HEIGHT).ceil() as usize).min(rows.len());
    let first_measure = ((clip.left() - rect.left()) / MEASURE_WIDTH)
        .floor()
        .max(0.0) as usize;
    let last_measure =
        (((clip.right() - rect.left()) / MEASURE_WIDTH).ceil() as usize).min(view.measure_count);

    // The playing measure, behind the squares.
    if let Some(playing) = view.playing
        && playing >= first_measure
        && playing < last_measure
    {
        let column = measure_rect(rect, playing);
        painter.rect_filled(
            column,
            0.0,
            Color32::from_rgb(255, 200, 40).gamma_multiply(0.25),
        );
    }

    // Row stripes and squares.
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
        if row % 2 == 1 {
            painter.rect_filled(row_rect, 0.0, visuals.faint_bg_color);
        }
        if row == view.cursor_track && view.cursor_measure < view.measure_count {
            let measure = Rect::from_min_max(
                pos2(
                    rect.left() + view.cursor_measure as f32 * MEASURE_WIDTH,
                    row_rect.top(),
                ),
                pos2(
                    rect.left() + (view.cursor_measure as f32 + 1.0) * MEASURE_WIDTH,
                    row_rect.bottom(),
                ),
            );
            painter.rect_stroke(
                measure,
                0.0,
                Stroke::new(1.5, visuals.selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
        for measure_index in first_measure..last_measure {
            if !cells.get(measure_index).copied().unwrap_or(false) {
                continue;
            }
            let cell = Rect::from_min_size(
                pos2(
                    rect.left() + measure_index as f32 * MEASURE_WIDTH + CELL_PADDING,
                    row_rect.top() + CELL_PADDING,
                ),
                vec2(
                    MEASURE_WIDTH - 2.0 * CELL_PADDING,
                    ROW_HEIGHT - 2.0 * CELL_PADDING,
                ),
            );
            let color = rows.get(row).map_or(visuals.text_color(), |track| {
                mixer::sounding_color(color32(track.color), track.audible)
            });
            painter.rect_filled(cell, 0.0, color);
        }
    }

    // The measure lines between the squares.
    for measure in first_measure..last_measure {
        let x = rect.left() + measure as f32 * MEASURE_WIDTH;
        let strong = measure % 4 == 0 || grid.sections.get(measure).is_some_and(Option::is_some);
        let color = if strong {
            visuals.widgets.noninteractive.bg_stroke.color
        } else {
            visuals
                .widgets
                .noninteractive
                .bg_stroke
                .color
                .gamma_multiply(0.4)
        };
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, color),
        );
    }

    // The selection, over the squares so it shows on the colored ones too: the score's
    // color and shade, and an outline around the selected bars (of one track's row, or of
    // every row).
    if let Some(selection) = view.selection
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
                pos2(rect.left() + selection.start() as f32 * MEASURE_WIDTH, top),
                pos2(
                    rect.left() + (selection.end() as f32 + 1.0) * MEASURE_WIDTH,
                    bottom,
                ),
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

    // Section titles in the master row, below every track: there they have the whole
    // width of their section and no measure number or repeat sign to collide with.
    let master_top = rect.top() + rows.len() as f32 * ROW_HEIGHT;
    for measure in first_measure..last_measure {
        let Some(Some(section)) = grid.sections.get(measure) else {
            continue;
        };
        let x = rect.left() + measure as f32 * MEASURE_WIDTH;
        let color = color32(section.color);
        // Clip the title to the measures before the next section, so two markers never
        // write over each other.
        let next = (measure.saturating_add(1)..view.measure_count)
            .find(|&index| grid.sections.get(index).is_some_and(Option::is_some))
            .unwrap_or(view.measure_count);
        let room = next.saturating_sub(measure) as f32 * MEASURE_WIDTH;
        // The title may run into the empty area after the last measure; it stops at the
        // next section's marker.
        let end_x = if next >= view.measure_count {
            rect.right()
        } else {
            x + room
        };
        painter.line_segment(
            [pos2(x, master_top), pos2(x, master_top + ROW_HEIGHT - 2.0)],
            Stroke::new(2.0, color),
        );
        painter.line_segment(
            [
                pos2(x, master_top + ROW_HEIGHT - 2.0),
                pos2(x + room, master_top + ROW_HEIGHT - 2.0),
            ],
            Stroke::new(1.5, color),
        );
        if end_x - x >= MEASURE_WIDTH {
            let clip = Rect::from_min_max(
                pos2(x + 3.0, master_top),
                pos2(end_x - 2.0, master_top + ROW_HEIGHT),
            );
            painter.with_clip_rect(clip).text(
                pos2(x + 4.0, master_top + ROW_HEIGHT / 2.0),
                Align2::LEFT_CENTER,
                &section.title,
                FontId::proportional(10.0),
                color,
            );
        }
    }

    let cell = pointer_cell(&response, rect, view.measure_count, rows.len());
    if let Some((track, measure)) = cell {
        let text = format!(
            "Track {}: {} — measure {}",
            track.saturating_add(1),
            rows.get(track).map_or("", |row| row.name.as_str()),
            measure.saturating_add(1)
        );
        response.clone().on_hover_text(text);
    }
    handle_matrix_clicks(ui, &response, rect, view, cell, actions);
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
fn measure_rect(rect: Rect, measure: usize) -> Rect {
    Rect::from_min_max(
        pos2(rect.left() + measure as f32 * MEASURE_WIDTH, rect.top()),
        pos2(
            rect.left() + (measure as f32 + 1.0) * MEASURE_WIDTH,
            rect.bottom(),
        ),
    )
}

/// Which track and measure the pointer is over.
fn pointer_cell(
    response: &Response,
    rect: Rect,
    measure_count: usize,
    track_count: usize,
) -> Option<(usize, usize)> {
    let pointer = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos())?;
    if !rect.contains(pointer) {
        return None;
    }
    let track = ((pointer.y - rect.top()) / ROW_HEIGHT) as usize;
    let measure = ((pointer.x - rect.left()) / MEASURE_WIDTH) as usize;
    (track < track_count && measure < measure_count).then_some((track, measure))
}

/// Click moves the cursor, drag selects measures, shift-click extends the selection.
fn handle_matrix_clicks(
    ui: &egui::Ui,
    response: &Response,
    rect: Rect,
    view: GridView,
    cell: Option<(usize, usize)>,
    actions: &mut Vec<GridAction>,
) {
    // The measure under the pointer even when the drag has left the matrix vertically
    // (onto another row or the ruler): only x decides the bar.
    let at = response.interact_pointer_pos().and_then(|pointer| {
        let x = pointer.x - rect.left();
        if x < 0.0 {
            return None;
        }
        let measure = (x / MEASURE_WIDTH) as usize;
        (measure < view.measure_count).then_some(measure)
    });
    if response.drag_started()
        && let Some(measure) = at
    {
        let selection = match cell {
            Some((track, _)) => MeasureSelection::on(track, measure),
            None => MeasureSelection::new(measure),
        };
        actions.push(GridAction::Select(selection));
        if let Some((track, measure)) = cell {
            actions.push(GridAction::MoveCursor { track, measure });
        }
    }
    if response.dragged()
        && let Some(measure) = at
    {
        let anchor = view.selection.map_or(measure, MeasureSelection::start);
        // Within the row the drag started in: that one track. Anywhere else: every track.
        let track = match cell {
            Some((track, _)) if track == view.cursor_track => Some(track),
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
        let extend = ui.input(|input| input.modifiers.shift);
        let selection = match (extend, view.selection) {
            (true, Some(selection)) => selection.to(measure),
            (true, None) => match cell {
                Some((track, _)) => MeasureSelection::on(track, measure),
                None => MeasureSelection::new(measure),
            },
            (false, _) => match cell {
                Some((track, _)) => MeasureSelection::on(track, measure),
                None => MeasureSelection::new(measure),
            },
        };
        actions.push(GridAction::Select(selection));
        if let Some((track, measure)) = cell {
            actions.push(GridAction::MoveCursor { track, measure });
        }
    }
}
