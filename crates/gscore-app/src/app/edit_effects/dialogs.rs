//! The small trill and tremolo dialogs: a trill's other fret and speed, and tremolo
//! picking's speed, chosen by clicking engraved signs.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::{Finger, Hand, Stroke, StrokeDirection, TremoloSpeed, Trill, TrillSpeed};

use super::super::harmonic_editor::{buttons, keys};
use super::super::shape_editor::Outcome;
use super::super::staff_widgets::{choice, stepper};
use super::super::style;
use super::super::tile_faces::{self, Face, music_glyph};
use super::super::tiles;

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

/// A fingering window, as Guitar Pro's: small, it stays open while the cursor moves
/// through the score and always shows the cursor note's finger. A click (or a key: `0`-`4`
/// and `t`, or `p i m a c`) sets the finger at once; arrows go to the next note.
#[derive(Debug, Clone)]
pub(super) struct FingerEditor {
    pos: Option<Pos2>,
    hand: Hand,
}

/// What the fingering window asks for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FingerOutcome {
    /// Still open, nothing chosen.
    Open,
    /// Set this finger on the selected notes (`None` removes it).
    Set(Option<Finger>),
    /// Closed.
    Close,
}

impl FingerEditor {
    /// A window for `hand`'s fingering.
    pub(super) fn new(hand: Hand) -> Self {
        FingerEditor { pos: None, hand }
    }

    /// The hand it sets.
    pub(super) fn hand(&self) -> Hand {
        self.hand
    }

    /// Shows the window with `current` (the cursor note's finger) chosen; `has_note` is
    /// false when the cursor is not on a note.
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        current: Option<Finger>,
        has_note: bool,
    ) -> FingerOutcome {
        let title = match self.hand {
            Hand::Left => "Left-hand fingering",
            Hand::Right => "Right-hand fingering",
        };
        let size = vec2(260.0, 120.0);
        // At the top right of the score, out of the way of the notes being fingered.
        let pos = *self.pos.get_or_insert_with(|| {
            let screen = ctx.content_rect();
            egui::pos2(screen.right() - size.x - 300.0, screen.top() + 110.0)
        });
        let mut outcome = FingerOutcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut self.pos) {
                    outcome = FingerOutcome::Close;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                            // As each hand writes them: 0 1 2 3 4 t, and p i m a c.
                            let order = match self.hand {
                                Hand::Left => Finger::ALL.to_vec(),
                                Hand::Right => vec![
                                    Finger::Thumb,
                                    Finger::Index,
                                    Finger::Middle,
                                    Finger::Ring,
                                    Finger::Little,
                                ],
                            };
                            for finger in order {
                                let sign = finger.sign(self.hand);
                                let chosen = current == Some(finger);
                                let tip = format!("{} (key {sign})", finger_name(finger));
                                let response = tiles::large_tile(
                                    ui,
                                    Face::Finger(sign),
                                    chosen,
                                    has_note,
                                    finger_name(finger),
                                    &tip,
                                );
                                if response.clicked() {
                                    // A second click on the chosen finger takes it off.
                                    outcome = FingerOutcome::Set((!chosen).then_some(finger));
                                }
                            }
                            let none = tiles::large_tile(
                                ui,
                                Face::Finger("–"),
                                false,
                                has_note && current.is_some(),
                                "No fingering",
                                "No fingering (key Backspace)",
                            );
                            if none.clicked() {
                                outcome = FingerOutcome::Set(None);
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(if has_note {
                                "Arrows go to the next note; the keys set the finger."
                            } else {
                                "Move the cursor to a note."
                            })
                            .weak()
                            .small(),
                        );
                    });
            });
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            outcome = FingerOutcome::Close;
        }
        outcome
    }
}

/// The finger a typed `text` chooses for `hand` (`Some(None)`: none), or `None` if the text
/// is no finger key.
pub(in crate::app) fn finger_key(hand: Hand, text: &str) -> Option<Option<Finger>> {
    Finger::ALL
        .into_iter()
        .filter(|finger| hand == Hand::Left || *finger != Finger::Open)
        .find(|finger| finger.sign(hand) == text)
        .map(Some)
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

/// Which spread chord the stroke dialog edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum StrokeEditorKind {
    /// A brush (strum).
    Brush,
    /// A rolled chord (arpeggio).
    Arpeggio,
}

