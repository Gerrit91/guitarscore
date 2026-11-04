//! The harmonic editor: which harmonic a note plays (natural, artificial, tapped, pinch,
//! semi), where the string is touched for the ones that need a touch point, and a picture
//! of the string's partials (like the classic diagram of harmonics on a string): the
//! vibrating length with one, two, three ... loops, the chosen partial highlighted and the
//! touch point marked on the frets.

use egui::{Align, Layout, Margin, Pos2, RichText, vec2};
use gscore_model::{Harmonic, HarmonicKind};

use super::harmonic_diagram;
use super::shape_editor::{Outcome, listen_button};
use super::style;
use super::tiles;

/// Every kind, in the order offered, with its label and what it is.
const KINDS: [(HarmonicKind, &str, &str); 5] = [
    (
        HarmonicKind::Natural,
        "Natural",
        "The open string touched lightly over the written fret (12, 7, 5, ...).",
    ),
    (
        HarmonicKind::Artificial,
        "Artificial",
        "The note is fretted and the string touched a few frets higher.",
    ),
    (
        HarmonicKind::Tapped,
        "Tapped",
        "The note is fretted and the string tapped a few frets higher.",
    ),
    (
        HarmonicKind::Pinch,
        "Pinch",
        "The picking thumb grazes the string at a node: the squeal.",
    ),
    (
        HarmonicKind::Semi,
        "Semi",
        "The note with a little of a harmonic mixed in (played as the note).",
    ),
];

/// Touch points offered as quick choices: frets above the fretted note.
const TOUCH_POINTS: [u8; 6] = [12, 7, 5, 19, 24, 4];
/// Highest partial the picture and the pitch consider.
const MAX_PARTIAL: u32 = 8;
/// How close (as a share of the string) a fret must be to a node (as the model decides).
const NODE_TOLERANCE: f64 = 0.05;
/// Size of the picture.
pub(super) const DIAGRAM: egui::Vec2 = egui::Vec2::new(560.0, 250.0);

/// The partial touched `frets` frets above where the string vibrates from, if a node of
/// one lies there (fret 12: the 2nd, 7 and 19: the 3rd, 5 and 24: the 4th, ...).
pub(super) fn partial(frets: u8) -> Option<u32> {
    let position = 1.0 - 2_f64.powf(-f64::from(frets) / 12.0);
    (2..=MAX_PARTIAL).find(|&n| {
        let multiple = position * f64::from(n);
        let node = multiple.round();
        node >= 1.0 && node < f64::from(n) && (multiple - node).abs() < NODE_TOLERANCE
    })
}

/// The interval the `n`th partial sounds above the vibrating string.
fn interval_name(n: u32) -> &'static str {
    match n {
        1 => "the note itself",
        2 => "an octave up",
        3 => "an octave and a fifth up",
        4 => "two octaves up",
        5 => "two octaves and a major third up",
        6 => "two octaves and a fifth up",
        7 => "two octaves and a minor seventh up",
        _ => "three octaves up",
    }
}

/// Whether `kind` is touched somewhere other than the written fret.
fn has_touch_point(kind: HarmonicKind) -> bool {
    matches!(
        kind,
        HarmonicKind::Artificial | HarmonicKind::Tapped | HarmonicKind::Pinch
    )
}

/// The open harmonic editor.
#[derive(Debug, Clone)]
pub(super) struct HarmonicEditor {
    pos: Option<Pos2>,
    kind: HarmonicKind,
    /// For a touched harmonic: how many frets above the fretted note.
    distance: u8,
    /// The cursor note's fret.
    fret: u8,
    /// What the draft sounds like ("E5"), worked out by the caller each frame.
    pub(super) sounds: Option<String>,
}

impl HarmonicEditor {
    /// An editor for a note on `fret` with `current` (or a natural harmonic).
    pub(super) fn new(current: Option<Harmonic>, fret: u8) -> Self {
        let kind = current.map_or(HarmonicKind::Natural, |h| h.kind);
        let distance = current
            .and_then(|h| h.node)
            .and_then(|node| node.checked_sub(fret))
            .filter(|&d| d > 0)
            .unwrap_or(12);
        HarmonicEditor {
            pos: None,
            kind,
            distance,
            fret,
            sounds: None,
        }
    }

    /// The harmonic for a note on `fret`: touched harmonics store their node's fret.
    pub(super) fn harmonic(&self, fret: u8) -> Harmonic {
        Harmonic {
            kind: self.kind,
            node: has_touch_point(self.kind).then(|| fret.saturating_add(self.distance)),
        }
    }

