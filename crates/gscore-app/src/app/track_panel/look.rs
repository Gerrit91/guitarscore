//! How the track looks: its color (a palette of presets, or any color), and which staves
//! its page shows.

use gscore_edit::Command;
use gscore_model::{TrackColor, TrackIndex};

use super::TrackPanel;
use crate::app::{style, swatches};

/// The track color: one click on a preset, or the color picker for any other.
pub(super) fn color(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) -> bool {
    let track = TrackIndex::new(index);
    let mut chosen = false;
    ui.label(egui::RichText::new("Color").color(style::tokens(ui).text_weak));
    if let Some(color) = swatches::presets(ui, snapshot.color) {
        commands.push((Command::SetTrackColor { track, color }, false));
        chosen = true;
    }
    // Any other color.
    let (red, green, blue) = snapshot.color.parts();
    let mut rgb = [red, green, blue];
    let (mut changed, mut dragged) = (false, false);
    ui.horizontal(|ui| {
        ui.label("Custom");
        let response = ui.color_edit_button_srgb(&mut rgb);
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Custom track color"));
        changed = response.changed();
        dragged = response.dragged();
    });
    let [red, green, blue] = rgb;
    let color = TrackColor::from_parts(red, green, blue);
    if changed && color != snapshot.color {
        // A color-picker drag is one undo step, like a slider drag.
        commands.push((Command::SetTrackColor { track, color }, dragged));
    }
    chosen
}

/// Three independent switches — standard notation, tablature and slash notation — with at
/// least one always on.
pub(super) fn staves(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    use super::super::icons::{self, Icon};
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Show").color(style::tokens(ui).text_weak));
    // A drum kit is always written on a percussion staff; there is nothing to choose.
    if snapshot.percussion {
        ui.label(egui::RichText::new("Percussion staff").color(style::tokens(ui).text_weak));
        return;
    }
    // Notation needs a tuning; slash notation does not.
    let notation_possible = !snapshot.tuning.is_empty();
    let staves = snapshot.staves;
    let count = staves.count();
    let track = TrackIndex::new(index);
    ui.horizontal(|ui| {
        for (icon, label, tip, on, possible) in [
            (
                Icon::Notation,
                "Standard notation",
                "Show standard notation",
                staves.notation(),
                notation_possible,
            ),
            (
                Icon::Tablature,
                "Tablature",
                "Show the tablature",
                staves.tablature(),
                true,
            ),
            (
                Icon::Slash,
                "Slash notation",
                "Show rhythm slashes instead of pitches",
                staves.slash(),
                true,
            ),
        ] {
            let last_one = on && count == 1;
            let enabled = possible && !last_one;
            let response =
                ui.add_enabled_ui(enabled, |ui| icons::toggle_button(ui, icon, label, tip, on));
            let response = response.inner;
            if last_one {
                response
                    .clone()
                    .on_hover_text("At least one staff must stay on");
            } else if !possible {
                response
                    .clone()
                    .on_hover_text("This track has no tuning to write notes from");
            }
            if response.clicked() && possible && !last_one {
                let next = match icon {
                    Icon::Notation => staves.with_notation(!on),
                    Icon::Tablature => staves.with_tablature(!on),
                    _ => staves.with_slash(!on),
                };
                if next != staves {
                    commands.push((
                        Command::SetTrackStaves {
                            track,
                            staves: next,
                        },
                        false,
                    ));
                }
            }
        }
    });
}
