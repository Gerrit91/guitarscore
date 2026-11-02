//! The track and song grid (the bottom panel): one row per track with its color, instrument
//! and mixer controls, and a measure matrix that fills a square where a track has notes.
//!
//! Clicking a square moves the edit cursor there; dragging selects a range of measures (for
//! copy/paste and looping later); the measure being played is highlighted. Track commands
//! go through `gscore-edit`, so every change can be undone.

use egui::{Align2, Color32, FontId, Rect, Sense, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{InstrumentKind, TrackColor, TrackIndex, TrackSound};

use super::GuitarScoreApp;
use crate::document::{Document, GridData};
use crate::state::MeasureSelection;

mod matrix;
mod mixer;

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
    /// Run a track command.
    Command(Command),
    /// Move the cursor to the first beat of a track and measure.
    MoveCursor { track: usize, measure: usize },
    /// Replace the selected measures.
    Select(MeasureSelection),
    /// Open the rename window for a track.
    Rename { track: usize },
}

/// What a track row shows; a snapshot taken before drawing so the song is not borrowed
/// while the widgets run.
pub(super) struct TrackRow {
    name: String,
    color: TrackColor,
    muted: bool,
    soloed: bool,
    /// The three-letter instrument tag shown in the row.
    tag: String,
    /// The instrument's full name, for the row's tooltip.
    description: String,
    sound: Option<TrackSound>,
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
        let rows = track_rows(document);
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
                GridAction::Command(command) => self.apply_command(command),
                GridAction::MoveCursor { track, measure } => self.move_cursor_to(track, measure),
                GridAction::Select(selection) => self.view.selection = Some(selection),
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
            self.apply_command(Command::RenameTrack {
                track: TrackIndex::new(rename.track),
                name: rename.name,
            });
        } else if !cancel && open {
            self.renaming = Some(rename);
        }
    }
}

/// A snapshot of the track rows.
fn track_rows(document: &Document) -> Vec<TrackRow> {
    document
        .song()
        .tracks()
        .map(|track| TrackRow {
            name: track.name().to_owned(),
            color: track.color(),
            muted: track.is_muted(),
            soloed: track.is_soloed(),
            tag: instrument_tag(track.instrument_kind()).to_owned(),
            description: track
                .instrument_name()
                .map_or_else(|| track.name().to_owned(), str::to_owned),
            sound: document.song().track_sound(track.index()),
        })
        .collect()
}

/// A three-letter tag for the track column.
fn instrument_tag(kind: InstrumentKind) -> &'static str {
    match kind {
        InstrumentKind::Strings => "STR",
        InstrumentKind::Drums => "DRM",
        InstrumentKind::Other => "OTH",
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
                track_headers(ui, rows, view, actions);
                egui::ScrollArea::horizontal()
                    .id_salt("song_grid")
                    .show(ui, |ui| matrix::matrix(ui, rows, grid, view, actions));
            });
        });
}

