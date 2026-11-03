//! How the track looks: its color (a palette of presets, or any color), and which staves
//! its page shows.

use gscore_edit::Command;
use gscore_model::{TrackColor, TrackIndex, TrackStaves};

use super::TrackPanel;
use crate::app::{style, swatches};

/// The track color: one click on a preset, or the color picker for any other.
pub(super) fn color(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    let track = TrackIndex::new(index);
    ui.label(egui::RichText::new("Color").color(style::tokens(ui).text_weak));
    if let Some(color) = swatches::presets(ui, snapshot.color) {
        commands.push((Command::SetTrackColor { track, color }, false));
    }
    // Any other color.
    let (red, green, blue) = snapshot.color.parts();
    let mut rgb = [red, green, blue];
    let (mut changed, mut dragged) = (false, false);
    ui.horizontal(|ui| {
        ui.label("Custom");
        let response = ui.color_edit_button_srgb(&mut rgb);
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Track color"));
        changed = response.changed();
        dragged = response.dragged();
    });
    let [red, green, blue] = rgb;
    let color = TrackColor::from_parts(red, green, blue);
    if changed && color != snapshot.color {
        // A color-picker drag is one undo step, like a slider drag.
        commands.push((Command::SetTrackColor { track, color }, dragged));
    }
}

/// Notes and tabs, tabs only, or notes only.
pub(super) fn staves(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Show").color(style::tokens(ui).text_weak));
    // A drum kit is always written on a percussion staff; there is nothing to choose.
    if snapshot.percussion {
        ui.label(egui::RichText::new("Percussion staff").color(style::tokens(ui).text_weak));
        return;
    }
    // Notation needs a tuning: without one, tabs only.
    let notation_possible = !snapshot.tuning.is_empty();
    ui.horizontal_wrapped(|ui| {
        for staves in TrackStaves::ALL {
            let enabled = notation_possible || staves == TrackStaves::TabOnly;
            let shown = if notation_possible {
                snapshot.staves
            } else {
                TrackStaves::TabOnly
            };
            let response = ui
                .add_enabled(
                    enabled,
                    egui::Button::selectable(shown == staves, staves.label()),
                )
                .on_disabled_hover_text("This track has no tuning to write notes from");
            if response.clicked() && staves != snapshot.staves {
                commands.push((
                    Command::SetTrackStaves {
                        track: TrackIndex::new(index),
                        staves,
                    },
                    false,
                ));
            }
        }
    });
}
