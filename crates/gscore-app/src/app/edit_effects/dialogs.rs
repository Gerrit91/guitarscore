//! The small trill and tremolo dialogs: a trill's other fret and speed, and tremolo
//! picking's speed, chosen by clicking engraved signs.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::{Finger, Hand, TremoloSpeed, Trill, TrillSpeed};

use super::super::harmonic_editor::{buttons, keys};
use super::super::shape_editor::Outcome;
use super::super::staff_widgets::{choice, stepper};
use super::super::style;
use super::super::tile_faces::{lettering, music_glyph};

/// Width of a dialog's content.
const WIDTH: f32 = 460.0;
/// Highest trill fret offered.
const MAX_FRET: u8 = 24;

/// The three trill speeds with their note signs.
const TRILL_SPEEDS: [(TrillSpeed, &str, &str); 3] = [
    (TrillSpeed::Sixteenth, "\u{E1D9}", "16th notes"),
    (TrillSpeed::ThirtySecond, "\u{E1DB}", "32nd notes"),
    (TrillSpeed::SixtyFourth, "\u{E1DD}", "64th notes"),
];

/// The three tremolo speeds with their stroke glyphs (one, two or three strokes).
const TREMOLO_SPEEDS: [(TremoloSpeed, &str, &str); 3] = [
    (
        TremoloSpeed::Eighth,
        "\u{E220}",
        "Eighth notes (one stroke)",
    ),
    (
        TremoloSpeed::Sixteenth,
        "\u{E221}",
        "16th notes (two strokes)",
    ),
    (
        TremoloSpeed::ThirtySecond,
        "\u{E222}",
        "32nd notes (three strokes)",
    ),
];

/// The open trill dialog.
#[derive(Debug, Clone)]
pub(super) struct TrillEditor {
    pos: Option<Pos2>,
    /// The other fret; ignored when `two_above` is chosen.
    fret: u8,
    speed: TrillSpeed,
    /// Use the default two frets above the note.
    two_above: bool,
}

impl TrillEditor {
    /// A dialog on `current`, or on two frets above `main_fret`.
    pub(super) fn new(current: Option<Trill>, main_fret: u8) -> Self {
        let trill = current.unwrap_or(Trill {
            fret: None,
            speed: TrillSpeed::Sixteenth,
        });
        TrillEditor {
            pos: None,
            fret: trill
                .fret
                .unwrap_or_else(|| main_fret.saturating_add(2).min(MAX_FRET)),
            speed: trill.speed,
            two_above: trill.fret.is_none(),
        }
    }

    /// The trill as chosen.
    pub(super) fn trill(&self) -> Trill {
        Trill {
            fret: (!self.two_above).then_some(self.fret),
            speed: self.speed,
        }
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Trill";
        let size = vec2(WIDTH + 32.0, 420.0);
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
                        self.sign(ui);
                        ui.add_space(6.0);
                        self.fret(ui);
                        ui.add_space(8.0);
                        ui.label(RichText::new("Speed").strong());
                        ui.add_space(2.0);
                        self.speeds(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, "No trill", None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// The engraved trill sign, large.
    fn sign(&self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 46.0), egui::Sense::hover());
        music_glyph(ui.painter(), rect.center(), "\u{E566}", 40.0, tokens.text);
    }

    /// The default (two frets above) or a chosen fret.
    fn fret(&mut self, ui: &mut egui::Ui) {
        let two = self.two_above;
        if choice(
            ui,
            two,
            "Two frets above",
            "The usual trill: a whole step up from the note.",
            |painter, rect, ink| {
                music_glyph(painter, rect.center(), "\u{E566}", 26.0, ink);
            },
        )
        .clicked()
        {
            self.two_above = true;
        }
        let (rect, change) = stepper(ui, "Trill fret", 34.0);
        let tokens = style::tokens(ui);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            self.fret.to_string(),
            egui::FontId::proportional(20.0),
            tokens.text,
        );
        if change != 0 {
            let next = i16::from(self.fret).saturating_add(i16::try_from(change).unwrap_or(0));
            self.fret = u8::try_from(next).unwrap_or(self.fret).min(MAX_FRET);
            self.two_above = false;
        }
    }

    /// The three speeds as engraved note signs.
    fn speeds(&mut self, ui: &mut egui::Ui) {
        for &(speed, sign, detail) in &TRILL_SPEEDS {
            if choice(
                ui,
                self.speed == speed,
                detail,
                "How fast the note alternates with the trilled fret.",
                |painter, rect, ink| music_glyph(painter, rect.center(), sign, 24.0, ink),
            )
            .clicked()
            {
                self.speed = speed;
            }
        }
    }
}

