//! The tuplet dialog: any tuplet, "n notes in the time of m", chosen with two steppers
//! under a large engraved bracket, or by clicking one of the common tuplets.

use egui::{Margin, Pos2, Rect, RichText, Sense, vec2};

use super::harmonic_editor::{buttons, keys};
use super::shape_editor::Outcome;
use super::staff_widgets::{choice, stepper};
use super::style;
use super::tile_faces::tuplet_bracket;

/// Width of the dialog's content.
const WIDTH: f32 = 520.0;
/// The most notes (and the longest "in the time of") a tuplet can have here.
const MAX: u8 = 32;

/// Common tuplets: notes, in the time of, name, what it is for.
const COMMON: [(u8, u8, &str, &str); 8] = [
    (3, 2, "Triplet", "Three in the time of two."),
    (6, 4, "Sextuplet", "Six in the time of four."),
    (5, 4, "Quintuplet", "Five in the time of four."),
    (7, 4, "Septuplet", "Seven in the time of four."),
    (9, 8, "Nonuplet", "Nine in the time of eight."),
    (10, 8, "Decuplet", "Ten in the time of eight."),
    (2, 3, "Duplet", "Two in the time of three (6/8)."),
    (4, 3, "Quadruplet", "Four in the time of three (6/8)."),
];

/// The open tuplet dialog.
#[derive(Debug, Clone)]
pub(super) struct TupletDialog {
    pos: Option<Pos2>,
    enters: u8,
    times: u8,
}

impl TupletDialog {
    /// A dialog on `current` (or a triplet).
    pub(super) fn new(current: Option<(u8, u8)>) -> Self {
        let (enters, times) = current.unwrap_or((3, 2));
        TupletDialog {
            pos: None,
            enters: enters.clamp(1, MAX),
            times: times.clamp(1, MAX),
        }
    }

    /// The tuplet as chosen; `None` when both numbers are equal (no tuplet at all).
    pub(super) fn tuplet(&self) -> Option<(u8, u8)> {
        (self.enters != self.times).then_some((self.enters, self.times))
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Tuplet";
        let size = vec2(WIDTH + 32.0, 560.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        self.numbers(ui);
                        ui.add_space(10.0);
                        ui.label(RichText::new("Common tuplets").strong());
                        ui.add_space(2.0);
                        self.common(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, "Notes without a tuplet", None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// The large bracket with the chosen numbers, the two steppers and what it means.
    fn numbers(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let (preview, _) = ui.allocate_exact_size(vec2(WIDTH, 70.0), Sense::hover());
        let bracket = Rect::from_center_size(preview.center(), vec2(180.0, 70.0));
        let text = format!("{}:{}", self.enters, self.times);
        tuplet_bracket(ui.painter(), bracket, &text, false, tokens.text, false);
        ui.horizontal(|ui| {
            ui.add_space((WIDTH - 2.0 * 150.0 - 70.0) / 2.0);
            self.enters = step(ui, "Notes", self.enters, 2);
            ui.add_space(8.0);
            ui.label(RichText::new("in the time of").weak());
            ui.add_space(8.0);
            self.times = step(ui, "In the time of", self.times, 1);
        });
        let meaning = if self.enters == self.times {
            "Equal numbers make no tuplet: OK takes the tuplet off.".to_owned()
        } else {
            format!(
                "{} notes take the time of {} of the same value.",
                self.enters, self.times
            )
        };
        ui.vertical_centered(|ui| ui.label(RichText::new(meaning).weak()));
    }

    /// The common tuplets in two columns.
    fn common(&mut self, ui: &mut egui::Ui) {
        let half = COMMON.len().div_ceil(2);
        ui.columns(2, |columns| {
            for (index, &(enters, times, name, detail)) in COMMON.iter().enumerate() {
                let Some(column) = columns.get_mut(usize::from(index >= half)) else {
                    continue;
                };
                let chosen = self.enters == enters && self.times == times;
                // The triplet and sextuplet as their palette tiles show them.
                let text = if matches!((enters, times), (3, 2) | (6, 4)) {
                    enters.to_string()
                } else {
                    format!("{enters}:{times}")
                };
                if choice(column, chosen, name, detail, |painter, rect, ink| {
                    tuplet_bracket(
                        painter,
                        rect.shrink2(vec2(6.0, 4.0)),
                        &text,
                        false,
                        ink,
                        false,
                    );
                })
                .clicked()
                {
                    self.enters = enters;
                    self.times = times;
                }
            }
        });
    }
}

/// A stepper for one number of the tuplet, at least `min`; returns the new value.
fn step(ui: &mut egui::Ui, label: &str, value: u8, min: u8) -> u8 {
    let (rect, change) = stepper(ui, label, 40.0);
    let tokens = style::tokens(ui);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        value.to_string(),
        egui::FontId::proportional(22.0),
        tokens.text,
    );
    let next = i16::from(value).saturating_add(i16::try_from(change).unwrap_or(0));
    u8::try_from(next).unwrap_or(value).clamp(min, MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_starts_on_the_current_tuplet_or_a_triplet() {
        assert_eq!(TupletDialog::new(None).tuplet(), Some((3, 2)));
        assert_eq!(TupletDialog::new(Some((7, 4))).tuplet(), Some((7, 4)));
        assert_eq!(TupletDialog::new(Some((90, 4))).tuplet(), Some((MAX, 4)));
    }

    #[test]
    fn equal_numbers_mean_no_tuplet() {
        let mut dialog = TupletDialog::new(None);
        dialog.enters = 4;
        dialog.times = 4;
        assert_eq!(dialog.tuplet(), None);
    }
}
