//! The track column of the song grid: one row per track with its color, name and mixer.

use egui::{Align2, Color32, FontId, Rect, Sense, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{MasterMix, TrackColor, TrackIndex, TrackSound};

use super::{GridAction, GridView, ROW_HEIGHT, RULER_HEIGHT, TRACK_COLUMN, bar, mixer};
use crate::app::instrument::Instrument;
use crate::document::Document;

/// What a track row shows; a snapshot taken before drawing so the song is not borrowed
/// while the widgets run.
pub(super) struct TrackRow {
    pub(super) name: String,
    pub(super) color: TrackColor,
    pub(super) muted: bool,
    pub(super) soloed: bool,
    /// The instrument family, shown as an icon.
    pub(super) instrument: Instrument,
    /// The instrument preset's name, for the mixer strip.
    pub(super) preset: String,
    /// The track's volume and pan, for the mixer strips.
    pub(super) volume: u8,
    pub(super) pan: u8,
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
            instrument: Instrument::of(&track, document.song().track_sound(track.index())),
            preset: document.song().track_sound(track.index()).map_or_else(
                || "No sound".to_owned(),
                |sound| {
                    if sound.percussion {
                        "Drum kit".to_owned()
                    } else {
                        gscore_model::program_name(sound.program).to_owned()
                    }
                },
            ),
            volume: document
                .song()
                .track_sound(track.index())
                .map_or(104, |sound| sound.volume),
            pan: document
                .song()
                .track_sound(track.index())
                .map_or(64, |sound| sound.pan),
            description: track
                .instrument_name()
                .map_or_else(|| track.name().to_owned(), str::to_owned),
            sound: document.song().track_sound(track.index()),
        })
        .collect()
}

/// The fixed left column: the header with track actions, then color, instrument, name and
/// mixer for every track, then the master row.
pub(super) fn headers(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: MasterMix,
    view: GridView,
    levels: &[f32],
    actions: &mut Vec<GridAction>,
) {
    let height = RULER_HEIGHT + (rows.len() as f32 + 1.0) * ROW_HEIGHT;
    ui.allocate_ui_with_layout(
        vec2(TRACK_COLUMN, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // Sliders and buttons fit inside a row: at the app's usual 24-pixel control
            // height a slider would reach below its row and push the next row down, out of
            // line with the matrix.
            ui.spacing_mut().interact_size.y = 16.0;
            bar::header(ui, rows, view, actions);
            for (index, row) in rows.iter().enumerate() {
                let level = levels.get(index).copied().unwrap_or(0.0);
                track_header(ui, index, rows.len(), row, view, level, actions);
            }
            let master_level = levels.iter().copied().fold(0.0_f32, f32::max)
                * f32::from(master.volume)
                / f32::from(gscore_model::MASTER_UNITY);
            bar::master_row(ui, master, master_level, actions);
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
    level: f32,
    actions: &mut Vec<GridAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(TRACK_COLUMN, ROW_HEIGHT), Sense::hover());
    // The row's widgets go into a child limited to the row: placing them in the column
    // itself would move its layout cursor and shift the rows below out of line with the
    // matrix.
    let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect));
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
        painter.rect_filled(rect, 3.0, visuals.selection.bg_fill.gamma_multiply(0.18));
    }
    // The track's color, then its instrument tag and name.
    let swatch = Rect::from_min_size(
        pos2(rect.left() + 3.0, rect.top() + 4.0),
        vec2(5.0, ROW_HEIGHT - 8.0),
    );
    painter.rect_filled(swatch, 1.0, color32(row.color));
    let icon = Rect::from_center_size(
        pos2(swatch.right() + 13.0, rect.center().y),
        vec2(14.0, 14.0),
    );
    row.instrument.paint(&painter, icon, visuals.text_color());
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
    let volume_rect = mixer::volume_rect(rect);
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
    volume_response
        .clone()
        .on_hover_text(format!("Volume {volume} — {}", mixer::db_text(volume)));
    mixer::paint_db(ui, rect, volume, volume_response.dragged());
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
    let pan_rect = mixer::pan_rect(rect);
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
    mixer::paint_activity(ui, mixer::activity_rect(rect), level);
    let meter = mixer::activity_rect(rect);
    let activity = ui.interact(
        meter,
        ui.scope_id().with(("activity", index)),
        Sense::hover(),
    );
    ui.ctx().accesskit_node_builder(activity.id, |node| {
        node.set_label(format!("Output activity {}", row.name));
    });
    activity.on_hover_text("Output activity (estimated from recent notes)");

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
            actions.push(GridAction::NewTrack {
                at: index.saturating_add(1),
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
