//! The fields of the staff dialogs: one small form per tile (tempo, time and key
//! signature, section marker, repeats and endings, barline, break, bars per line).

use egui::RichText;
use gscore_model::{
    Feel, Fermata, MAX_BARS_PER_SYSTEM, MAX_SECTION_TITLE_CHARS, MAX_TEMPO, SystemBreak,
};

use super::edit_staff::{Dialog, StaffState, Target};

/// The window title of a dialog.
pub(super) fn title(dialog: Dialog) -> &'static str {
    match dialog {
        Dialog::Tempo => "Tempo",
        Dialog::Time => "Time signature",
        Dialog::Key => "Key signature",
        Dialog::Section => "Section",
        Dialog::RepeatStart => "Start repeat",
        Dialog::RepeatEnd => "End repeat",
        Dialog::Endings => "Alternative endings",
        Dialog::Feel => "Swing feel",
        Dialog::Fermata => "Fermata",
        Dialog::Barline => "Barline",
        Dialog::Break => "Line break",
        Dialog::Bars => "Bars per line",
    }
}

/// The dialog's fields.
pub(super) fn body(ui: &mut egui::Ui, state: &mut StaffState, target: Target) {
    state.error = None;
    match target.dialog {
        Dialog::Tempo => {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut state.tempo)
                        .range(1..=MAX_TEMPO)
                        .suffix(" BPM"),
                );
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Tempo value"));
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("The song keeps this tempo until another change.")
                    .small()
                    .weak(),
            );
        }
        Dialog::Time => {
            ui.horizontal(|ui| {
                let response = ui.add(egui::DragValue::new(&mut state.numerator).range(1..=64));
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Time numerator");
                });
                ui.label("/");
                egui::ComboBox::from_id_salt("staff_time_denominator")
                    .selected_text(state.denominator.to_string())
                    .width(64.0)
                    .show_ui(ui, |ui| {
                        for value in [1_u16, 2, 4, 8, 16, 32, 64, 128] {
                            ui.selectable_value(&mut state.denominator, value, value.to_string());
                        }
                    });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("The upper number counts the beats; the lower says their value.")
                    .small()
                    .weak(),
            );
        }
        Dialog::Key => {
            ui.horizontal(|ui| {
                let response = ui.add(egui::DragValue::new(&mut state.key).range(-7..=7));
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Key accidentals");
                });
                ui.checkbox(&mut state.minor, "minor");
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Positive counts sharps, negative flats (-7 to 7).")
                    .small()
                    .weak(),
            );
        }
        Dialog::Section => {
            ui.horizontal(|ui| {
                let response = ui.color_edit_button_srgb(&mut state.section_color);
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Section color");
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.section_title)
                        .hint_text("Section title")
                        .desired_width(200.0),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Section title");
                });
            });
            if state.section_title.chars().count() > MAX_SECTION_TITLE_CHARS {
                state.error = Some(format!(
                    "The title is longer than {MAX_SECTION_TITLE_CHARS} characters."
                ));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Leave the title empty to remove the marker.")
                    .small()
                    .weak(),
            );
        }
        Dialog::RepeatStart => {
            ui.checkbox(&mut state.repeat_start, "Start repeat at this bar");
            ui.add_space(4.0);
            ui.label(
                RichText::new("The repeat runs to the bar with the next end repeat.")
                    .small()
                    .weak(),
            );
        }
        Dialog::RepeatEnd => {
            ui.checkbox(&mut state.repeat_end, "End the repeat at this bar");
            ui.add_enabled_ui(state.repeat_end, |ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut state.repeat_count)
                        .range(1..=127)
                        .suffix(" ×"),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Repeat count");
                });
                ui.label("played again");
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("The section plays once, then this many times again.")
                    .small()
                    .weak(),
            );
        }
        Dialog::Endings => {
            ui.label("This bar is played on these passes:");
            ui.horizontal(|ui| {
                for number in 1..=4_u8 {
                    let bit = 1_u8 << number.saturating_sub(1);
                    let mut on = state.endings & bit != 0;
                    let label = format!("Ending {number}");
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            state.endings |= bit;
                        } else {
                            state.endings &= !bit;
                        }
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("A bar can belong to more than one ending.")
                    .small()
                    .weak(),
            );
        }
        Dialog::Feel => {
            for feel in Feel::ALL {
                ui.radio_value(&mut state.feel, feel, feel.label());
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Guitar Pro stores eighth- and sixteenth-note triplet feel; dotted \
                     feels are kept in GuitarScore's own GP7/GPX attribute.",
                )
                .small()
                .weak(),
            );
        }
        Dialog::Fermata => {
            ui.radio_value(&mut state.fermata, None, "None");
            for fermata in Fermata::ALL {
                ui.radio_value(&mut state.fermata, Some(fermata), fermata.label());
            }
        }
        Dialog::Barline => {
            ui.checkbox(&mut state.double_bar, "End this bar with a double barline");
            ui.add_space(4.0);
            ui.label(
                RichText::new("A double barline divides sections of the score.")
                    .small()
                    .weak(),
            );
        }
        Dialog::Break => {
            let first = target.measure.get() == 0;
            if first {
                ui.label(
                    RichText::new("The first bar always starts the score.")
                        .small()
                        .weak(),
                );
            }
            ui.add_enabled_ui(!first, |ui| {
                ui.radio_value(&mut state.system_break, SystemBreak::None, "No break");
                ui.radio_value(
                    &mut state.system_break,
                    SystemBreak::Line,
                    "Start a new line here",
                );
                ui.radio_value(
                    &mut state.system_break,
                    SystemBreak::Page,
                    "Start a new page here",
                );
            });
        }
        Dialog::Bars => {
            ui.radio_value(&mut state.bars_fixed, false, "As many as fit");
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.bars_fixed, true, "Fixed:");
                ui.add_enabled_ui(state.bars_fixed, |ui| {
                    let response = ui
                        .add(egui::DragValue::new(&mut state.bars).range(1..=MAX_BARS_PER_SYSTEM));
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Bars per line value");
                    });
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("This applies to the whole shown track.")
                    .small()
                    .weak(),
            );
        }
    }
}