    /// The partial the draft sounds, if any: for a natural harmonic over the written fret,
    /// for a touched one at its distance above the fretted note.
    fn chosen_partial(&self) -> Option<u32> {
        match self.kind {
            HarmonicKind::Natural => partial(self.fret),
            HarmonicKind::Semi => None,
            _ => partial(self.distance),
        }
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Harmonic";
        let size = vec2(DIAGRAM.x + 32.0, 520.0);
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
                        ui.set_width(DIAGRAM.x);
                        self.fields(ui);
                        ui.add_space(8.0);
                        self.diagram(ui);
                        ui.add_space(6.0);
                        self.result(ui);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(
                            ui,
                            &mut outcome,
                            "Play the note without a harmonic",
                            "Play the note with this harmonic",
                        );
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    /// The kind and, for touched harmonics, the touch point.
    fn fields(&mut self, ui: &mut egui::Ui) {
        let items: Vec<(&str, &str)> = KINDS.iter().map(|(_, label, _)| (*label, *label)).collect();
        let chosen = KINDS
            .iter()
            .position(|(kind, ..)| *kind == self.kind)
            .unwrap_or(0);
        if let Some(index) = tiles::segments(ui, &items, chosen, "The kind of harmonic")
            && let Some((kind, ..)) = KINDS.get(index)
        {
            self.kind = *kind;
        }
        let detail = KINDS
            .iter()
            .find(|(kind, ..)| *kind == self.kind)
            .map_or("", |(_, _, detail)| *detail);
        ui.label(RichText::new(detail).weak());
        ui.add_space(4.0);
        if has_touch_point(self.kind) {
            ui.horizontal(|ui| {
                ui.label("Touched");
                let response = ui.add(
                    egui::DragValue::new(&mut self.distance)
                        .range(1..=36)
                        .suffix(" frets"),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Touch point in frets above the note");
                });
                ui.label(format!(
                    "above the note: over fret {}",
                    self.fret.saturating_add(self.distance)
                ));
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for distance in TOUCH_POINTS {
                    let text = format!("+{distance}");
                    let on = self.distance == distance;
                    let tip = partial(distance)
                        .map_or_else(String::new, |n| format!("Sounds {}", interval_name(n)));
                    if ui
                        .add(egui::Button::new(text).selected(on))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.distance = distance;
                    }
                }
            });
        } else if self.kind == HarmonicKind::Natural && partial(self.fret).is_none() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "Fret {} is no harmonic node; natural harmonics ring over frets 12, 7, \
                     5, 4, 9, 19 or 24.",
                    self.fret
                ),
            );
        }
    }

    /// The partials picture.
    fn diagram(&self, ui: &mut egui::Ui) {
        // Where the string vibrates from: the nut for a natural harmonic, else the fret;
        // and where it is touched, counted from there.
        let from_fret = match self.kind {
            HarmonicKind::Natural | HarmonicKind::Semi => 0,
            _ => self.fret,
        };
        let touch = match self.kind {
            HarmonicKind::Natural => Some(self.fret),
            HarmonicKind::Semi => None,
            _ => Some(self.distance),
        };
        harmonic_diagram::draw(ui, from_fret, touch, self.chosen_partial());
    }

    /// What the draft sounds like.
    fn result(&self, ui: &mut egui::Ui) {
        let text = match (self.chosen_partial(), &self.sounds) {
            (Some(n), Some(pitch)) => format!(
                "Sounds {pitch}: the {} partial, {}.",
                ordinal(n),
                interval_name(n)
            ),
            (None, Some(pitch)) => format!("Sounds {pitch}, the fretted note."),
            (_, None) => String::new(),
        };
        ui.label(RichText::new(text).strong());
    }
}

/// "2nd", "3rd", "5th".
fn ordinal(n: u32) -> String {
    match n {
        2 => "2nd".to_owned(),
        3 => "3rd".to_owned(),
        _ => format!("{n}th"),
    }
}

/// Listen on the left; OK, Remove and Cancel on the right. `remove` and `listen` are the
/// tooltips of those buttons.
pub(super) fn buttons(ui: &mut egui::Ui, outcome: &mut Outcome, remove: &str, listen: &str) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 30.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            let button = |text: &str| egui::Button::new(text).min_size(vec2(88.0, 28.0));
            if ui.add(button("Cancel")).clicked() {
                *outcome = Outcome::Cancel;
            }
            if ui.add(button("Remove")).on_hover_text(remove).clicked() {
                *outcome = Outcome::Remove;
            }
            let tokens = style::tokens(ui);
            let ok = egui::Button::new(RichText::new("OK").color(tokens.on_accent))
                .fill(tokens.accent)
                .min_size(vec2(88.0, 28.0));
            if ui.add(ok).clicked() {
                *outcome = Outcome::Apply;
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if listen_button(ui).on_hover_text(listen).clicked() {
                    *outcome = Outcome::Listen;
                }
            });
        },
    );
}

/// Escape cancels and Enter applies while a dialog is still open.
pub(super) fn keys(ui: &egui::Ui, outcome: &mut Outcome) {
    if *outcome != Outcome::Open {
        return;
    }
    ui.input(|input| {
        if input.key_pressed(egui::Key::Escape) {
            *outcome = Outcome::Cancel;
        } else if input.key_pressed(egui::Key::Enter) {
            *outcome = Outcome::Apply;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partials_are_found_on_their_nodes() {
        assert_eq!(partial(12), Some(2));
        assert_eq!(partial(7), Some(3));
        assert_eq!(partial(19), Some(3));
        assert_eq!(partial(5), Some(4));
        assert_eq!(partial(24), Some(4));
        assert_eq!(partial(4), Some(5));
        assert_eq!(partial(1), None);
    }

    #[test]
    fn touched_harmonics_store_their_node_fret() {
        let mut editor = HarmonicEditor::new(None, 5);
        assert_eq!(editor.harmonic(5).node, None, "natural by default");
        editor.kind = HarmonicKind::Artificial;
        assert_eq!(editor.harmonic(5).node, Some(17));
        let reopened = HarmonicEditor::new(Some(editor.harmonic(5)), 5);
        assert_eq!(reopened.distance, 12);
        assert_eq!(reopened.chosen_partial(), Some(2));
    }
}
