//! The String sound dialog: the knobs of the modelled strings, kind by kind (nylon, steel,
//! electric, bass). Every change is heard at once (also while the song plays) and kept in
//! the settings; "Listen" plays a short phrase, "Preset" goes back to the built-in values.
//! Opened from the track panel's Sound section.

use std::ops::RangeInclusive;

use egui::{Margin, Pos2, RichText, vec2};
use gscore_guitar::ranges;
use gscore_play::StringKind;

use super::GuitarScoreApp;
use crate::app::style;
use crate::playback::{Heard, STRING_KINDS};
use crate::settings::StringSound;

/// Width of the dialog's content.
const WIDTH: f32 = 400.0;

/// The dialog while it is open: which kind of string it shows, and where it was dragged.
#[derive(Debug)]
pub(super) struct StringSoundDialog {
    kind: StringKind,
    pos: Option<Pos2>,
}

impl StringSoundDialog {
    /// The dialog showing `kind`.
    pub(super) fn of(kind: StringKind) -> Self {
        StringSoundDialog { kind, pos: None }
    }
}

/// The name of `kind` as the dialog shows it.
pub(super) fn kind_name(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Nylon => "Nylon guitar",
        StringKind::Steel => "Steel guitar",
        StringKind::Electric => "Electric guitar",
        StringKind::Bass => "Bass",
    }
}

/// What the user did in the dialog this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    listen: bool,
    preset: bool,
    turn_on: bool,
}

fn range((low, high): (f32, f32)) -> RangeInclusive<f32> {
    low..=high
}

/// One knob: a labelled slider with its explanation on hover. Returns whether it moved.
pub(super) fn knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    limits: (f32, f32),
    label: &str,
    unit: &str,
    logarithmic: bool,
    tip: &str,
) -> bool {
    let response = ui
        .horizontal(|ui| {
            ui.add_sized(
                vec2(130.0, 18.0),
                egui::Label::new(RichText::new(label).color(style::tokens(ui).text_weak)),
            )
            .on_hover_text(tip);
            let slider = egui::Slider::new(value, range(limits))
                .logarithmic(logarithmic)
                .suffix(unit)
                .max_decimals(if logarithmic { 2 } else { 3 });
            let response = ui.add(slider).on_hover_text(tip);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(label);
            });
            response
        })
        .inner;
    response.changed()
}

/// The knobs of one kind of string. Returns whether any moved.
fn knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
    ui.label(RichText::new("Strike").strong());
    changed |= knob(
        ui,
        &mut sound.pick_position,
        ranges::PICK_POSITION,
        "Pick position",
        "",
        false,
        "Where the pick strikes, as a share of the string from the bridge: near the bridge \
         is thin and twangy, toward the middle round and hollow.",
    );
    changed |= knob(
        ui,
        &mut sound.hardness,
        ranges::HARDNESS,
        "Hardness",
        "",
        false,
        "How hard the pick is: a soft thumb (0) to a hard pick (1). Harder strikes are \
         brighter.",
    );
    changed |= knob(
        ui,
        &mut sound.softness,
        ranges::SOFTNESS,
        "Roundness",
        "",
        false,
        "The shape of the strike: a sharp snap (0), bright and buzzy, to a rounded pluck \
         (1), soft. Lower notes sound less synthetic when it is rounder.",
    );
    ui.add_space(4.0);
    ui.label(RichText::new("String").strong());
    changed |= knob(
        ui,
        &mut sound.sustain,
        ranges::SUSTAIN,
        "Sustain",
        " s",
        true,
        "How long a ringing note takes to die away (60 dB) at the low A string's pitch.",
    );
    changed |= knob(
        ui,
        &mut sound.sustain_exponent,
        ranges::SUSTAIN_EXPONENT,
        "Shorter when high",
        "",
        false,
        "How much shorter higher notes ring: 0 the same as low ones, 1 much shorter.",
    );
    changed |= knob(
        ui,
        &mut sound.brilliance,
        ranges::BRILLIANCE,
        "Brilliance",
        " s",
        true,
        "How long the high overtones (around 2 kHz) ring, on every string alike: short is \
         mellow, long is bright and metallic.",
    );
    changed |= knob(
        ui,
        &mut sound.stiffness,
        ranges::STIFFNESS,
        "Stiffness",
        "",
        false,
        "A stiff string's high overtones run sharp: closer to -0.6 is more metallic and \
         bell-like, 0 is an ideal string.",
    );
    let mut stages = f32::from(sound.stiffness_stages);
    let limits = (
        ranges::STIFFNESS_STAGES.0 as f32,
        ranges::STIFFNESS_STAGES.1 as f32,
    );
    if knob(
        ui,
        &mut stages,
        limits,
        "Stiffness strength",
        "",
        false,
        "How many times the stiffness is applied (0 to 4).",
    ) {
        sound.stiffness_stages = stages.round().clamp(limits.0, limits.1) as u8;
        changed = true;
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Tone").strong());
    changed |= knob(
        ui,
        &mut sound.tone,
        ranges::TONE,
        "Tone",
        " Hz",
        true,
        "The pickup's or body's tone: overtones above this frequency are rolled off.",
    );
    changed |= knob(
        ui,
        &mut sound.resonance,
        ranges::RESONANCE,
        "Resonance",
        "",
        false,
        "A peak at the tone frequency, like a pickup's resonance: 0.5 flat, 4 a strong \
         nasal peak.",
    );
    changed |= knob(
        ui,
        &mut sound.level,
        ranges::LEVEL,
        "Level",
        "",
        false,
        "How loud these strings are against the rest of the mix.",
    );
    changed
}

