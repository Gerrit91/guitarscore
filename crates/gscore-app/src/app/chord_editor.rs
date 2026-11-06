//! The chord diagram dialog: a name, the base fret and a fret (or x) per string.

use egui::{Pos2, RichText, Ui};
use gscore_model::ChordDiagram;

use super::shape_editor::Outcome;
use super::style;

/// The open chord diagram dialog.
#[derive(Debug, Clone)]
pub(super) struct ChordEditor {
    pos: Option<Pos2>,
    name: String,
    base_fret: u8,
    /// One fret per string, from the lowest; -1 is x (not played).
    frets: Vec<i8>,
}

impl ChordEditor {
    /// A dialog on `current`, or on an empty six-string shape.
    pub(super) fn new(current: Option<&ChordDiagram>) -> Self {
        match current {
            Some(chord) => ChordEditor {
                pos: None,
                name: chord.name.clone(),
                base_fret: chord.base_fret,
                frets: chord.frets.iter().map(|&(_, fret)| fret).collect(),
            },
            None => ChordEditor {
                pos: None,
                name: "C".to_owned(),
                base_fret: 0,
                frets: vec![-1; 6],
            },
        }
    }

    /// The diagram as chosen.
    pub(super) fn diagram(&self) -> ChordDiagram {
        let mut diagram = ChordDiagram::new(self.name.trim());
        diagram.string_count = u8::try_from(self.frets.len()).unwrap_or(6).max(1);
        diagram.base_fret = self.base_fret;
        diagram.frets = self
            .frets
            .iter()
            .enumerate()
            .filter_map(|(string, &fret)| Some((u8::try_from(string).ok()?, fret)))
            .collect();
        if diagram.name.trim().is_empty() {
            diagram.name = "Chord".to_owned();
        }
        diagram
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context, has_current: bool) -> Outcome {
        let size = egui::vec2(320.0, 360.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new("Chord diagram")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.set_width(280.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Chord diagram").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("X").clicked() {
                            outcome = Outcome::Cancel;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.name)
                            .hint_text("C, Am, Gb7/#11")
                            .char_limit(64),
                    );
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Chord name");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Base fret");
                    let response = ui.add(egui::DragValue::new(&mut self.base_fret).range(0..=24));
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Base fret");
                    });
                    response
                        .on_hover_text("0 shows the nut; higher values show that fret at the top");
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Frets, lowest string first").weak());
                let mut strings = self.frets.len();
                ui.horizontal(|ui| {
                    ui.label("Strings");
                    let response = ui.add(egui::DragValue::new(&mut strings).range(1..=12));
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Chord strings");
                    });
                    if response.changed() {
                        self.frets.resize(strings, -1);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (string, fret) in self.frets.iter_mut().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{}", string.saturating_add(1))).weak());
                            fret_choice(ui, string, fret);
                        });
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(true, egui::Button::new("Apply"))
                        .on_hover_text("Put this chord diagram above the beat")
                        .clicked()
                    {
                        outcome = Outcome::Apply;
                    }
                    if ui
                        .add_enabled(has_current, egui::Button::new("Remove chord"))
                        .clicked()
                    {
                        outcome = Outcome::Remove;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Cancel;
                    }
                });
                ui.label(
                    RichText::new(
                        "x is a string that is not played; 0 is an open string. The diagram is \
                         drawn above the staff.",
                    )
                    .weak(),
                );
                self.keys(ui, &mut outcome);
            });
        outcome
    }

    fn keys(&self, ui: &mut Ui, outcome: &mut Outcome) {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            *outcome = Outcome::Cancel;
        }
        if ui.input(|input| {
            input.key_pressed(egui::Key::Enter) && (input.modifiers.command || input.modifiers.ctrl)
        }) {
            *outcome = Outcome::Apply;
        }
    }
}

/// One string's fret: x, open, or 1 to 24.
fn fret_choice(ui: &mut Ui, string: usize, fret: &mut i8) {
    let response = egui::ComboBox::from_id_salt(("chord_fret", string))
        .selected_text(fret_label(*fret))
        .width(48.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(fret, -1, "x");
            ui.selectable_value(fret, 0, "0");
            for value in 1..=24 {
                ui.selectable_value(fret, value, value.to_string());
            }
        })
        .response;
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("String {} fret", string.saturating_add(1)));
    });
}

fn fret_label(fret: i8) -> String {
    if fret < 0 {
        "x".to_owned()
    } else {
        fret.to_string()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_new_diagram_keeps_the_name_and_frets() {
        let mut editor = ChordEditor::new(None);
        editor.name = "  Am  ".to_owned();
        editor.base_fret = 1;
        editor.frets = vec![-1, 0, 2, 2, 2, 0];
        let diagram = editor.diagram();
        assert_eq!(diagram.name, "Am");
        assert_eq!(diagram.base_fret, 1);
        assert_eq!(
            diagram.frets,
            [(0, -1), (1, 0), (2, 2), (3, 2), (4, 2), (5, 0)]
        );
        assert_eq!(diagram.string_count, 6);
    }
}
