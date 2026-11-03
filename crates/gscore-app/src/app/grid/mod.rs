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

mod bar;
mod matrix;
pub(super) mod mixer;
mod rows;
mod strips;

pub(crate) use strips::MixerState;

/// Width of the fixed track column on the left.
pub(super) const TRACK_COLUMN: f32 = 384.0;
/// Height of one track row.
pub(super) const ROW_HEIGHT: f32 = 24.0;
/// Height of the ruler above the matrix (measure numbers, sections, repeats).
pub(super) const RULER_HEIGHT: f32 = 26.0;
/// Most track rows the panel shows when it opens; more tracks scroll.
pub(crate) const OPEN_ROWS: usize = 8;
/// The panel's margins and the horizontal scroll bar, around the rows.
const PANEL_EXTRA: f32 = 22.0;

/// The panel's height for `tracks` rows plus the master row.
pub(crate) fn height_for(tracks: usize) -> f32 {
    RULER_HEIGHT + (tracks as f32 + 1.0) * ROW_HEIGHT + PANEL_EXTRA
}

/// Default height of the mixer view, and the range it may be dragged to.
pub(crate) const MIXER_HEIGHT: f32 = 390.0;
pub(crate) const MIXER_MIN_HEIGHT: f32 = 240.0;
pub(crate) const MIXER_MAX_HEIGHT: f32 = 620.0;

/// The height the panel opens at: all tracks (up to eight) and the master row.
pub(crate) fn opening_height(tracks: usize) -> f32 {
    height_for(tracks.min(OPEN_ROWS))
}

/// Width of one measure column: the same as a row, so every cell is a square.
pub(super) const MEASURE_WIDTH: f32 = ROW_HEIGHT;
/// Padding inside a matrix square: none, the squares touch.
pub(super) const CELL_PADDING: f32 = 0.0;
/// Room a section title needs before it is drawn (three measures).
pub(super) const SECTION_TITLE_MEASURES: f32 = 3.0;

/// What the grid asks the app to do after drawing.
#[derive(Debug)]
pub(super) enum GridAction {
    /// Run a track command; `merge` continues the previous undo step (one slider drag).
    Command { command: Command, merge: bool },
    /// Open the new-track popup, inserting before this track.
    NewTrack { at: usize },
    /// Switch between the measure grid and the mixer.
    ToggleMixer,
    /// Run a command, then show `track` (a track that was added or moved).
    CommandShowing { command: Command, track: usize },
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
        let master = document.song().master_mix();
        let measure_count = document.song().measure_count();
        let playing = self.playback.current_measure().map(|m| m.saturating_sub(1));
        let levels: Vec<f32> = (0..rows.len())
            .map(|index| self.playback.track_level(TrackIndex::new(index)))
            .collect();
        let mut actions = Vec::new();
        draw(
            ui,
            &rows,
            master,
            &grid,
            GridView {
                measure_count,
                cursor_measure: cursor.measure.get(),
                cursor_track: cursor.track.get(),
                playing,
                selection: self.view.selection,
                shown_track: self.view.track,
                mixer: self.view.grid_mixer,
            },
            &levels,
            &mut self.mixer,
            &mut actions,
        );
        for action in actions {
            self.apply_grid_action(action);
        }
    }

    /// Runs one action from the grid (or the score's right-click menu).
    pub(super) fn apply_grid_action(&mut self, action: GridAction) {
        match action {
            GridAction::Command { command, merge } => self.apply_command(command, merge),
            GridAction::CommandShowing { command, track } => {
                let revision = self.document.as_ref().map(|d| d.editor().revision());
                self.apply_command(command, false);
                if self.document.as_ref().map(|d| d.editor().revision()) != revision {
                    self.view.track = track;
                }
            }
            GridAction::NewTrack { at } => self.begin_new_track(at),
            GridAction::ToggleMixer => {
                self.view.grid_mixer = !self.view.grid_mixer;
                self.save_settings();
            }
            GridAction::BreakMerge => {
                if let Some(document) = &mut self.document {
                    document.editor_mut().break_merge();
                }
            }
            GridAction::MoveCursor { track, measure } => self.move_cursor_to(track, measure),
            GridAction::Select(selection) => {
                self.view.selection = Some(selection);
                if let Some(document) = &mut self.document {
                    document
                        .editor_mut()
                        .set_selection(Some(gscore_edit::Selection::Measures {
                            anchor: gscore_model::MeasureIndex::new(selection.start()),
                            focus: gscore_model::MeasureIndex::new(selection.end()),
                        }));
                }
                // While playing, playback goes to the selection's first measure, unless
                // that measure is already playing (the selection just follows it).
                let playing_measure = if self.playback.state() == gscore_play::PlayState::Playing {
                    self.playback
                        .current_measure()
                        .map(|measure| measure.saturating_sub(1))
                } else {
                    None
                };
                if playing_measure != Some(selection.start()) {
                    self.jump_playback_to(self.view.track, selection.start());
                }
            }
            GridAction::Rename { track } => {
                let name = self
                    .document
                    .as_ref()
                    .and_then(|document| document.song().tracks().nth(track))
                    .map_or_else(String::new, |row| row.name().to_owned());
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

    /// Moves the cursor to the first beat of `track`, `measure`. While playing, choosing a
    /// track (or the measure that is already playing) keeps playback going where it is; only
    /// a different measure seeks.
    fn move_cursor_to(&mut self, track: usize, measure: usize) {
        let Some(document) = &mut self.document else {
            return;
        };
        let playing_measure = if self.playback.state() == gscore_play::PlayState::Playing {
            self.playback
                .current_measure()
                .map(|measure| measure.saturating_sub(1))
        } else {
            None
        };
        let same_measure = playing_measure == Some(measure);
        let mut cursor = document.editor().cursor();
        cursor.track = TrackIndex::new(track);
        cursor.measure = gscore_model::MeasureIndex::new(measure);
        if !same_measure {
            cursor.voice = gscore_model::VoiceIndex::new(0);
            cursor.beat = gscore_model::BeatIndex::new(0);
        }
        document.editor_mut().set_cursor(cursor);
        self.view.track = track;
        if !same_measure {
            self.jump_playback_to(track, measure);
        }
    }

    /// While playing, continues playback at the first beat of `track`, `measure`.
    fn jump_playback_to(&mut self, track: usize, measure: usize) {
        if self.playback.count_in_pending() {
            // A count-in for the old place is pointless now.
            if let Err(error) = self.playback.stop() {
                self.notice = Some(error);
            }
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let at = gscore_model::BeatPosition {
            track: TrackIndex::new(track),
            measure: gscore_model::MeasureIndex::new(measure),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(0),
        };
        if let Err(error) = self.playback.jump_to(document.song(), at) {
            self.notice = Some(error);
        }
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
    mixer: bool,
}

#[allow(clippy::too_many_arguments)]
fn draw(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: gscore_model::MasterMix,
    grid: &GridData,
    view: GridView,
    levels: &[f32],
    mixer_state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    if view.mixer {
        bar::header(ui, rows, view, actions);
        strips::strips(ui, rows, master, levels, mixer_state, actions);
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal_top(|ui| {
                rows::headers(ui, rows, master, view, levels, actions);
                egui::ScrollArea::horizontal()
                    .id_salt("song_grid")
                    .show(ui, |ui| matrix::matrix(ui, rows, grid, view, actions));
            });
        });
}
