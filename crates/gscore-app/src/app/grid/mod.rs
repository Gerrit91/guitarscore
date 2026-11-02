//! The track and song grid (the bottom panel): one row per track with its color, instrument
//! and mixer controls, and a measure matrix that fills a square where a track has notes.
//!
//! Clicking a square moves the edit cursor there; dragging selects a range of measures (for
//! copy/paste and looping later); the measure being played is highlighted. Track commands
//! go through `gscore-edit`, so every change can be undone.

use gscore_edit::Command;
use gscore_model::TrackIndex;

use super::GuitarScoreApp;
use crate::document::GridData;
use crate::state::MeasureSelection;

use rows::TrackRow;

mod matrix;
mod mixer;
mod rows;

/// Width of the fixed track column on the left.
pub(super) const TRACK_COLUMN: f32 = 330.0;
/// Height of one track row.
pub(super) const ROW_HEIGHT: f32 = 24.0;
/// Height of the ruler above the matrix (measure numbers, sections, repeats).
pub(super) const RULER_HEIGHT: f32 = 26.0;
/// Width of one measure column.
pub(super) const MEASURE_WIDTH: f32 = 13.0;
/// Padding inside a matrix square.
pub(super) const CELL_PADDING: f32 = 2.5;
/// Room a section title needs before it is drawn (four measures).
pub(super) const SECTION_TITLE_MEASURES: f32 = 4.0;

/// What the grid asks the app to do after drawing.
pub(super) enum GridAction {
    /// Run a track command; `merge` continues the previous undo step (one slider drag).
    Command { command: Command, merge: bool },
    /// A slider drag started or ended: the next command starts a new undo step.
    BreakMerge,
    /// Move the cursor to the first beat of a track and measure.
    MoveCursor { track: usize, measure: usize },
    /// Replace the selected measures.
    Select(MeasureSelection),
    /// Open the rename window for a track.
    Rename { track: usize },
    /// Show a track in the track panel.
    ShowTrack { track: usize },
}

/// The rename window's state.
#[derive(Debug)]
pub(super) struct RenameTrack {
    /// The track being renamed.
    pub track: usize,
    /// The text typed so far.
    pub name: String,
    /// Whether the text field has been focused yet.
    pub focused: bool,
}

impl GuitarScoreApp {
    /// The bottom panel: track headers and the measure matrix.
    pub(super) fn track_grid(&mut self, ui: &mut egui::Ui) {
        let Some(document) = &mut self.document else {
            return;
        };
        let grid = document.grid();
        let cursor = document.editor().cursor();
        let rows = rows::track_rows(document);
        let measure_count = document.song().measure_count();
        let playing = self.playback.current_measure().map(|m| m.saturating_sub(1));
        let mut actions = Vec::new();
        draw(
            ui,
            &rows,
            &grid,
            GridView {
                measure_count,
                cursor_measure: cursor.measure.get(),
                cursor_track: cursor.track.get(),
                playing,
                selection: self.view.selection,
                shown_track: self.view.track,
            },
            &mut actions,
        );
        for action in actions {
            match action {
                GridAction::Command { command, merge } => self.apply_command(command, merge),
                GridAction::BreakMerge => {
                    if let Some(document) = &mut self.document {
                        document.editor_mut().break_merge();
                    }
                }
                GridAction::MoveCursor { track, measure } => self.move_cursor_to(track, measure),
                GridAction::Select(selection) => {
                    self.view.selection = Some(selection);
                    if let Some(document) = &mut self.document {
                        document.editor_mut().set_selection(Some(
                            gscore_edit::Selection::Measures {
                                anchor: gscore_model::MeasureIndex::new(selection.start()),
                                focus: gscore_model::MeasureIndex::new(selection.end()),
                            },
                        ));
                    }
                }
                GridAction::Rename { track } => {
                    let name = rows
                        .get(track)
                        .map_or_else(String::new, |row| row.name.clone());
                    self.renaming = Some(RenameTrack {
                        track,
                        name,
                        focused: false,
                    });
                }
                GridAction::ShowTrack { track } => {
                    self.view.track = track;
                    self.view.show_track_panel = true;
                }
            }
        }
    }

    /// Moves the cursor to the first beat of `track`, `measure`.
    fn move_cursor_to(&mut self, track: usize, measure: usize) {
        let Some(document) = &mut self.document else {
            return;
        };
        let mut cursor = document.editor().cursor();
        cursor.track = TrackIndex::new(track);
        cursor.measure = gscore_model::MeasureIndex::new(measure);
        cursor.voice = gscore_model::VoiceIndex::new(0);
        cursor.beat = gscore_model::BeatIndex::new(0);
        document.editor_mut().set_cursor(cursor);
        self.view.track = track;
    }

    /// The rename window: a text field and Rename/Cancel.
    pub(super) fn show_rename_window(&mut self, ctx: &egui::Context) {
        let Some(mut rename) = self.renaming.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Rename track")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let response =
                    ui.add(egui::TextEdit::singleline(&mut rename.name).hint_text("Track name"));
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Track name"));
                if !rename.focused {
                    response.request_focus();
                    rename.focused = true;
                }
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply && !rename.name.is_empty() {
            self.apply_command(
                Command::RenameTrack {
                    track: TrackIndex::new(rename.track),
                    name: rename.name,
                },
                false,
            );
        } else if !cancel && open {
            self.renaming = Some(rename);
        }
    }
}

/// What the grid needs to know about the app while drawing.
#[derive(Clone, Copy)]
pub(super) struct GridView {
    measure_count: usize,
    cursor_measure: usize,
    cursor_track: usize,
    playing: Option<usize>,
    selection: Option<MeasureSelection>,
    shown_track: usize,
}

fn draw(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    grid: &GridData,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal_top(|ui| {
                rows::headers(ui, rows, view, actions);
                egui::ScrollArea::horizontal()
                    .id_salt("song_grid")
                    .show(ui, |ui| matrix::matrix(ui, rows, grid, view, actions));
            });
        });
}
