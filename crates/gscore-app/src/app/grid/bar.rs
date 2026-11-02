//! The song grid's header above the track column (add a track, move the shown one up or
//! down, mute all, solo all) and its master row below the tracks (the volume and pan of all
//! tracks together). The mute/solo buttons and the master controls line up with the track
//! rows' own.

use egui::{Align2, FontId, Rect, Sense, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{MASTER_UNITY, MasterMix, TrackIndex};

use super::rows::TrackRow;
use super::{GridAction, GridView, ROW_HEIGHT, RULER_HEIGHT, TRACK_COLUMN, mixer};
use crate::app::icons::{self, Icon};

/// The header cell: the "Tracks" title, track actions, and mute/solo for all tracks.
pub(super) fn header(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    view: GridView,
    actions: &mut Vec<GridAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(TRACK_COLUMN, RULER_HEIGHT), Sense::hover());
    // The panel's title bar already says "Tracks".
    let shown = view.shown_track;
    let after = shown.saturating_add(1);
    let button_at = |slot: f32| {
        Rect::from_min_size(
            pos2(rect.left() + 2.0 + slot * 27.0, rect.center().y - 11.0),
            vec2(26.0, 22.0),
        )
    };
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    bar.scope_builder(egui::UiBuilder::new().max_rect(button_at(0.0)), |ui| {
        if icons::button(
            ui,
            Icon::Add,
            "New track",
            "Add a track after the shown one",
        )
        .clicked()
        {
            actions.push(GridAction::CommandShowing {
                command: Command::InsertTrack {
                    at: TrackIndex::new(after),
                },
                track: after,
            });
        }
    });
    bar.scope_builder(egui::UiBuilder::new().max_rect(button_at(1.0)), |ui| {
        ui.add_enabled_ui(shown > 0 && shown < rows.len(), |ui| {
            if icons::button(ui, Icon::MoveUp, "Move track up", "Move the shown track up").clicked()
            {
                let to = shown.saturating_sub(1);
                actions.push(GridAction::CommandShowing {
                    command: Command::MoveTrack {
                        from: TrackIndex::new(shown),
                        to: TrackIndex::new(to),
                    },
                    track: to,
                });
            }
        });
    });
    bar.scope_builder(egui::UiBuilder::new().max_rect(button_at(2.0)), |ui| {
        ui.add_enabled_ui(after < rows.len(), |ui| {
            if icons::button(
                ui,
                Icon::MoveDown,
                "Move track down",
                "Move the shown track down",
            )
            .clicked()
            {
                actions.push(GridAction::CommandShowing {
                    command: Command::MoveTrack {
                        from: TrackIndex::new(shown),
                        to: TrackIndex::new(after),
                    },
                    track: after,
                });
            }
        });
    });

    // Mute and solo for all tracks, above the rows' own buttons.
    let row = Rect::from_min_size(rect.min, vec2(TRACK_COLUMN, ROW_HEIGHT));
    let controls = mixer::controls_rect(row);
    let at = |dx: f32| {
        Rect::from_center_size(
            pos2(controls.left() + dx, rect.center().y),
            vec2(18.0, 18.0),
        )
    };
    let tracks = rows.len();
    let all = |make: &dyn Fn(TrackIndex) -> Command| Command::Batch {
        commands: (0..tracks).map(|t| make(TrackIndex::new(t))).collect(),
    };
    let all_muted = !rows.is_empty() && rows.iter().all(|row| row.muted);
    let mute = !all_muted;
    if toggle(
        &mut bar,
        at(10.0),
        "M",
        "Mute all",
        all_muted,
        if mute {
            "Mute all tracks"
        } else {
            "Unmute all tracks"
        },
    ) {
        actions.push(GridAction::Command {
            command: all(&|track| Command::SetTrackMute { track, mute }),
            merge: false,
        });
    }
    let all_soloed = !rows.is_empty() && rows.iter().all(|row| row.soloed);
    let solo = !all_soloed;
    if toggle(
        &mut bar,
        at(32.0),
        "S",
        "Solo all",
        all_soloed,
        if solo {
            "Solo all tracks"
        } else {
            "Unsolo all tracks"
        },
    ) {
        actions.push(GridAction::Command {
            command: all(&|track| Command::SetTrackSolo { track, solo }),
            merge: false,
        });
    }
}

/// A small letter toggle like a row's mute and solo buttons; true when clicked.
fn toggle(ui: &mut egui::Ui, rect: Rect, letter: &str, label: &str, on: bool, tip: &str) -> bool {
    let response = ui
        .put(rect, egui::Button::new(letter).selected(on).small())
        .on_hover_text(tip);
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(label));
    response.clicked()
}

/// The master row: the volume of all tracks and a pan that shifts all of them.
pub(super) fn master_row(ui: &mut egui::Ui, master: MasterMix, actions: &mut Vec<GridAction>) {
    let (rect, _) = ui.allocate_exact_size(vec2(TRACK_COLUMN, ROW_HEIGHT), Sense::hover());
    let visuals = ui.visuals().clone();
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        egui::Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.4)),
    );
    ui.painter().text(
        pos2(rect.left() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Master",
        FontId::proportional(12.0),
        visuals.strong_text_color(),
    );
    let controls = mixer::controls_rect(rect);
    let volume_rect = Rect::from_min_size(
        pos2(controls.left() + 46.0, rect.center().y - 8.0),
        vec2(86.0, 16.0),
    );
    let mut volume = master.volume;
    let slider = ui.put(
        volume_rect,
        egui::Slider::new(&mut volume, 0..=127)
            .show_value(false)
            .trailing_fill(true)
            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
    );
    ui.ctx()
        .accesskit_node_builder(slider.id, |node| node.set_label("Master volume"));
    let percent = u32::from(volume) * 100 / u32::from(MASTER_UNITY);
    let slider = slider.on_hover_text(format!(
        "Volume of all tracks: {percent}% (double-click: 100%)"
    ));
    if slider.double_clicked() {
        volume = MASTER_UNITY;
    }
    if slider.drag_started() || slider.drag_stopped() {
        actions.push(GridAction::BreakMerge);
    }
    if (slider.changed() || slider.double_clicked()) && volume != master.volume {
        let merge = slider.dragged() && !slider.drag_started() && !slider.clicked();
        actions.push(GridAction::Command {
            command: Command::SetMasterVolume { volume },
            merge,
        });
    }
    let pan_rect = Rect::from_center_size(
        pos2(controls.left() + 148.0, rect.center().y),
        vec2(16.0, 16.0),
    );
    let knob = mixer::pan_knob(ui, pan_rect, master.pan, usize::MAX, "Master pan");
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((pan, merge)) = knob.change {
        actions.push(GridAction::Command {
            command: Command::SetMasterPan { pan },
            merge,
        });
    }
}