/// The fixed left column: color, instrument, name and mixer for every track.
fn track_headers(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let height = RULER_HEIGHT + rows.len() as f32 * ROW_HEIGHT;
    ui.allocate_ui_with_layout(
        vec2(TRACK_COLUMN, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (ruler, _) =
                ui.allocate_exact_size(vec2(TRACK_COLUMN, RULER_HEIGHT), Sense::hover());
            ui.painter().text(
                pos2(ruler.left() + 6.0, ruler.center().y),
                Align2::LEFT_CENTER,
                "Tracks",
                FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
            for (index, row) in rows.iter().enumerate() {
                track_header(ui, index, rows.len(), row, view, actions);
            }
        },
    );
}

/// One track row: click to show the track, right-click for track commands.
fn track_header(
    ui: &mut egui::Ui,
    index: usize,
    count: usize,
    row: &TrackRow,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(TRACK_COLUMN, ROW_HEIGHT), Sense::hover());
    let response = ui.interact(
        rect,
        ui.scope_id().with(("track_row", index)),
        Sense::click(),
    );
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("Track {}: {}", index.saturating_add(1), row.name));
        node.set_role(egui::Role::Button);
    });
    let painter = ui.painter().clone();
    let visuals = ui.visuals().clone();
    if index % 2 == 1 {
        painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
    }
    if index == view.shown_track {
        painter.rect_filled(rect, 0.0, visuals.selection.bg_fill.gamma_multiply(0.35));
    }
    // The track's color, then its instrument tag and name.
    let swatch = Rect::from_min_size(
        pos2(rect.left() + 3.0, rect.top() + 4.0),
        vec2(5.0, ROW_HEIGHT - 8.0),
    );
    painter.rect_filled(swatch, 1.0, color32(row.color));
    let tag_font = FontId::monospace(9.0);
    painter.text(
        pos2(swatch.right() + 5.0, rect.center().y),
        Align2::LEFT_CENTER,
        &row.tag,
        tag_font,
        visuals.weak_text_color(),
    );
    let name_rect = Rect::from_min_max(
        pos2(swatch.right() + 28.0, rect.top()),
        pos2(mixer::controls_rect(rect).left() - 4.0, rect.bottom()),
    );
    painter.with_clip_rect(name_rect).text(
        pos2(name_rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        &row.name,
        FontId::proportional(12.0),
        visuals.text_color(),
    );

    // Mute and solo, on the right of the name.
    let controls = mixer::controls_rect(rect);
    let mute = Rect::from_center_size(
        pos2(controls.left() + 10.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    let solo = Rect::from_center_size(
        pos2(controls.left() + 32.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    if ui
        .put(mute, egui::Button::new("M").selected(row.muted).small())
        .on_hover_text("Mute this track")
        .clicked()
    {
        actions.push(GridAction::Command(Command::SetTrackMute {
            track: TrackIndex::new(index),
            mute: !row.muted,
        }));
    }
    if ui
        .put(solo, egui::Button::new("S").selected(row.soloed).small())
        .on_hover_text("Solo this track")
        .clicked()
    {
        actions.push(GridAction::Command(Command::SetTrackSolo {
            track: TrackIndex::new(index),
            solo: !row.soloed,
        }));
    }

    // Volume and pan.
    let volume_rect = Rect::from_min_size(
        pos2(controls.left() + 46.0, rect.center().y - 8.0),
        vec2(86.0, 16.0),
    );
    let (volume_value, pan_value) = row
        .sound
        .map_or((104, 64), |sound| (sound.volume, sound.pan));
    let mut volume = volume_value;
    let volume_response = ui.put(
        volume_rect,
        egui::Slider::new(&mut volume, 0..=127)
            .show_value(false)
            .trailing_fill(true)
            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
    );
    ui.ctx()
        .accesskit_node_builder(volume_response.id, |node| node.set_label("Volume"));
    if volume_response.changed() && volume != volume_value {
        actions.push(GridAction::Command(Command::SetTrackVolume {
            track: TrackIndex::new(index),
            volume,
        }));
    }
    let pan_rect = Rect::from_center_size(
        pos2(controls.left() + 148.0, rect.center().y),
        vec2(16.0, 16.0),
    );
    if let Some(pan) = mixer::pan_knob(ui, pan_rect, pan_value, index) {
        actions.push(GridAction::Command(Command::SetTrackPan {
            track: TrackIndex::new(index),
            pan,
        }));
    }

    response
        .clone()
        .on_hover_text(format!("{} — {}", row.description, row.name));
    if response.clicked() {
        let measure = view.cursor_measure;
        actions.push(GridAction::MoveCursor {
            track: index,
            measure,
        });
    }
    response.context_menu(|ui| {
        if ui.button("Add track below").clicked() {
            actions.push(GridAction::Command(Command::InsertTrack {
                at: TrackIndex::new(index.saturating_add(1)),
            }));
            ui.close();
        }
        if ui
            .add_enabled(count > 1, egui::Button::new("Delete track"))
            .clicked()
        {
            actions.push(GridAction::Command(Command::DeleteTrack {
                track: TrackIndex::new(index),
            }));
            ui.close();
        }
        if ui.button("Duplicate track").clicked() {
            actions.push(GridAction::Command(Command::DuplicateTrack {
                track: TrackIndex::new(index),
            }));
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            actions.push(GridAction::Rename { track: index });
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(index > 0, egui::Button::new("Move up"))
            .clicked()
        {
            actions.push(GridAction::Command(Command::MoveTrack {
                from: TrackIndex::new(index),
                to: TrackIndex::new(index.saturating_sub(1)),
            }));
            ui.close();
        }
        if ui
            .add_enabled(
                index.saturating_add(1) < count,
                egui::Button::new("Move down"),
            )
            .clicked()
        {
            actions.push(GridAction::Command(Command::MoveTrack {
                from: TrackIndex::new(index),
                to: TrackIndex::new(index.saturating_add(1)),
            }));
            ui.close();
        }
    });
}

/// A track color as an egui color.
fn color32(color: TrackColor) -> Color32 {
    let (red, green, blue) = color.parts();
    Color32::from_rgb(red, green, blue)
}
