//! The tuner: a reference tone (a sine wave) for tuning a guitar by ear.
//!
//! The window shows the tone as a wave (denser for higher notes, moving while it sounds),
//! the chosen note and its frequency,
//! a note picker, the shown track's open strings as quick choices, and Play/Stop. The tone
//! comes from `gscore_audio::TunerPlayer` on the default output device; the rest of the app
//! keeps working while it sounds.

use egui::{Align, Layout, Margin, Pos2, RichText, Sense, Stroke, vec2};
use gscore_model::{Pitch, TrackIndex};

use super::GuitarScoreApp;
use super::shape_editor::Outcome;
use super::style;

/// The dialog's content width and height.
const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 430.0;
/// The lowest and highest note the picker offers (C1 to C8).
const LOWEST_NOTE: u8 = 24;
const HIGHEST_NOTE: u8 = 108;
/// A4 (MIDI 69) is 440 Hz.
const CONCERT_A: f32 = 440.0;
const CONCERT_A_NOTE: u8 = 69;

/// What the window asks the tone player to do after it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToneRequest {
    /// Play (or retune to) this note.
    Play(u8),
    /// Silence the tone.
    Stop,
}

/// The open tuner.
#[derive(Debug)]
pub(super) struct TunerState {
    /// Where the window sits.
    pos: Option<Pos2>,
    /// The note whose frequency is played (a MIDI note number).
    note: u8,
    /// The tone generator, once it was started.
    player: Option<gscore_audio::TunerPlayer>,
    /// Why the tone does not sound, if it does not.
    error: Option<String>,
}

impl Default for TunerState {
    fn default() -> Self {
        TunerState {
            pos: None,
            // A4: the usual reference.
            note: CONCERT_A_NOTE,
            player: None,
            error: None,
        }
    }
}

/// The frequency of a MIDI note, in hertz.
pub(super) fn frequency(note: u8) -> f32 {
    let semitones = f32::from(note) - f32::from(CONCERT_A_NOTE);
    CONCERT_A * 2.0_f32.powf(semitones / 12.0)
}

/// The note name of a MIDI note ("A4", "E2").
fn note_name(note: u8) -> String {
    Pitch::new(note).map_or_else(|_| "?".to_owned(), |pitch| pitch.to_string())
}

/// `note` moved by `step` semitones, clamped to the picker's range.
fn semitone(note: u8, step: i8) -> u8 {
    let moved = i16::from(note).saturating_add(i16::from(step));
    let low = i16::from(LOWEST_NOTE);
    let high = i16::from(HIGHEST_NOTE);
    u8::try_from(moved.clamp(low, high)).unwrap_or(note)
}

impl GuitarScoreApp {
    /// Opens the tuner window (or brings it up again).
    pub(super) fn open_tuner(&mut self) {
        if self.tuner.is_none() {
            self.tuner = Some(TunerState::default());
        }
    }

