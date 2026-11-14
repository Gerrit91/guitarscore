//! The transpose window: moves what the song sounds like (Guitar Pro's track transpose,
//! which leaves the written notes and the tablature as they are) or the notes themselves,
//! moved on the fretboard so the tablature shows the new pitches ("change the fingerings").
//! It can transpose the whole song or only the shown track.

use egui::{Align, Layout, Margin, Pos2, RichText, vec2};
use gscore_edit::Command;
use gscore_model::{MAX_TRANSPOSE, TrackIndex};

use super::GuitarScoreApp;
use super::shape_editor::Outcome;
use super::style;

/// The window's content width and height.
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 250.0;

/// What the window transposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The whole song.
    Song,
    /// Only the track the panels show.
    Track,
}

/// The open transpose window.
#[derive(Debug)]
pub(super) struct TransposeWindow {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<Pos2>,
    /// How far to transpose, in semitones.
    semitones: i32,
    scope: Scope,
    /// Move the notes themselves (change the fingerings) instead of only the sound.
    fingerings: bool,
    /// Why the last Transpose click did nothing, if it did nothing.
    error: Option<String>,
}

impl Default for TransposeWindow {
    fn default() -> Self {
        TransposeWindow {
            pos: None,
            semitones: 0,
            scope: Scope::Song,
            fingerings: false,
            error: None,
        }
    }
}

impl GuitarScoreApp {
    /// Opens the transpose window (or brings it up again).
    pub(super) fn open_transpose(&mut self) {
        if self.transpose.is_none() {
            self.transpose = Some(TransposeWindow::default());
        }
    }

    /// The transpose window, while it is open.
    pub(super) fn show_transpose(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.transpose.take() else {
            return;
        };
        // The shown track, for the track scope.
        let track = self.tabs.get(self.active).map(|tab| tab.track);
        let track_name = self
            .tabs
            .get(self.active)
            .and_then(|tab| {
                let name = tab
                    .document
                    .song()
                    .track(TrackIndex::new(tab.track))
                    .map(|track| track.name().trim().to_owned())
                    .unwrap_or_default();
                (!name.is_empty()).then_some(name)
            })
            .unwrap_or_else(|| "the shown track".to_owned());
        let title = "Transpose";
        let size = vec2(WIDTH + 32.0, HEIGHT);
        let pos = *window
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
                if style::window_title_bar(ui, title, &mut window.pos) {
                    outcome = Outcome::Cancel;
                }
                style::dialog_body(ui, Margin::symmetric(16, 12), |ui| {
                    ui.set_width(WIDTH);
                    let tokens = style::tokens(ui);
                    ui.label(
                        RichText::new("Transpose by")
                            .color(tokens.text_weak)
                            .size(12.0),
                    );
                    ui.horizontal(|ui| {
                        let lower = ui.add_enabled(
                            window.semitones > -MAX_TRANSPOSE,
                            egui::Button::new("−").min_size(vec2(28.0, 24.0)),
                        );
                        ui.ctx().accesskit_node_builder(lower.id, |node| {
                            node.set_label("A semitone lower");
                        });
                        if lower.on_hover_text("A semitone lower").clicked() {
                            window.semitones = window.semitones.saturating_sub(1);
                        }
                        let value = ui.add_sized(
                            vec2(56.0, 24.0),
                            egui::Label::new(
                                RichText::new(step_value(window.semitones))
                                    .size(16.0)
                                    .strong(),
                            ),
                        );
                        ui.ctx().accesskit_node_builder(value.id, |node| {
                            node.set_label(format!(
                                "Transposition: {}",
                                step_text(window.semitones)
                            ));
                        });
                        let higher = ui.add_enabled(
                            window.semitones < MAX_TRANSPOSE,
                            egui::Button::new("+").min_size(vec2(28.0, 24.0)),
                        );
                        ui.ctx().accesskit_node_builder(higher.id, |node| {
                            node.set_label("A semitone higher");
                        });
                        if higher.on_hover_text("A semitone higher").clicked() {
                            window.semitones = window.semitones.saturating_add(1);
                        }
                        ui.label(
                            RichText::new(match window.semitones {
                                0 => "no change yet".to_owned(),
                                12 => "an octave up".to_owned(),
                                -12 => "an octave down".to_owned(),
                                step if step > 0 => format!("{step} semitones up"),
                                step => format!("{} semitones down", step.saturating_neg()),
                            })
                            .color(tokens.text_weak)
                            .size(12.0),
                        );
                    });
                    ui.add_space(6.0);
                    ui.radio_value(&mut window.scope, Scope::Song, "The whole song");
                    ui.radio_value(
                        &mut window.scope,
                        Scope::Track,
                        format!("Only {track_name}"),
                    );
                    ui.add_space(4.0);
                    ui.checkbox(&mut window.fingerings, "Change the fingerings")
                        .on_hover_text(
                            "Move the notes themselves on the fretboard, re-fingered onto the \
                             strings, so the tablature shows the new pitches. Off, only the \
                             sound moves and the written notes stay as they are.",
                        );
                    if let Some(error) = &window.error {
                        ui.add_space(4.0);
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.add_space(8.0);
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 30.0),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            let button =
                                |text: &str| egui::Button::new(text).min_size(vec2(96.0, 28.0));
                            if ui.add(button("Cancel")).clicked() {
                                outcome = Outcome::Cancel;
                            }
                            let ready = window.semitones != 0;
                            let tokens = style::tokens(ui);
                            let apply = egui::Button::new(
                                RichText::new("Transpose").color(tokens.on_accent),
                            )
                            .fill(tokens.accent)
                            .min_size(vec2(96.0, 28.0));
                            let apply = ui
                                .add_enabled(ready, apply)
                                .on_disabled_hover_text("Choose how far to transpose first");
                            ui.ctx().accesskit_node_builder(apply.id, |node| {
                                node.set_label("Apply the transposition");
                            });
                            let clicked = apply.clicked();
                            if clicked
                                && apply_transpose(
                                    self,
                                    window.scope,
                                    track,
                                    window.semitones,
                                    window.fingerings,
                                    &mut window.error,
                                )
                            {
                                outcome = Outcome::Apply;
                            }
                        },
                    );
                    if outcome == Outcome::Open {
                        ui.input(|input| {
                            if input.key_pressed(egui::Key::Escape) {
                                outcome = Outcome::Cancel;
                            }
                        });
                    }
                });
            });
        if outcome == Outcome::Open {
            self.transpose = Some(window);
        }
    }
}