/// The open fingering dialog.
#[derive(Debug, Clone)]
pub(super) struct FingerEditor {
    pos: Option<Pos2>,
    hand: Hand,
    finger: Option<Finger>,
}

impl FingerEditor {
    /// A dialog on `current`, or on the index finger.
    pub(super) fn new(hand: Hand, current: Option<Finger>) -> Self {
        FingerEditor {
            pos: None,
            hand,
            finger: current.or(Some(Finger::Index)),
        }
    }

    /// The finger as chosen; `None` removes the fingering.
    pub(super) fn finger(&self) -> Option<Finger> {
        self.finger
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = match self.hand {
            Hand::Left => "Left-hand fingering",
            Hand::Right => "Right-hand fingering",
        };
        let size = vec2(WIDTH + 32.0, 360.0);
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
                        ui.label(
                            RichText::new(match self.hand {
                                Hand::Left => {
                                    "The fretting hand: 0 is the open string, t the thumb."
                                }
                                Hand::Right => {
                                    "The picking hand: p i m a c, as classical guitar writes them."
                                }
                            })
                            .weak(),
                        );
                        ui.add_space(4.0);
                        for finger in Finger::ALL {
                            let sign = finger.sign(self.hand);
                            let name = finger_name(finger);
                            if choice(
                                ui,
                                self.finger == Some(finger),
                                name,
                                "Which finger plays this note.",
                                |painter, rect, ink| {
                                    lettering(painter, rect.center(), sign, 16.0, false, ink);
                                },
                            )
                            .clicked()
                            {
                                self.finger = Some(finger);
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, "No fingering", None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }
}

/// A finger's name for the dialog.
fn finger_name(finger: Finger) -> &'static str {
    match finger {
        Finger::Open => "Open string",
        Finger::Thumb => "Thumb",
        Finger::Index => "Index",
        Finger::Middle => "Middle",
        Finger::Ring => "Ring",
        Finger::Little => "Little",
    }
}

/// The open tremolo dialog.
#[derive(Debug, Clone)]
pub(super) struct TremoloEditor {
    pos: Option<Pos2>,
    speed: TremoloSpeed,
}

impl TremoloEditor {
    /// A dialog on `current`, or on 16ths.
    pub(super) fn new(current: Option<TremoloSpeed>) -> Self {
        TremoloEditor {
            pos: None,
            speed: current.unwrap_or(TremoloSpeed::Sixteenth),
        }
    }

    /// The speed as chosen.
    pub(super) fn speed(&self) -> TremoloSpeed {
        self.speed
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Tremolo picking";
        let size = vec2(WIDTH + 32.0, 400.0);
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
                        self.sign(ui);
                        ui.add_space(6.0);
                        for &(speed, sign, detail) in &TREMOLO_SPEEDS {
                            if choice(
                                ui,
                                self.speed == speed,
                                detail,
                                "How fast the note is repeated.",
                                |painter, rect, ink| {
                                    music_glyph(painter, rect.center(), sign, 26.0, ink)
                                },
                            )
                            .clicked()
                            {
                                self.speed = speed;
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, "No tremolo picking", None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// The chosen tremolo strokes, large.
    fn sign(&self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 46.0), egui::Sense::hover());
        let sign = match self.speed {
            TremoloSpeed::Eighth => "\u{E220}",
            TremoloSpeed::Sixteenth => "\u{E221}",
            TremoloSpeed::ThirtySecond => "\u{E222}",
        };
        music_glyph(ui.painter(), rect.center(), sign, 40.0, tokens.text);
    }
}
