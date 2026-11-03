//! How the track looks: its color (a palette of presets, or any color), and which staves
//! its page shows.

use egui::{Color32, Sense, Stroke, vec2};
use gscore_edit::Command;
use gscore_model::{TrackColor, TrackIndex, TrackStaves};

use super::TrackPanel;
use crate::app::style;

/// The preset colors: names (for tooltips and screen readers) and `rrggbb`.
pub(crate) const PRESETS: [(&str, u32); 16] = [
    ("Red", 0xE5_3935),
    ("Orange", 0xFB_8C00),
    ("Yellow", 0xFD_D835),
    ("Lime", 0x7C_B342),
    ("Green", 0x43_A047),
    ("Teal", 0x00_897B),
    ("Cyan", 0x00_ACC1),
    ("Sky", 0x03_9BE5),
    ("Blue", 0x1E_88E5),
    ("Indigo", 0x39_49AB),
    ("Purple", 0x8E_24AA),
    ("Pink", 0xD8_1B60),
    ("Brown", 0x6D_4C41),
    ("Grey", 0x75_7575),
    ("Slate", 0x54_6E7A),
    ("Black", 0x21_2121),
];

/// The track color: one click on a preset, or the color picker for any other.
pub(super) fn color(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    let track = TrackIndex::new(index);
    ui.label(egui::RichText::new("Color").color(style::tokens(ui).text_weak));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (name, rgb) in PRESETS {
            let preset = TrackColor::new(rgb);
            let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(format!("Color {name}"));
                node.set_role(egui::Role::Button);
            });
            let (red, green, blue) = preset.parts();
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgb(red, green, blue));
            // A thin edge keeps dark presets visible in the dark theme (and light ones in
            // the light theme).
            let edge = Stroke::new(1.0, ui.visuals().weak_text_color());
            ui.painter()
                .rect_stroke(rect, 3.0, edge, egui::StrokeKind::Inside);
            let chosen = preset == snapshot.color;
            if chosen || response.hovered() {
                let stroke = Stroke::new(2.0, ui.visuals().strong_text_color());
                ui.painter()
                    .rect_stroke(rect.expand(1.5), 4.0, stroke, egui::StrokeKind::Outside);
            }
            if response.on_hover_text(name).clicked() && !chosen {
                commands.push((
                    Command::SetTrackColor {
                        track,
                        color: preset,
                    },
                    false,
                ));
            }
        }
    });
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
    let color =
        TrackColor::new(u32::from(red) * 0x1_0000 + u32::from(green) * 0x100 + u32::from(blue));
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
    // Notation needs a tuning (and is not drawn for drums): without one, tabs only.
    let notation_possible = !snapshot.tuning.is_empty() && !snapshot.percussion;
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
