//! The knobs of the modelled strings, as the String sound window shows them
//! (`string_sound_window`): strike, string and tone, each explained on hover.

use std::ops::RangeInclusive;

use egui::{RichText, vec2};
use gscore_guitar::ranges;
use gscore_play::StringKind;

use crate::app::style;
use crate::settings::StringSound;

/// The name of `kind` as the dialog shows it.
pub(super) fn kind_name(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Nylon => "Nylon guitar",
        StringKind::Steel => "Steel guitar",
        StringKind::Electric => "Electric guitar",
        StringKind::Bass => "Bass",
    }
}

fn range((low, high): (f32, f32)) -> RangeInclusive<f32> {
    low..=high
}

/// One knob: a labelled slider with its explanation on hover. Returns whether it moved.
/// One knob: a label, then its slider. Returns whether the value moved.
pub(super) fn knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    limits: (f32, f32),
    label: &str,
    unit: &str,
    logarithmic: bool,
    tip: &str,
) -> bool {
    knob_response(ui, value, limits, label, unit, logarithmic, tip).changed()
}

/// One knob's slider, for callers that also need its drag state (a change that rebuilds
/// something is applied when the knob is let go rather than while it turns).
pub(super) fn knob_response(
    ui: &mut egui::Ui,
    value: &mut f32,
    limits: (f32, f32),
    label: &str,
    unit: &str,
    logarithmic: bool,
    tip: &str,
) -> egui::Response {
    ui.horizontal(|ui| {
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
    .inner
}

/// The knobs of one kind of string. Returns whether any moved.
pub(super) fn knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
    // Two columns, so the window fits a small screen: the strike and the tone on the
    // left, the string on the right.
    ui.columns(2, |columns| {
        if let [left, right] = columns {
            changed |= strike_and_tone(left, sound);
            changed |= string_knobs(right, sound);
        }
    });
    changed
}

/// The strike's and the tone's knobs. Returns whether any moved.
fn strike_and_tone(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
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
    ui.label(RichText::new("Tone").strong());
    changed |= knob(
        ui,
        &mut sound.bridge,
        ranges::BRIDGE,
        "Bridge",
        "",
        false,
        "How much of what is heard is the force the string puts on the bridge (or a \
         magnetic pickup's signal) rather than its swing: 0 full and round, 1 lean in the \
         lows like a real guitar. Takes away the synth-bass character of low notes.",
    );
    changed |= knob(
        ui,
        &mut sound.low_cut,
        ranges::LOW_CUT,
        "Low cut",
        " Hz",
        true,
        "The body or pickup barely passes the lowest frequencies: below this they are \
         rolled off.",
    );
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

/// The string's knobs. Returns whether any moved.
fn string_knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
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
    changed |= knob(
        ui,
        &mut sound.second,
        ranges::SECOND,
        "Second vibration",
        "",
        false,
        "A real string also vibrates in a second direction that decays faster; together \
         they beat and give a long note its living, two-stage decay. 0 is one perfectly \
         even vibration (more synthetic).",
    );
    changed |= knob(
        ui,
        &mut sound.detune,
        ranges::DETUNE,
        "Detune",
        " ct",
        false,
        "How far the second vibration is detuned: the speed of the beating.",
    );
    changed
}