impl GuitarScoreApp {
    /// Opens the String sound dialog on `kind`.
    pub(super) fn open_string_sound(&mut self, kind: StringKind) {
        match &mut self.string_sound {
            Some(dialog) => dialog.kind = kind,
            None => self.string_sound = Some(StringSoundDialog::of(kind)),
        }
    }

    /// The String sound dialog, while it is open.
    pub(super) fn show_string_sound(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.string_sound.take() else {
            return;
        };
        let title = "String sound";
        let size = vec2(WIDTH + 32.0, 520.0);
        let pos = *dialog
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut actions = Actions::default();
        let mut sound = self.playback.string_sounds().get(dialog.kind);
        let mut changed = false;
        let modelled = self.playback.modelled_guitars();
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut dialog.pos) {
                    actions.close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.horizontal(|ui| {
                            for kind in STRING_KINDS {
                                if ui
                                    .selectable_label(dialog.kind == kind, kind_name(kind))
                                    .clicked()
                                {
                                    dialog.kind = kind;
                                }
                            }
                        });
                        // A newly chosen kind shows its own knobs from the next frame.
                        sound = self.playback.string_sounds().get(dialog.kind);
                        ui.add_space(6.0);
                        if !modelled {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Guitars and basses play on the SoundFont now.")
                                        .weak(),
                                );
                                if ui.small_button("Use modelled strings").clicked() {
                                    actions.turn_on = true;
                                }
                            });
                            ui.add_space(4.0);
                        }
                        changed = knobs(ui, &mut sound);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button("Listen")
                                .on_hover_text("Play a short phrase on these strings")
                                .clicked()
                            {
                                actions.listen = true;
                            }
                            if ui
                                .button("Preset")
                                .on_hover_text("Back to the built-in sound of this kind")
                                .clicked()
                            {
                                actions.preset = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        actions.close = true;
                                    }
                                },
                            );
                        });
                        ui.label(
                            RichText::new(
                                "Changes are heard at once, also while the song plays, and \
                                 apply to every track of this kind.",
                            )
                            .weak(),
                        );
                    });
            });
        let kind = dialog.kind;
        if changed {
            self.playback.set_string_sound(kind, sound);
        }
        if actions.preset {
            self.playback.set_string_sound(
                kind,
                StringSound::from(gscore_audio::default_string_model(kind)),
            );
        }
        if actions.turn_on {
            self.set_modelled_guitars(true);
        }
        if actions.listen {
            match self.playback.play_string_demo(kind) {
                Ok(Heard::Skipped(why)) => self.notice = Some(format!("Not played: {why}")),
                Ok(_) => {}
                Err(error) => self.notice = Some(error),
            }
        }
        if !actions.close {
            self.string_sound = Some(dialog);
        }
    }
}
