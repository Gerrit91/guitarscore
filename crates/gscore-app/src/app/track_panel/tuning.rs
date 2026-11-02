//! The tuning: string count, capo, and each string's open note, chosen by name (E2, F#3);
//! the MIDI numbers stay behind the scenes.

use egui::RichText;
use gscore_edit::Command;
use gscore_model::{MAX_STRINGS, Pitch, TrackIndex};

use super::TrackPanel;

/// The notes a string can be tuned to in the list: C0 to B7 (MIDI 12 to 107), which covers
/// every string instrument. A tuning outside it is still shown and can be stepped.
const LOWEST: u8 = 12;
const HIGHEST: u8 = 107;

pub(super) fn tuning(
    ui: &mut egui::Ui,
    index: usize,
    snapshot: &TrackPanel,
    commands: &mut Vec<(Command, bool)>,
) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("Tuning");
        if let Some(name) = &snapshot.tuning_name {
            ui.label(RichText::new(name).weak());
        }
    });
    let track = TrackIndex::new(index);
    let mut strings = snapshot.string_count;
    let mut capo = snapshot.capo;
    ui.horizontal(|ui| {
        ui.label("Strings");
        let response = ui.add(
            egui::DragValue::new(&mut strings)
                .range(1..=MAX_STRINGS)
                .speed(1)
                .update_while_editing(false),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Strings"));
        ui.add_space(8.0);
        ui.label("Capo");
        let response = ui.add(
            egui::DragValue::new(&mut capo)
                .range(0..=127)
                .speed(1)
                .update_while_editing(false),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Capo"));
        if capo == 0 {
            ui.label(RichText::new("none").weak());
        }
    });
    if strings != snapshot.string_count {
        commands.push((Command::SetTrackStrings { track, strings }, false));
    }
    if capo != snapshot.capo {
        commands.push((Command::SetTrackCapo { track, capo }, false));
    }
    if snapshot.tuning.is_empty() {
        ui.label(RichText::new("This track has no tuning").weak());
    }
    for string in &snapshot.tuning {
        let number = string.string.get();
        let current = string.pitch.get();
        let mut pitch = current;
        ui.horizontal(|ui| {
            ui.label(format!("String {number}"));
            if ui
                .small_button("−")
                .on_hover_text("A semitone lower")
                .clicked()
            {
                pitch = pitch.saturating_sub(1);
            }
            let combo = egui::ComboBox::from_id_salt(("string_pitch", index, number))
                .selected_text(note_name(current))
                .width(56.0)
                .show_ui(ui, |ui| {
                    for candidate in (LOWEST..=HIGHEST).rev() {
                        ui.selectable_value(&mut pitch, candidate, note_name(candidate));
                    }
                });
            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                node.set_label(format!("String {number} note"));
            });
            if ui
                .small_button("+")
                .on_hover_text("A semitone higher")
                .clicked()
            {
                pitch = pitch.saturating_add(1).min(127);
            }
        });
        if pitch != current
            && let Ok(pitch) = Pitch::new(pitch)
        {
            commands.push((
                Command::SetTrackStringPitch {
                    track,
                    string: string.string,
                    pitch,
                },
                false,
            ));
        }
    }
}

/// The note name of a MIDI pitch ("E2", "F#3").
pub(super) fn note_name(pitch: u8) -> String {
    Pitch::new(pitch).map_or_else(|_| "?".to_owned(), |pitch| pitch.to_string())
}
