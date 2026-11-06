//! The chord diagram dialog: the diagram as the score draws it, edited by clicking it (a
//! click on the grid puts a finger there, a click above a string makes it open or not
//! played), with the chord's name and the fret the diagram starts at. It has as many
//! strings as the track's instrument.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};
use gscore_model::ChordDiagram;

use super::shape_editor::Outcome;
use super::style;

/// Frets the diagram shows.
const FRETS: u8 = 5;
/// Distance between two strings and two frets on screen.
const STRING_GAP: f32 = 30.0;
const FRET_GAP: f32 = 34.0;
/// The row above the grid for the open and not-played marks.
const MARKER_ROW: f32 = 26.0;
/// Room left of the grid for the first fret's number.
const SIDE: f32 = 34.0;
/// The most strings a diagram has.
const MAX_STRINGS: usize = 12;

/// The open chord diagram dialog.
#[derive(Debug, Clone)]
pub(super) struct ChordEditor {
    pos: Option<Pos2>,
    name: String,
    /// The fret at the top of the grid (0 and 1: the nut).
    base_fret: u8,
    /// One fret per string, from the lowest; -1 is x (not played), 0 an open string.
    frets: Vec<i8>,
}

/// Where a click on the diagram lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot {
    /// The mark above a string.
    Marker(usize),
    /// A fret on a string.
    Fret(usize, i8),
}

impl ChordEditor {
    /// A dialog on `current`, or on the cursor beat's notes (`beat_shape`, lowest string
    /// first, and the chord they make, `beat_chord`) when there is no diagram yet;
    /// `strings` is the track's string count.
    pub(super) fn new(
        current: Option<&ChordDiagram>,
        strings: usize,
        beat_shape: &[i8],
        beat_chord: Option<&str>,
    ) -> Self {
        let strings = strings.clamp(1, MAX_STRINGS);
        match current {
            Some(chord) => {
                let mut frets = vec![-1; strings];
                for &(string, fret) in &chord.frets {
                    if let Some(slot) = frets.get_mut(usize::from(string)) {
                        *slot = fret;
                    }
                }
                ChordEditor {
                    pos: None,
                    name: chord.name.clone(),
                    base_fret: chord.base_fret,
                    frets,
                }
            }
            None => {
                let mut frets: Vec<i8> = beat_shape.iter().copied().take(strings).collect();
                frets.resize(strings, -1);
                // A shape high on the neck starts the grid at its lowest fret.
                let lowest = frets.iter().copied().filter(|&fret| fret > 0).min();
                let highest = frets.iter().copied().max().unwrap_or(0);
                let base_fret = match lowest {
                    Some(lowest) if highest > i8::try_from(FRETS).unwrap_or(5) => {
                        u8::try_from(lowest).unwrap_or(0)
                    }
                    _ => 0,
                };
                ChordEditor {
                    pos: None,
                    name: beat_chord.unwrap_or_default().to_owned(),
                    base_fret,
                    frets,
                }
            }
        }
    }

