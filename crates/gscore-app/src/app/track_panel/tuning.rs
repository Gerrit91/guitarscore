//! The tuning: the panel's "Tuning…" button and the dialog it opens. The dialog shows the
//! instrument's strings (the highest string at the top), lets the user pick a common tuning
//! or set each string's open note, and changes the string count and the capo.
//!
//! OK turns all of it into one [`Command::Batch`], so the whole change is one undo step.

use egui::{Align, Layout, Margin, Pos2, RichText, vec2};
use gscore_edit::Command;
use gscore_model::{MAX_STRINGS, Pitch, StringNumber, TrackIndex};

use super::TrackPanel;
use crate::app::harmonic_editor::keys;
pub(super) use crate::app::shape_editor::Outcome;
use crate::app::style;

/// The notes a string can be tuned to in the list: C0 to B7 (MIDI 12 to 107), which covers
/// every string instrument. A tuning outside it is still shown and can be stepped.
const LOWEST: u8 = 12;
const HIGHEST: u8 = 107;

/// Width of the dialog's content.
const WIDTH: f32 = 430.0;

/// A common tuning: its name and the open strings from the lowest up.
struct Preset {
    name: &'static str,
    pitches: &'static [u8],
}

/// The common tunings the dialog offers.
const PRESETS: &[Preset] = &[
    Preset {
        name: "E standard (guitar)",
        pitches: &[40, 45, 50, 55, 59, 64],
    },
    Preset {
        name: "Eb standard (guitar)",
        pitches: &[39, 44, 49, 54, 58, 63],
    },
    Preset {
        name: "D standard (guitar)",
        pitches: &[38, 43, 48, 53, 57, 62],
    },
    Preset {
        name: "Drop D",
        pitches: &[38, 45, 50, 55, 59, 64],
    },
    Preset {
        name: "Double drop D",
        pitches: &[38, 45, 50, 55, 59, 62],
    },
    Preset {
        name: "DADGAD",
        pitches: &[38, 45, 50, 55, 57, 62],
    },
    Preset {
        name: "Open G",
        pitches: &[38, 43, 50, 55, 59, 62],
    },
    Preset {
        name: "Open D",
        pitches: &[38, 45, 50, 54, 57, 62],
    },
    Preset {
        name: "Open C",
        pitches: &[36, 43, 48, 55, 60, 64],
    },
    Preset {
        name: "B standard (7-string)",
        pitches: &[35, 40, 45, 50, 55, 59, 64],
    },
    Preset {
        name: "Drop A (7-string)",
        pitches: &[33, 40, 45, 50, 55, 59, 64],
    },
    Preset {
        name: "F# standard (8-string)",
        pitches: &[30, 35, 40, 45, 50, 55, 59, 64],
    },
    Preset {
        name: "E standard (bass)",
        pitches: &[28, 33, 38, 43],
    },
    Preset {
        name: "Drop D (bass)",
        pitches: &[26, 33, 38, 43],
    },
    Preset {
        name: "B standard (5-string bass)",
        pitches: &[23, 28, 33, 38, 43],
    },
    Preset {
        name: "Drop A (5-string bass)",
        pitches: &[21, 28, 33, 38, 43],
    },
];

/// The open dialog.
#[derive(Debug, Clone)]
pub(super) struct TuningDialog {
    /// The track the tuning belongs to.
    track: usize,
    /// Where the window sits.
    pos: Option<Pos2>,
    /// The draft: string count, capo and each string's pitch (string 1 first).
    strings: usize,
    capo: u8,
    pitches: Vec<u8>,
    /// The values the dialog opened with, so OK only changes what differs.
    original_strings: usize,
    original_capo: u8,
    original_pitches: Vec<u8>,
    /// Why OK is disabled, if the draft cannot become a command.
    error: Option<String>,
}

impl TuningDialog {
    /// A dialog on `index`'s current tuning.
    pub(super) fn of(index: usize, snapshot: &TrackPanel) -> Self {
        let mut pitches: Vec<u8> = snapshot.tuning.iter().map(|s| s.pitch.get()).collect();
        if pitches.len() != snapshot.string_count {
            pitches = default_tuning(snapshot.string_count);
        }
        TuningDialog {
            track: index,
            pos: None,
            strings: snapshot.string_count.max(1),
            capo: snapshot.capo,
            original_strings: snapshot.string_count.max(1),
            original_capo: snapshot.capo,
            original_pitches: pitches.clone(),
            pitches,
            error: None,
        }
    }