    /// The tuner window, while it is open.
    pub(super) fn show_tuner(&mut self, ctx: &egui::Context) {
        let Some(mut tuner) = self.tuner.take() else {
            return;
        };
        // The shown track's open strings, for quick choices (empty without a tuning).
        let strings: Vec<(u8, u8)> = self
            .tabs
            .get(self.active)
            .and_then(|tab| {
                tab.document
                    .song()
                    .track(TrackIndex::new(tab.track))
                    .and_then(|track| track.tuning().ok().flatten())
            })
            .unwrap_or_default()
            .into_iter()
            .map(|string| (string.string.get(), string.pitch.get()))
            .collect();
        // A device that failed after it was opened.
        if tuner.error.is_none()
            && let Some(error) = tuner.player.as_ref().and_then(|player| player.error())
        {
            tuner.error = Some(error);
        }

        let title = "Tuner";
        let size = vec2(WIDTH + 32.0, HEIGHT);
        let pos = *tuner
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        let mut request = None;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut tuner.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        let playing = tuner
                            .player
                            .as_ref()
                            .is_some_and(|player| player.is_playing());
                        picture(ui, tuner.note, playing);
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(note_name(tuner.note))
                                    .size(34.0)
                                    .strong()
                                    .color(style::tokens(ui).text),
                            );
                            ui.label(
                                RichText::new(format!("{:.2} Hz", frequency(tuner.note)))
                                    .size(14.0)
                                    .color(style::tokens(ui).text_weak),
                            );
                        });
                        ui.add_space(8.0);
                        let mut note = tuner.note;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Note").color(style::tokens(ui).text_weak));
                            if ui
                                .add_enabled(note > LOWEST_NOTE, egui::Button::new("−"))
                                .on_hover_text("A semitone lower")
                                .clicked()
                            {
                                note = semitone(note, -1);
                            }
                            let combo = egui::ComboBox::from_id_salt("tuner_note")
                                .selected_text(note_name(note))
                                .width(WIDTH - 140.0)
                                .show_ui(ui, |ui| {
                                    for candidate in (LOWEST_NOTE..=HIGHEST_NOTE).rev() {
                                        ui.selectable_value(
                                            &mut note,
                                            candidate,
                                            format!(
                                                "{} — {:.2} Hz",
                                                note_name(candidate),
                                                frequency(candidate)
                                            ),
                                        );
                                    }
                                });
                            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                                node.set_label("Note to play");
                            });
                            if ui
                                .add_enabled(note < HIGHEST_NOTE, egui::Button::new("+"))
                                .on_hover_text("A semitone higher")
                                .clicked()
                            {
                                note = semitone(note, 1);
                            }
                        });
                        if note != tuner.note {
                            tuner.note = note;
                            request = Some(ToneRequest::Play(note));
                        }
                        if !strings.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("Open string of the shown track")
                                    .small()
                                    .color(style::tokens(ui).text_weak),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                for (number, pitch) in &strings {
                                    let label = format!("{number}: {}", note_name(*pitch));
                                    if ui
                                        .add(
                                            egui::Button::new(label)
                                                .small()
                                                .selected(tuner.note == *pitch),
                                        )
                                        .on_hover_text(format!(
                                            "{} — {:.2} Hz",
                                            note_name(*pitch),
                                            frequency(*pitch)
                                        ))
                                        .clicked()
                                    {
                                        tuner.note = *pitch;
                                        request = Some(ToneRequest::Play(*pitch));
                                    }
                                }
                            });
                        }
                        if let Some(error) = &tuner.error {
                            ui.add_space(4.0);
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                let button =
                                    |text: &str| egui::Button::new(text).min_size(vec2(96.0, 28.0));
                                if ui.add(button("Close")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                let tokens = style::tokens(ui);
                                let text = if playing { "Stop" } else { "Play" };
                                let tone =
                                    egui::Button::new(RichText::new(text).color(tokens.on_accent))
                                        .fill(tokens.accent)
                                        .min_size(vec2(96.0, 28.0));
                                if ui.add(tone).clicked() {
                                    request = Some(if playing {
                                        ToneRequest::Stop
                                    } else {
                                        ToneRequest::Play(tuner.note)
                                    });
                                }
                            },
                        );
                        if outcome == Outcome::Open {
                            // A focused button already takes Space as its click.
                            let focused = ui.memory(|memory| memory.focused().is_some());
                            ui.input(|input| {
                                if input.key_pressed(egui::Key::Escape) {
                                    outcome = Outcome::Cancel;
                                } else if input.key_pressed(egui::Key::Space) && !focused {
                                    request = Some(if playing {
                                        ToneRequest::Stop
                                    } else {
                                        ToneRequest::Play(tuner.note)
                                    });
                                }
                            });
                        }
                    });
            });
        match request {
            Some(ToneRequest::Play(note)) => start_or_retune(&mut tuner, note),
            Some(ToneRequest::Stop) => {
                if let Some(player) = &tuner.player {
                    player.stop();
                }
            }
            None => {}
        }
        match outcome {
            Outcome::Cancel | Outcome::Apply | Outcome::Remove | Outcome::Listen => {
                if let Some(player) = &tuner.player {
                    player.stop();
                }
            }
            Outcome::Open => self.tuner = Some(tuner),
        }
    }
}