impl StrokeEditorKind {
    /// The window title and the Remove button's text.
    fn title(self) -> (&'static str, &'static str) {
        match self {
            StrokeEditorKind::Brush => ("Brush", "No brush"),
            StrokeEditorKind::Arpeggio => ("Arpeggio", "No arpeggio"),
        }
    }
}

/// The note values a stroke can be spread over.
const STROKE_SPEEDS: [(u8, &str); 5] = [
    (4, "1/4"),
    (8, "1/8"),
    (16, "1/16"),
    (32, "1/32"),
    (64, "1/64"),
];

/// The open brush or arpeggio dialog.
#[derive(Debug, Clone)]
pub(super) struct StrokeEditor {
    pos: Option<Pos2>,
    kind: StrokeEditorKind,
    direction: StrokeDirection,
    speed: u8,
    on_beat: bool,
}

impl StrokeEditor {
    /// A dialog on `current`, or on `kind`'s defaults in `direction`.
    pub(super) fn new(
        kind: StrokeEditorKind,
        current: Option<Stroke>,
        direction: StrokeDirection,
    ) -> Self {
        let stroke = current.unwrap_or(match kind {
            StrokeEditorKind::Brush => Stroke::brush(direction),
            StrokeEditorKind::Arpeggio => Stroke::arpeggio(direction),
        });
        StrokeEditor {
            pos: None,
            kind,
            direction: stroke.direction,
            speed: stroke.speed,
            on_beat: stroke.on_beat,
        }
    }

    /// The stroke as chosen.
    pub(super) fn stroke(&self) -> Stroke {
        Stroke {
            direction: self.direction,
            speed: self.speed,
            on_beat: self.on_beat,
        }
    }

    /// The kind of spread chord this dialog edits.
    pub(super) fn kind(&self) -> StrokeEditorKind {
        self.kind
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let (title, remove) = self.kind.title();
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
                        ui.label(
                            RichText::new(
                                "How quick the strings are struck, and whether the first \
                                 string starts on the beat or the last one lands on it.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        self.directions(ui);
                        ui.add_space(8.0);
                        ui.label(RichText::new("How quick").strong());
                        ui.horizontal_wrapped(|ui| {
                            for (speed, label) in STROKE_SPEEDS {
                                let selected = self.speed == speed;
                                let response = ui.add(egui::Button::selectable(selected, label));
                                // A distinct accessible name: "1/16" is also a palette
                                // note length.
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::Button,
                                        true,
                                        selected,
                                        format!("Spread over a {label} note"),
                                    )
                                });
                                if response
                                    .on_hover_text(format!(
                                        "Spread the strings over a {label} note"
                                    ))
                                    .clicked()
                                {
                                    self.speed = speed;
                                }
                            }
                        });
                        ui.add_space(8.0);
                        self.timing(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, remove, None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// Down or up, with the engraved sign.
    fn directions(&mut self, ui: &mut egui::Ui) {
        for (direction, name, detail) in [
            (
                StrokeDirection::Down,
                "Down",
                "From the lowest string up to the highest.",
            ),
            (
                StrokeDirection::Up,
                "Up",
                "From the highest string down to the lowest.",
            ),
        ] {
            let up = direction == StrokeDirection::Up;
            if choice(
                ui,
                self.direction == direction,
                name,
                detail,
                move |painter, rect, ink| {
                    let tile = egui::Rect::from_center_size(rect.center(), vec2(27.0, 32.0));
                    tile_faces::paint_face(painter, tile, Face::Arpeggio { up }, ink);
                },
            )
            .clicked()
            {
                self.direction = direction;
            }
        }
    }

    /// Start on the beat, or fall so the last string lands on it.
    fn timing(&mut self, ui: &mut egui::Ui) {
        if choice(
            ui,
            self.on_beat,
            "Starts on the beat",
            "The first string is struck on the beat; the rest follow.",
            |_, _, _| {},
        )
        .clicked()
        {
            self.on_beat = true;
        }
        if choice(
            ui,
            !self.on_beat,
            "Ends on the beat",
            "The spread falls before the beat, so its last string lands on it.",
            |_, _, _| {},
        )
        .clicked()
        {
            self.on_beat = false;
        }
    }
}