    /// Draws the window while it is open.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Tuning";
        let size = vec2(WIDTH + 32.0, 430.0);
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
                        self.body(ui);
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        let mut reset = false;
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                let button =
                                    |text: &str| egui::Button::new(text).min_size(vec2(84.0, 28.0));
                                if ui.add(button("Cancel")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                let tokens = style::tokens(ui);
                                let ok =
                                    egui::Button::new(RichText::new("OK").color(tokens.on_accent))
                                        .fill(tokens.accent)
                                        .min_size(vec2(84.0, 28.0));
                                if ui.add(ok).clicked() {
                                    outcome = Outcome::Apply;
                                }
                                if ui
                                    .add(button("Reset"))
                                    .on_hover_text("Back to standard tuning for these strings")
                                    .clicked()
                                {
                                    reset = true;
                                }
                            },
                        );
                        if reset {
                            self.pitches = default_tuning(self.strings);
                            self.error = None;
                        }
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
                "String 1 is the highest. Pick a common tuning, or set each string's open \
                 note; the capo and the string count are below.",
            )
            .weak(),
        );
        ui.add_space(6.0);

        // A common tuning.
        let matching = PRESETS.iter().find(|preset| {
            preset.pitches.len() == self.pitches.len()
                && preset
                    .pitches
                    .iter()
                    .rev()
                    .copied()
                    .eq(self.pitches.iter().copied())
        });
        let mut chosen = matching.map(|preset| preset.name);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Common tuning").color(style::tokens(ui).text_weak));
            let combo = egui::ComboBox::from_id_salt("common_tuning")
                .selected_text(chosen.unwrap_or("Custom"))
                .width(WIDTH - 130.0)
                .show_ui(ui, |ui| {
                    for preset in PRESETS {
                        let label = format!("{} ({} strings)", preset.name, preset.pitches.len());
                        if ui
                            .selectable_label(chosen == Some(preset.name), label)
                            .clicked()
                        {
                            chosen = Some(preset.name);
                            ui.close();
                        }
                    }
                });
            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                node.set_label("Common tuning");
            });
        });
        if let Some(name) = chosen
            && matching.map(|preset| preset.name) != Some(name)
            && let Some(preset) = PRESETS.iter().find(|preset| preset.name == name)
        {
            self.strings = preset.pitches.len();
            self.pitches = preset.pitches.iter().rev().copied().collect();
            self.error = None;
        }

        ui.add_space(8.0);
        self.strings_visual(ui);
        ui.add_space(8.0);

        // String count and capo.
        ui.horizontal(|ui| {
            ui.label("Strings");
            let response = ui.add(
                egui::DragValue::new(&mut self.strings)
                    .range(1..=MAX_STRINGS)
                    .speed(1)
                    .update_while_editing(false),
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Strings"));
            ui.add_space(12.0);
            ui.label("Capo");
            let response = ui.add(
                egui::DragValue::new(&mut self.capo)
                    .range(0..=127)
                    .speed(1)
                    .update_while_editing(false),
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Capo"));
            if self.capo == 0 {
                ui.label(RichText::new("none").weak());
            }
        });
        // Keep the drafts in step with the string count: a new string starts in fourths
        // below the lowest, as the model adds it.
        if self.pitches.len() != self.strings {
            if self.strings > self.pitches.len() {
                let mut pitch = self
                    .pitches
                    .last()
                    .copied()
                    .unwrap_or_else(|| default_tuning(1).first().copied().unwrap_or(64));
                while self.pitches.len() < self.strings {
                    pitch = pitch.saturating_sub(5);
                    self.pitches.push(pitch);
                }
            } else {
                self.pitches.truncate(self.strings);
            }
        }
    }

    /// The strings as a headstock: a nut at the left and one line per string, thickest for
    /// the lowest, with its note beside it.
    fn strings_visual(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let count = self.strings;
        for index in 0..count {
            let number = index.saturating_add(1);
            ui.horizontal(|ui| {
                let (line, _) = ui.allocate_exact_size(vec2(90.0, 16.0), egui::Sense::hover());
                let thickness = 1.0 + (index as f32 / count.max(1) as f32) * 2.4;
                let y = line.center().y;
                // The nut.
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(line.left(), y - 6.0),
                        egui::pos2(line.left() + 4.0, y + 6.0),
                    ),
                    1.0,
                    tokens.text_weak,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(line.left() + 4.0, y),
                        egui::pos2(line.right(), y),
                    ],
                    egui::Stroke::new(thickness, tokens.text),
                );
                ui.label(
                    RichText::new(format!("String {number}"))
                        .small()
                        .color(tokens.text_weak),
                );
                let current = self.pitches.get(index).copied().unwrap_or(LOWEST);
                let mut pitch = current;
                if ui
                    .small_button("−")
                    .on_hover_text("A semitone lower")
                    .clicked()
                {
                    pitch = pitch.saturating_sub(1);
                }
                let combo = egui::ComboBox::from_id_salt(("tuning_note", index))
                    .selected_text(note_name(current))
                    .width(58.0)
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
                if pitch != current
                    && let Some(slot) = self.pitches.get_mut(index)
                {
                    *slot = pitch;
                }
            });
        }
    }

    /// The change the draft describes: one batch, so it is one undo step.
    pub(super) fn commands(&self) -> Vec<Command> {
        let track = TrackIndex::new(self.track);
        let mut commands = Vec::new();
        if self.strings != self.original_strings {
            commands.push(Command::SetTrackStrings {
                track,
                strings: self.strings,
            });
        }
        if self.capo != self.original_capo {
            commands.push(Command::SetTrackCapo {
                track,
                capo: self.capo,
            });
        }
        for (index, &pitch) in self.pitches.iter().enumerate() {
            if self.original_pitches.get(index).copied() == Some(pitch) {
                continue;
            }
            let Ok(number) = u8::try_from(index.saturating_add(1)) else {
                continue;
            };
            let (Ok(string), Ok(pitch)) = (StringNumber::new(number), Pitch::new(pitch)) else {
                continue;
            };
            commands.push(Command::SetTrackStringPitch {
                track,
                string,
                pitch,
            });
        }
        match commands.len() {
            0 => Vec::new(),
            1 => commands,
            _ => vec![Command::Batch { commands }],
        }
    }

    /// The string count the draft asks for (for checking against the song).
    pub(super) fn strings(&self) -> usize {
        self.strings
    }

    /// The track the dialog changes.
    pub(super) fn track(&self) -> usize {
        self.track
    }

    /// Shows why OK cannot apply the draft.
    pub(super) fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }
}