/// Plays `note`: starts the tone generator the first time, retunes it afterwards.
fn start_or_retune(tuner: &mut TunerState, note: u8) {
    // A device that failed (unplugged, busy) is opened again on the next Play.
    if tuner
        .player
        .as_ref()
        .is_some_and(|player| player.error().is_some())
    {
        tuner.player = None;
    }
    if tuner.player.is_none() {
        match gscore_audio::TunerPlayer::start() {
            Ok(player) => tuner.player = Some(player),
            Err(error) => {
                tuner.error = Some(error.to_string());
                return;
            }
        }
    }
    let Some(player) = &tuner.player else {
        return;
    };
    match player.play(frequency(note)) {
        Ok(()) => tuner.error = player.error(),
        Err(error) => tuner.error = Some(error.to_string()),
    }
}

/// How many cycles of the wave the picture shows for `note`: one and a half at the
/// lowest note up to nine at the highest, evenly per octave, so a higher note reads as a
/// denser wave.
fn cycles(note: u8) -> f32 {
    let span = f32::from(HIGHEST_NOTE - LOWEST_NOTE);
    let t = (f32::from(note.clamp(LOWEST_NOTE, HIGHEST_NOTE) - LOWEST_NOTE) / span).clamp(0.0, 1.0);
    1.5 + t * 7.5
}

/// The picture: the reference tone as a sine wave across the panel. Its density follows
/// the pitch; while the tone sounds it is drawn in the accent color and moves, silent it
/// rests in a muted gray. It fades out at both ends.
fn picture(ui: &mut egui::Ui, note: u8, playing: bool) {
    let tokens = style::tokens(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 104.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, tokens.control);
    let inner = rect.shrink2(vec2(18.0, 16.0));
    let mid = inner.center().y;
    painter.line_segment(
        [
            egui::pos2(inner.left(), mid),
            egui::pos2(inner.right(), mid),
        ],
        Stroke::new(1.0, tokens.line),
    );

    let (color, amplitude, phase) = if playing {
        // A slow drift, the same for every pitch: it says "sounding", it is not the tone.
        let time = ui.input(|input| input.time) as f32;
        ui.ctx().request_repaint();
        (tokens.accent, inner.height() / 2.0, time * 0.8)
    } else {
        (tokens.text_weak, inner.height() * 0.32, 0.0)
    };
    let cycles = cycles(note);
    let steps = 240;
    let point = |step: usize| {
        let t = step as f32 / steps as f32;
        let angle = std::f32::consts::TAU * (t * cycles - phase);
        egui::pos2(
            inner.left() + t * inner.width(),
            mid - angle.sin() * amplitude,
        )
    };
    let points: Vec<_> = (0..=steps).map(point).collect();
    // Fade in and out over the outer fifth on each side.
    let (left, width) = (inner.left(), inner.width().max(1.0));
    let stroke = egui::epaint::PathStroke::new_uv(2.0, move |_, pos| {
        let t = ((pos.x - left) / width).clamp(0.0, 1.0);
        let edge = (t.min(1.0 - t) / 0.2).clamp(0.0, 1.0);
        color.gamma_multiply(edge * edge * (3.0 - 2.0 * edge))
    });
    painter.add(egui::epaint::PathShape::line(points, stroke));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concert_a_is_440_hertz_and_the_octave_doubles_it() {
        assert!((frequency(69) - 440.0).abs() < 0.001);
        assert!((frequency(81) - 880.0).abs() < 0.001);
        assert!((frequency(57) - 220.0).abs() < 0.001);
        // The guitar's open low E.
        assert!((frequency(40) - 82.41).abs() < 0.02);
    }

    #[test]
    fn the_wave_gets_denser_with_the_pitch() {
        assert!((cycles(LOWEST_NOTE) - 1.5).abs() < 0.001);
        assert!((cycles(HIGHEST_NOTE) - 9.0).abs() < 0.001);
        assert!(cycles(40) < cycles(64) && cycles(64) < cycles(CONCERT_A_NOTE + 12));
    }

    #[test]
    fn the_semitone_buttons_stop_at_the_picker_ends() {
        assert_eq!(semitone(69, 1), 70);
        assert_eq!(semitone(69, -1), 68);
        assert_eq!(semitone(LOWEST_NOTE, -1), LOWEST_NOTE);
        assert_eq!(semitone(HIGHEST_NOTE, 1), HIGHEST_NOTE);
    }
}
