//! The track column of the song grid: one row per track with its color, name and mixer.

use egui::{Align2, Color32, FontId, Rect, Sense, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{InstrumentKind, MasterMix, TrackColor, TrackIndex, TrackSound};

use super::{GridAction, GridView, ROW_HEIGHT, RULER_HEIGHT, TRACK_COLUMN, bar, mixer};
use crate::document::Document;

/// What a track row shows; a snapshot taken before drawing so the song is not borrowed
/// while the widgets run.
pub(super) struct TrackRow {
    pub(super) name: String,
    pub(super) color: TrackColor,
    pub(super) muted: bool,
    pub(super) soloed: bool,
    /// The three-letter instrument tag shown in the row.
    tag: String,
    /// The instrument's full name, for the row's tooltip.
    description: String,
    sound: Option<TrackSound>,
}

/// A snapshot of the track rows.
pub(super) fn track_rows(document: &Document) -> Vec<TrackRow> {
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

/// The fixed left column: the header with track actions, then color, instrument, name and
/// mixer for every track, then the master row.
pub(super) fn headers(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: MasterMix,
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let height = RULER_HEIGHT + (rows.len() as f32 + 1.0) * ROW_HEIGHT;
    ui.allocate_ui_with_layout(
        vec2(TRACK_COLUMN, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            bar::header(ui, rows, view, actions);
            for (index, row) in rows.iter().enumerate() {
                track_header(ui, index, rows.len(), row, view, actions);
            }
            bar::master_row(ui, master, actions);
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
        actions.push(GridAction::Command {
            command: Command::SetTrackMute {
                track: TrackIndex::new(index),
                mute: !row.muted,
            },
            merge: false,
        });
    }
    if ui
        .put(solo, egui::Button::new("S").selected(row.soloed).small())
        .on_hover_text("Solo this track")
        .clicked()
    {
        actions.push(GridAction::Command {
            command: Command::SetTrackSolo {
                track: TrackIndex::new(index),
                solo: !row.soloed,
            },
            merge: false,
        });
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
    if volume_response.drag_started() || volume_response.drag_stopped() {
        actions.push(GridAction::BreakMerge);
    }
    if volume_response.changed() && volume != volume_value {
        actions.push(GridAction::Command {
            command: Command::SetTrackVolume {
                track: TrackIndex::new(index),
                volume,
            },
            merge: volume_response.dragged()
                && !volume_response.drag_started()
                && !volume_response.clicked(),
        });
    }
    let pan_rect = Rect::from_center_size(
        pos2(controls.left() + 148.0, rect.center().y),
        vec2(16.0, 16.0),
    );
    let knob = mixer::pan_knob(ui, pan_rect, pan_value, index, "Pan");
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((pan, merge)) = knob.change {
        actions.push(GridAction::Command {
            command: Command::SetTrackPan {
                track: TrackIndex::new(index),
                pan,
            },
            merge,
        });
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
        if ui.button("Edit track…").clicked() {
            actions.push(GridAction::ShowTrack { track: index });
            ui.close();
        }
        ui.separator();
        if ui.button("Add track below").clicked() {
            actions.push(GridAction::Command {
                command: Command::InsertTrack {
                    at: TrackIndex::new(index.saturating_add(1)),
                },
                merge: false,
            });
            ui.close();
        }
        if ui
            .add_enabled(count > 1, egui::Button::new("Delete track"))
            .clicked()
        {
            actions.push(GridAction::Command {
                command: Command::DeleteTrack {
                    track: TrackIndex::new(index),
                },
                merge: false,
            });
            ui.close();
        }
        if ui.button("Duplicate track").clicked() {
            actions.push(GridAction::Command {
                command: Command::DuplicateTrack {
                    track: TrackIndex::new(index),
                },
                merge: false,
            });
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
            actions.push(GridAction::Command {
                command: Command::MoveTrack {
                    from: TrackIndex::new(index),
                    to: TrackIndex::new(index.saturating_sub(1)),
                },
                merge: false,
            });
            ui.close();
        }
        if ui
            .add_enabled(
                index.saturating_add(1) < count,
                egui::Button::new("Move down"),
            )
            .clicked()
        {
            actions.push(GridAction::Command {
                command: Command::MoveTrack {
                    from: TrackIndex::new(index),
                    to: TrackIndex::new(index.saturating_add(1)),
                },
                merge: false,
            });
            ui.close();
        }
    });
}

/// A track color as an egui color.
pub(super) fn color32(color: TrackColor) -> Color32 {
    let (red, green, blue) = color.parts();
    Color32::from_rgb(red, green, blue)
}