/// Runs the transposition the window asks for; `true` when it was applied (the window then
/// closes), `false` with the reason in `error` when it could not be.
fn apply_transpose(
    app: &mut GuitarScoreApp,
    scope: Scope,
    track: Option<usize>,
    semitones: i32,
    fingerings: bool,
    error: &mut Option<String>,
) -> bool {
    let Some(tab) = app.tabs.get_mut(app.active) else {
        *error = Some("no song is open".to_owned());
        return false;
    };
    let at = TrackIndex::new(track.unwrap_or(tab.track));
    let command = match (fingerings, scope) {
        (true, Scope::Track) => Command::TransposeNotes {
            track: Some(at),
            semitones,
        },
        (true, Scope::Song) => Command::TransposeNotes {
            track: None,
            semitones,
        },
        (false, Scope::Track) => Command::TransposeTrack {
            track: at,
            semitones,
        },
        (false, Scope::Song) => Command::TransposeSong { semitones },
    };
    let _ = tab;
    match app.run_command_from_window(command) {
        Ok(()) => {
            let what = match (fingerings, scope) {
                (true, Scope::Song) => "Moved every track's notes".to_owned(),
                (true, Scope::Track) => "Moved the track's notes".to_owned(),
                (false, Scope::Song) => "Transposed the song".to_owned(),
                (false, Scope::Track) => "Transposed the track".to_owned(),
            };
            app.status = Some(format!("{what} {}", step_text(semitones)));
            true
        }
        Err(failure) => {
            *error = Some(failure);
            false
        }
    }
}

/// The value in the middle of the window: "0", "+3", "−12".
fn step_value(semitones: i32) -> String {
    match semitones {
        0 => "0".to_owned(),
        step if step > 0 => format!("+{step}"),
        step => format!("-{}", step.saturating_neg()),
    }
}

/// "up 3 semitones", "down an octave".
fn step_text(semitones: i32) -> String {
    match semitones {
        12 => "up an octave".to_owned(),
        -12 => "down an octave".to_owned(),
        step if step > 0 => format!("up {step} semitones"),
        step => format!("down {} semitones", step.saturating_neg()),
    }
}
