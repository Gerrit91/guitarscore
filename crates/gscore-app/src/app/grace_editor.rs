//! The grace note dialog: the grace note's fret, its speed (a 64th, 32nd or 16th) and
//! whether it comes before the beat or on it, each chosen by clicking its engraved sign.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::{Fret, Grace, GraceSpeed};

use super::harmonic_editor::{buttons, keys};
use super::shape_editor::Outcome;
use super::staff_widgets::{choice, glyph_in};
use super::style;

/// Width of the dialog's content.
const WIDTH: f32 = 440.0;
/// Highest fret offered.
const MAX_FRET: u8 = 36;

/// A speed's sign (a 64th, 32nd or 16th note), name and what it does.
fn speed_choice(speed: GraceSpeed) -> (&'static str, &'static str, &'static str) {
    match speed {
        GraceSpeed::Fast => (
            "\u{E1DD}",
            "Fast",
            "A 64th: a quick flick into the main note.",
        ),
        GraceSpeed::Normal => ("\u{E1DB}", "Normal", "A 32nd: the usual grace note."),
        GraceSpeed::Slow => (
            "\u{E1D9}",
            "Slow",
            "A 16th: clearly heard before the main note.",
        ),
    }
}

/// The timings: sign, name, what it does, and whether it is on the beat.
const TIMINGS: [(&str, &str, &str, bool); 2] = [
    (
        "\u{E560}",
        "Before the beat",
        "Acciaccatura: the main note stays on time.",
        false,
    ),
    (
        "\u{E562}",
        "On the beat",
        "Appoggiatura: it takes the beat, the main note follows.",
        true,
    ),
];

/// The open grace note dialog.
#[derive(Debug, Clone)]
pub(super) struct GraceEditor {
    pos: Option<Pos2>,
    fret: u8,
    speed: GraceSpeed,
    on_beat: bool,
    dead: bool,
    /// The main note's fret, for the quick choices around it.
    main_fret: u8,
}

impl GraceEditor {
    /// A dialog for a note on `main_fret` with `current` (or a grace note two frets below,
    /// the usual hammer-on into the note; one above an open string).
    pub(super) fn new(current: Option<Grace>, main_fret: u8) -> Self {
        let grace = current.unwrap_or_else(|| {
            let fret = if main_fret >= 2 {
                main_fret.saturating_sub(2)
            } else {
                main_fret.saturating_add(2)
            };
            Grace::before(Fret::new(fret).unwrap_or(Fret::OPEN))
        });
        GraceEditor {
            pos: None,
            fret: grace.fret.get(),
            speed: grace.speed,
            on_beat: grace.on_beat,
            dead: grace.dead,
            main_fret,
        }
    }

    /// The grace note as chosen.
    pub(super) fn grace(&self) -> Grace {
        Grace {
            fret: Fret::new(self.fret.min(MAX_FRET)).unwrap_or(Fret::OPEN),
            speed: self.speed,
            on_beat: self.on_beat,
            dead: self.dead,
        }
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Grace note";
        let size = vec2(WIDTH + 32.0, 520.0);
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
                        self.fret_row(ui);
                        ui.add_space(10.0);
                        heading(ui, "Speed");
                        self.speeds(ui);
                        ui.add_space(10.0);
                        heading(ui, "Timing");
                        self.timings(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(
                            ui,
                            &mut outcome,
                            "Play the note without a grace note",
                            "Play the note with this grace note",
                        );
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// The fret, with quick choices around the main note, and the dead-note switch.
    fn fret_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Fret").strong());
            let response = ui.add_enabled(
                !self.dead,
                egui::DragValue::new(&mut self.fret).range(0..=MAX_FRET),
            );
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label("Fret of the grace note");
            });
            ui.label(RichText::new(format!("main note on fret {}", self.main_fret)).weak());
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for offset in [-2_i16, -1, 1, 2] {
                let target = i16::from(self.main_fret).saturating_add(offset);
                let Ok(fret) = u8::try_from(target) else {
                    continue;
                };
                if fret > MAX_FRET {
                    continue;
                }
                let text = if offset < 0 {
                    format!("{} below", offset.unsigned_abs())
                } else {
                    format!("{offset} above")
                };
                let tip = if offset < 0 {
                    "Hammer on from below into the main note"
                } else {
                    "Pull off from above into the main note"
                };
                let on = !self.dead && self.fret == fret;
                if ui
                    .add_enabled(!self.dead, egui::Button::new(text).selected(on))
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.fret = fret;
                }
            }
        });
        ui.checkbox(&mut self.dead, "Dead note: a muted, percussive click");
    }

    fn speeds(&mut self, ui: &mut egui::Ui) {
        for speed in GraceSpeed::ALL {
            let (sign, name, detail) = speed_choice(speed);
            if choice(
                ui,
                self.speed == speed,
                name,
                detail,
                |painter, rect, ink| {
                    glyph_in(painter, rect, sign, 34.0, 8.0, ink);
                },
            )
            .clicked()
            {
                self.speed = speed;
            }
        }
    }

    fn timings(&mut self, ui: &mut egui::Ui) {
        for (sign, name, detail, on_beat) in TIMINGS {
            // The grace note and the main note it leads into.
            let sign = format!("{sign}\u{2009}\u{E0A4}");
            if choice(
                ui,
                self.on_beat == on_beat,
                name,
                detail,
                |painter, rect, ink| {
                    glyph_in(painter, rect, &sign, 30.0, 6.0, ink);
                },
            )
            .clicked()
            {
                self.on_beat = on_beat;
            }
        }
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong());
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_grace_note_starts_two_frets_below() {
        let editor = GraceEditor::new(None, 7);
        let grace = editor.grace();
        assert_eq!(grace.fret.get(), 5);
        assert_eq!(grace.speed, GraceSpeed::Normal);
        assert!(!grace.on_beat && !grace.dead);
        // Below fret 2 there is no room: two above instead.
        assert_eq!(GraceEditor::new(None, 1).grace().fret.get(), 3);
    }

    #[test]
    fn an_existing_grace_note_is_edited_as_it_is() {
        let current = Grace {
            fret: Fret::new(9).unwrap(),
            speed: GraceSpeed::Slow,
            on_beat: true,
            dead: true,
        };
        assert_eq!(GraceEditor::new(Some(current), 7).grace(), current);
    }
}