/// The panel's button: the current tuning's name, opening the dialog.
pub(super) fn button(ui: &mut egui::Ui, snapshot: &TrackPanel, open: &mut bool) {
    let enabled = snapshot.kind == gscore_model::InstrumentKind::Strings && !snapshot.percussion;
    let name = snapshot
        .tuning_name
        .clone()
        .unwrap_or_else(|| "Set a tuning…".to_owned());
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(format!("Tuning: {name}…")).min_size(vec2(ui.available_width(), 24.0)),
    );
    if !enabled {
        response
            .clone()
            .on_disabled_hover_text("This track is not a string instrument, so it has no tuning");
    }
    if response
        .on_hover_text("Choose a common tuning or set each string's open note")
        .clicked()
    {
        *open = true;
    }
}

/// The note name of a MIDI pitch ("E2", "F#3").
pub(super) fn note_name(pitch: u8) -> String {
    Pitch::new(pitch).map_or_else(|_| "?".to_owned(), |pitch| pitch.to_string())
}

/// A standard tuning for `count` strings, from string 1 down.
fn default_tuning(count: usize) -> Vec<u8> {
    let low_to_high: Vec<u8> = match count {
        0 | 1 => vec![64],
        2 => vec![55, 60],
        3 => vec![48, 55, 60],
        4 => vec![28, 33, 38, 43],
        5 => vec![33, 38, 43, 48, 53],
        6 => vec![40, 45, 50, 55, 59, 64],
        7 => vec![35, 40, 45, 50, 55, 59, 64],
        8 => vec![30, 35, 40, 45, 50, 55, 59, 64],
        // Beyond eight strings, all fourths below the eight-string tuning.
        _ => {
            let mut pitches = vec![30, 35, 40, 45, 50, 55, 59, 64];
            while pitches.len() < count && pitches.len() < MAX_STRINGS {
                let low = pitches.first().copied().unwrap_or(30_u8).saturating_sub(5);
                pitches.insert(0, low);
            }
            pitches
        }
    };
    low_to_high.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_tunings_have_the_right_number_of_strings() {
        for preset in PRESETS {
            assert!(
                (4..=8).contains(&preset.pitches.len()),
                "{}: {} strings",
                preset.name,
                preset.pitches.len()
            );
        }
    }

    #[test]
    fn a_standard_tuning_is_six_strings_low_to_high() {
        assert_eq!(default_tuning(6), vec![64, 59, 55, 50, 45, 40]);
        assert_eq!(default_tuning(4), vec![43, 38, 33, 28]);
    }
}
