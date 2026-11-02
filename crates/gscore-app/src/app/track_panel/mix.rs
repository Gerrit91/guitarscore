//! The track panel's bar of track actions (add, move up and down, mute and solo all) and
//! its master row (the volume and pan of all tracks together).

use egui::{RichText, Sense, vec2};
use gscore_edit::Command;
use gscore_model::TrackIndex;

use super::TrackPanel;
use crate::app::grid::mixer::pan_knob;
use crate::app::icons::{self, Icon};

/// What the bar and the master row asked for this frame.
#[derive(Debug, Default)]
pub(super) struct BarActions {
    /// One command, and whether it continues the current drag.
    pub(super) command: Option<(Command, bool)>,
    /// The track to show once the command ran (an added or moved track).
    pub(super) show_track: Option<usize>,
    /// A drag started or ended: the next command starts a new undo step.
    pub(super) break_merge: bool,
}

/// Add a track after this one, move it up or down, mute all, solo all.
pub(super) fn action_bar(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    actions: &mut BarActions,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let after = index.saturating_add(1);
        if icons::button(ui, Icon::Add, "New track", "Add a track after this one").clicked() {
            actions.command = Some((
                Command::InsertTrack {
                    at: TrackIndex::new(after),
                },
                false,
            ));
            actions.show_track = Some(after);
        }
        ui.add_enabled_ui(index > 0, |ui| {
            if icons::button(ui, Icon::MoveUp, "Move track up", "Move this track up").clicked() {
                let to = index.saturating_sub(1);
                actions.command = Some((
                    Command::MoveTrack {
                        from: TrackIndex::new(index),
                        to: TrackIndex::new(to),
                    },
                    false,
                ));
                actions.show_track = Some(to);
            }
        });
        ui.add_enabled_ui(after < snapshot.tracks, |ui| {
            if icons::button(
                ui,
                Icon::MoveDown,
                "Move track down",
                "Move this track down",
            )
            .clicked()
            {
                actions.command = Some((
                    Command::MoveTrack {
                        from: TrackIndex::new(index),
                        to: TrackIndex::new(after),
                    },
                    false,
                ));
                actions.show_track = Some(after);
            }
        });
        ui.separator();
        let tracks = snapshot.tracks;
        let all = |make: &dyn Fn(TrackIndex) -> Command| Command::Batch {
            commands: (0..tracks).map(|t| make(TrackIndex::new(t))).collect(),
        };
        let mute = !snapshot.all_muted;
        let tip = if mute {
            "Mute all tracks"
        } else {
            "Unmute all tracks"
        };
        if toggle(ui, "M", "Mute all", snapshot.all_muted, tip) {
            actions.command = Some((all(&|track| Command::SetTrackMute { track, mute }), false));
        }
        let solo = !snapshot.all_soloed;
        let tip = if solo {
            "Solo all tracks"
        } else {
            "Unsolo all tracks"
        };
        if toggle(ui, "S", "Solo all", snapshot.all_soloed, tip) {
            actions.command = Some((all(&|track| Command::SetTrackSolo { track, solo }), false));
        }
    });
}

/// A small letter toggle like the song grid's mute and solo buttons; true when clicked.
fn toggle(ui: &mut egui::Ui, letter: &str, label: &str, on: bool, tip: &str) -> bool {
    let response = ui
        .add(
            egui::Button::selectable(on, RichText::new(letter).strong()).min_size(vec2(22.0, 22.0)),
        )
        .on_hover_text(tip);
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(label));
    response.clicked()
}

/// The master volume slider and pan knob.
pub(super) fn master_row(ui: &mut egui::Ui, snapshot: &TrackPanel, actions: &mut BarActions) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.strong("Master");
        let mut volume = snapshot.master.volume;
        let slider = ui.add(
            egui::Slider::new(&mut volume, 0..=127)
                .show_value(false)
                .trailing_fill(true)
                .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }),
        );
        ui.ctx()
            .accesskit_node_builder(slider.id, |node| node.set_label("Master volume"));
        let percent = u32::from(volume) * 100 / u32::from(gscore_model::MASTER_UNITY);
        let slider = slider.on_hover_text(format!(
            "Volume of all tracks: {percent}% (double-click: 100%)"
        ));
        if slider.double_clicked() {
            volume = gscore_model::MASTER_UNITY;
        }
        if slider.drag_started() || slider.drag_stopped() {
            actions.break_merge = true;
        }
        if (slider.changed() || slider.double_clicked()) && volume != snapshot.master.volume {
            let merge = slider.dragged() && !slider.drag_started() && !slider.clicked();
            actions.command = Some((Command::SetMasterVolume { volume }, merge));
        }
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        let knob = pan_knob(
            ui,
            rect.shrink(1.0),
            snapshot.master.pan,
            usize::MAX,
            "Master pan",
        );
        if knob.boundary {
            actions.break_merge = true;
        }
        if let Some((pan, merge)) = knob.change {
            actions.command = Some((Command::SetMasterPan { pan }, merge));
        }
    });
    ui.add_space(2.0);
}