    /// The diagram as chosen.
    pub(super) fn diagram(&self) -> ChordDiagram {
        let mut diagram = ChordDiagram::new(self.name.trim());
        diagram.string_count = u8::try_from(self.frets.len()).unwrap_or(6).max(1);
        diagram.fret_count = FRETS;
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

    /// The first fret the grid shows.
    fn first_fret(&self) -> i8 {
        i8::try_from(self.base_fret.max(1)).unwrap_or(1)
    }

    /// What a click on `spot` does: a finger on a fret (a second click takes it off, leaving
    /// the string open); above a string, open and not played take turns.
    fn click(&mut self, spot: Spot) {
        match spot {
            Spot::Marker(string) => {
                if let Some(fret) = self.frets.get_mut(string) {
                    *fret = if *fret == 0 { -1 } else { 0 };
                }
            }
            Spot::Fret(string, value) => {
                if let Some(fret) = self.frets.get_mut(string) {
                    *fret = if *fret == value { 0 } else { value };
                }
            }
        }
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context, has_current: bool) -> Outcome {
        let size = vec2(360.0, 420.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new("Chord diagram")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "Chord diagram", &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        self.body(ui, has_current, &mut outcome);
                    });
            });
        outcome
    }

    fn body(&mut self, ui: &mut Ui, has_current: bool, outcome: &mut Outcome) {
        ui.horizontal(|ui| {
            ui.label("Name");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text("C, Am, Gb7/#11")
                    .desired_width(150.0)
                    .char_limit(64),
            );
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label("Chord name");
            });
            ui.label("First fret");
            let mut first = self.base_fret.max(1);
            let response = ui.add(egui::DragValue::new(&mut first).range(1..=24));
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label("First fret");
            });
            if response.changed() {
                self.base_fret = if first <= 1 { 0 } else { first };
            }
            response.on_hover_text("The fret at the top of the diagram; 1 shows the nut");
        });
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            if let Some(spot) = self.diagram_ui(ui) {
                self.click(spot);
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Click the grid to put a finger on a fret (again to take it off); click \
                 above a string to make it open (o) or not played (x).",
            )
            .weak(),
        );
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button("Apply")
                .on_hover_text("Put this chord diagram above the beat")
                .clicked()
            {
                *outcome = Outcome::Apply;
            }
            if ui
                .add_enabled(has_current, egui::Button::new("Remove chord"))
                .clicked()
            {
                *outcome = Outcome::Remove;
            }
            if ui.button("Cancel").clicked() {
                *outcome = Outcome::Cancel;
            }
        });
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            *outcome = Outcome::Cancel;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command) {
            *outcome = Outcome::Apply;
        }
    }

    /// The diagram, drawn as the score draws it; returns where it was clicked.
    fn diagram_ui(&self, ui: &mut Ui) -> Option<Spot> {
        let strings = self.frets.len().max(1);
        let width = strings.saturating_sub(1) as f32 * STRING_GAP;
        let size = vec2(
            width + 2.0 * SIDE,
            MARKER_ROW + f32::from(FRETS) * FRET_GAP + 8.0,
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label("Chord diagram grid");
        });
        let tokens = style::tokens(ui);
        let ink = tokens.text;
        let painter = ui.painter();
        let left = rect.left() + SIDE;
        let grid_top = rect.top() + MARKER_ROW;
        let grid_bottom = grid_top + f32::from(FRETS) * FRET_GAP;
        let string_x = |string: usize| left + string as f32 * STRING_GAP;
        let thin = Stroke::new(1.0, ink);
        for string in 0..strings {
            let x = string_x(string);
            painter.line_segment([egui::pos2(x, grid_top), egui::pos2(x, grid_bottom)], thin);
        }
        let at_nut = self.base_fret <= 1;
        for fret in 0..=FRETS {
            let y = grid_top + f32::from(fret) * FRET_GAP;
            let stroke = if fret == 0 && at_nut {
                Stroke::new(4.0, ink)
            } else {
                thin
            };
            painter.line_segment([egui::pos2(left, y), egui::pos2(left + width, y)], stroke);
        }
        if !at_nut {
            painter.text(
                egui::pos2(left - 10.0, grid_top + FRET_GAP / 2.0),
                egui::Align2::RIGHT_CENTER,
                format!("{}fr", self.base_fret),
                egui::FontId::proportional(13.0),
                ink,
            );
        }
        let first = self.first_fret();
        let marker_y = rect.top() + MARKER_ROW / 2.0 - 2.0;
        for (string, &fret) in self.frets.iter().enumerate() {
            let x = string_x(string);
            if fret < 0 {
                let half = 5.0;
                let stroke = Stroke::new(1.6, ink);
                painter.line_segment(
                    [
                        egui::pos2(x - half, marker_y - half),
                        egui::pos2(x + half, marker_y + half),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(x - half, marker_y + half),
                        egui::pos2(x + half, marker_y - half),
                    ],
                    stroke,
                );
            } else if fret == 0 {
                painter.circle_stroke(egui::pos2(x, marker_y), 5.5, Stroke::new(1.6, ink));
            } else {
                let row = fret.saturating_sub(first);
                if (0..i8::try_from(FRETS).unwrap_or(5)).contains(&row) {
                    let y = grid_top + (f32::from(row) + 0.5) * FRET_GAP;
                    painter.circle_filled(egui::pos2(x, y), 9.0, ink);
                }
            }
        }
        // Where a click would go: a faint finger, or a highlighted mark.
        let spot_at = |pos: Pos2| -> Option<Spot> {
            let string = ((pos.x - left) / STRING_GAP).round();
            if string < 0.0 || string >= strings as f32 {
                return None;
            }
            let string = string as usize;
            if (pos.x - string_x(string)).abs() > STRING_GAP / 2.0 {
                return None;
            }
            if pos.y < grid_top {
                return Some(Spot::Marker(string));
            }
            let row = ((pos.y - grid_top) / FRET_GAP).floor();
            if row < 0.0 || row >= f32::from(FRETS) {
                return None;
            }
            Some(Spot::Fret(
                string,
                first.saturating_add(i8::try_from(row as i32).unwrap_or(0)),
            ))
        };
        if let Some(pos) = response.hover_pos()
            && let Some(spot) = spot_at(pos)
        {
            let faint = Color32::from_white_alpha(0).lerp_to_gamma(ink, 0.3);
            match spot {
                Spot::Fret(string, fret) if self.frets.get(string) != Some(&fret) => {
                    let row = fret.saturating_sub(first);
                    let y = grid_top + (f32::from(row) + 0.5) * FRET_GAP;
                    painter.circle_filled(egui::pos2(string_x(string), y), 9.0, faint);
                }
                Spot::Marker(string) => {
                    let center = egui::pos2(string_x(string), marker_y);
                    painter.rect_filled(
                        Rect::from_center_size(center, vec2(18.0, 18.0)),
                        4.0,
                        tokens.control_hover,
                    );
                }
                Spot::Fret(..) => {}
            }
        }
        response
            .clicked()
            .then(|| response.interact_pointer_pos())
            .flatten()
            .and_then(spot_at)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_new_diagram_keeps_the_name_and_frets() {
        let mut editor = ChordEditor::new(None, 6, &[], None);
        editor.name = "  Am  ".to_owned();
        editor.frets = vec![-1, 0, 2, 2, 1, 0];
        let diagram = editor.diagram();
        assert_eq!(diagram.name, "Am");
        assert_eq!(
            diagram.frets,
            [(0, -1), (1, 0), (2, 2), (3, 2), (4, 1), (5, 0)]
        );
        assert_eq!(diagram.string_count, 6);
    }

    #[test]
    fn the_diagram_has_the_tracks_strings_and_starts_from_the_beat() {
        // A seven-string track whose beat holds a shape high on the neck.
        let editor = ChordEditor::new(None, 7, &[-1, 8, 10, 10, 9, 8, -1], Some("Fm"));
        assert_eq!(editor.frets.len(), 7);
        assert_eq!(editor.frets, [-1, 8, 10, 10, 9, 8, -1]);
        assert_eq!(editor.base_fret, 8, "the grid starts at the shape");
        assert_eq!(editor.name, "Fm", "named from the beat's notes");
        // An existing six-string diagram on a seven-string track gets the seventh string.
        let six = ChordEditor::new(None, 6, &[0, 2, 2, 1, 0, 0], None).diagram();
        let seven = ChordEditor::new(Some(&six), 7, &[], None);
        assert_eq!(seven.frets, [0, 2, 2, 1, 0, 0, -1]);
        assert_eq!(seven.base_fret, 0);
    }

    #[test]
    fn clicks_put_fingers_on_and_take_them_off() {
        let mut editor = ChordEditor::new(None, 6, &[], None);
        assert_eq!(editor.frets, [-1; 6]);
        editor.click(Spot::Fret(1, 3));
        assert_eq!(editor.frets[1], 3);
        editor.click(Spot::Fret(1, 2));
        assert_eq!(editor.frets[1], 2, "another fret moves the finger");
        editor.click(Spot::Fret(1, 2));
        assert_eq!(
            editor.frets[1], 0,
            "the same fret takes it off: open string"
        );
        editor.click(Spot::Marker(1));
        assert_eq!(editor.frets[1], -1, "open, then not played");
        editor.click(Spot::Marker(1));
        assert_eq!(editor.frets[1], 0);
    }
}
