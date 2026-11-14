//! How the track looks: its color (a palette of presets, or any color), and which staves
//! its page shows.

use gscore_edit::Command;
use gscore_model::{TrackColor, TrackIndex, TrackStaves};

use super::TrackPanel;
use crate::app::tiles::{self, Face};
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

/// The tile of standard notation: a treble clef.
pub(in crate::app) const NOTATION_FACE: Face<'static> = Face::Glyph {
    text: "\u{E050}",
    size: 17.0,
    baseline: 4.0,
};
/// The tile of tablature: the tab clef.
pub(in crate::app) const TABLATURE_FACE: Face<'static> = Face::Glyph {
    text: "\u{E06D}",
    size: 17.0,
    baseline: 0.0,
};
/// The tile of slash notation: a rhythm slash, as the slash staff writes a beat.
pub(in crate::app) const SLASH_FACE: Face<'static> = Face::Glyph {
    text: "\u{E101}",
    size: 34.0,
    baseline: 0.0,
};

/// Three independent switches — standard notation, tablature and slash notation — with at
/// least one always on.
pub(super) fn staves(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.add_space(4.0);
    // A drum kit is always written on a percussion staff; its one choice is the drum tab
    // below it, which shows each hit's MIDI drum note.
    if snapshot.percussion {
        let staves = snapshot.staves;
        let on = staves.tablature();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let response = tiles::tile(
                ui,
                TABLATURE_FACE,
                on,
                true,
                "Drum tablature",
                "Show a tablature below the percussion staff: each hit's MIDI drum note on \
                 the line of the kit it is stored on",
            );
            ui.label(egui::RichText::new("Percussion staff").color(style::tokens(ui).text_weak));
            if response.clicked() {
                // The percussion staff always shows, so the notation switch stays on with
                // the tab off (a track must keep one staff).
                let next = if on {
                    TrackStaves::NOTATION_ONLY
                } else {
                    staves.with_tablature(true)
                };
                if next != staves {
                    commands.push((
                        Command::SetTrackStaves {
                            track: TrackIndex::new(index),
                            staves: next,
                        },
                        false,
                    ));
                }
            }
        });
        return;
    }
    // Notation needs pitches (a tuning, or a voice's stand-in strings); slash notation
    // does not.
    let notation_possible = snapshot.has_pitches;
    let staves = snapshot.staves;
    let count = staves.count();
    let track = TrackIndex::new(index);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (staff, face, label, tip, on, possible) in [
            (
                Staff::Notation,
                NOTATION_FACE,
                "Standard notation",
                "Show standard notation",
                staves.notation(),
                notation_possible,
            ),
            (
                Staff::Tablature,
                TABLATURE_FACE,
                "Tablature",
                "Show the tablature",
                staves.tablature(),
                true,
            ),
            (
                Staff::Slash,
                SLASH_FACE,
                "Slash notation",
                "Show rhythm slashes instead of pitches",
                staves.slash(),
                true,
            ),
        ] {
            let last_one = on && count == 1;
            let enabled = possible && !last_one;
            let tip = if last_one {
                format!("{tip}\nAt least one staff must stay on")
            } else if !possible {
                format!("{tip}\nThis track has no tuning to write notes from")
            } else {
                tip.to_owned()
            };
            let response = tiles::tile(ui, face, on, enabled, label, &tip);
            if response.clicked() && enabled {
                let next = match staff {
                    Staff::Notation => staves.with_notation(!on),
                    Staff::Tablature => staves.with_tablature(!on),
                    Staff::Slash => staves.with_slash(!on),
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

/// The three staves a track can show.
#[derive(Debug, Clone, Copy)]
enum Staff {
    Notation,
    Tablature,
    Slash,
}
