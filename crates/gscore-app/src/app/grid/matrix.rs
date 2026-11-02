//! The measure matrix of the song grid: the ruler with measure numbers, section markers,
//! repeats and alternative endings, and one row of squares per track. Clicking moves the
//! cursor; dragging selects measures; the playing measure is highlighted.

use egui::{Align2, Color32, FontId, Rect, Response, Sense, Stroke, pos2, vec2};

use crate::document::GridData;
use crate::state::MeasureSelection;

use super::{
    CELL_PADDING, GridAction, GridView, MEASURE_WIDTH, ROW_HEIGHT, RULER_HEIGHT,
    SECTION_TITLE_MEASURES, TrackRow, color32,
};

/// The measure matrix: ruler with numbers, sections and repeats, then one row per track.
pub(super) fn matrix(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    grid: &GridData,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let width = view.measure_count as f32 * MEASURE_WIDTH;
    let height = RULER_HEIGHT + rows.len() as f32 * ROW_HEIGHT;
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

    // The playing measure and the selection, behind the squares.
    if let Some(playing) = view.playing
        && playing < view.measure_count
    {
        let column = measure_rect(rect, playing);
        painter.rect_filled(
            column,
            0.0,
            Color32::from_rgb(255, 200, 40).gamma_multiply(0.25),
        );
    }
    if let Some(selection) = view.selection {
        for measure in selection.start()..=selection.end().min(view.measure_count.saturating_sub(1))
        {
            let column = measure_rect(rect, measure);
            painter.rect_filled(column, 0.0, visuals.selection.bg_fill.gamma_multiply(0.30));
        }
    }

    // Row stripes and squares.
    for (row, cells) in grid.cells.iter().enumerate().take(rows.len()) {
        let row_rect = Rect::from_min_max(
            pos2(
                rect.left(),
                rect.top() + RULER_HEIGHT + row as f32 * ROW_HEIGHT,
            ),
            pos2(
                rect.right(),
                rect.top() + RULER_HEIGHT + (row as f32 + 1.0) * ROW_HEIGHT,
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
        for (measure_index, has_notes) in cells.iter().enumerate() {
            if !*has_notes {
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
            let color = rows
                .get(row)
                .map_or(visuals.text_color(), |track| color32(track.color));
            painter.rect_filled(cell, 2.0, color);
        }
    }

    // Grid lines and measure numbers.
    let number_font = FontId::monospace(9.0);
    for measure in 0..view.measure_count {
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
        if measure % 4 == 0 || view.measure_count <= 64 {
            painter.text(
                pos2(x + 2.0, rect.top() + 2.0),
                Align2::LEFT_TOP,
                (measure.saturating_add(1)).to_string(),
                number_font.clone(),
                visuals.weak_text_color(),
            );
        }
    }

    // Sections, repeats and alternative endings in the ruler.
    for measure in 0..view.measure_count {
        let x = rect.left() + measure as f32 * MEASURE_WIDTH;
        if let Some(Some(section)) = grid.sections.get(measure) {
            let color = color32(section.color);
            painter.line_segment(
                [
                    pos2(x, rect.top() + RULER_HEIGHT - 3.0),
                    pos2(x + MEASURE_WIDTH, rect.top() + RULER_HEIGHT - 3.0),
                ],
                Stroke::new(2.0, color),
            );
            // Clip the title to the measures before the next section, so two markers
            // never write over each other.
            let next = (measure.saturating_add(1)..view.measure_count)
                .find(|&index| grid.sections.get(index).is_some_and(Option::is_some))
                .unwrap_or(view.measure_count);
            let room = next.saturating_sub(measure) as f32 * MEASURE_WIDTH;
            if room >= MEASURE_WIDTH * SECTION_TITLE_MEASURES {
                let clip = Rect::from_min_max(
                    pos2(x, rect.top()),
                    pos2(x + room, rect.top() + RULER_HEIGHT),
                );
                painter.with_clip_rect(clip).text(
                    pos2(x + 2.0, rect.top() + RULER_HEIGHT - 6.0),
                    Align2::LEFT_BOTTOM,
                    &section.title,
                    FontId::proportional(9.0),
                    color,
                );
            }
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
    if !rect.contains(pointer) || pointer.y < rect.top() + RULER_HEIGHT {
        return None;
    }
    let track = ((pointer.y - rect.top() - RULER_HEIGHT) / ROW_HEIGHT) as usize;
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
    let pointer = response.interact_pointer_pos();
    let at = pointer.and_then(|pointer| {
        if !rect.contains(pointer) || pointer.y < rect.top() + RULER_HEIGHT {
            return None;
        }
        let measure = ((pointer.x - rect.left()) / MEASURE_WIDTH) as usize;
        (measure < view.measure_count).then_some(measure)
    });
    if response.drag_started()
        && let Some(measure) = at
    {
        actions.push(GridAction::Select(MeasureSelection::new(measure)));
        if let Some((track, measure)) = cell {
            actions.push(GridAction::MoveCursor { track, measure });
        }
    }
    if response.dragged()
        && let Some(measure) = at
    {
        let anchor = view.selection.map_or(measure, MeasureSelection::start);
        actions.push(GridAction::Select(
            MeasureSelection::new(anchor).to(measure),
        ));
    }
    if response.clicked()
        && let Some(measure) = at
    {
        let extend = ui.input(|input| input.modifiers.shift);
        let selection = if extend {
            view.selection.map_or_else(
                || MeasureSelection::new(measure),
                |selection| selection.to(measure),
            )
        } else {
            MeasureSelection::new(measure)
        };
        actions.push(GridAction::Select(selection));
        if let Some((track, measure)) = cell {
            actions.push(GridAction::MoveCursor { track, measure });
        }
    }
}
