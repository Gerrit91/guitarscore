//! The knobs of the modelled strings and of the chain's blocks, as the Sound chain window
//! shows them: round knobs with their name and value under them, the details on hover.

use egui::{Align2, FontId, RichText, Sense, Stroke, Vec2, pos2, vec2};
use gscore_guitar::ranges;
use gscore_play::StringKind;

use crate::app::style;
use crate::settings::StringSound;

/// The space one knob takes: the knob, its name and its value.
const CELL: Vec2 = vec2(78.0, 60.0);
/// The knob's radius and where its centre sits in the cell.
const RADIUS: f32 = 13.0;
const CENTRE_Y: f32 = 16.0;

/// The name of `kind` as the windows show it.
pub(super) fn kind_name(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Nylon => "Nylon guitar",
        StringKind::Steel => "Steel guitar",
        StringKind::Electric => "Electric guitar",
        StringKind::Bass => "Bass",
    }
}

/// What a knob did this frame.
#[derive(Default, Clone, Copy)]
pub(super) struct KnobEdit {
    /// The value moved.
    pub(super) changed: bool,
    /// A drag ended (for a value that is rebuilt when it is let go).
    pub(super) released: bool,
}

/// A round knob: drag sideways (or use the arrow keys) to turn it, double-click for the
/// default. Its name and its value are under it, the explanation on hover. `logarithmic`
/// spreads frequencies (and other wide ranges) evenly.
#[allow(clippy::too_many_arguments)]
pub(super) fn round_knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    limits: (f32, f32),
    label: &str,
    unit: &str,
    reset: f32,
    logarithmic: bool,
    tip: &str,
) -> KnobEdit {
    let (low, high) = limits;
    let fraction = |value: f32| -> f32 {
        let share = if logarithmic {
            (value.max(low).ln() - low.ln()) / (high.ln() - low.ln())
        } else {
            (value - low) / (high - low)
        };
        share.clamp(0.0, 1.0)
    };
    let to_value = |fraction: f32| -> f32 {
        let value = if logarithmic {
            (low.ln() + (fraction.clamp(0.0, 1.0)) * (high.ln() - low.ln())).exp()
        } else {
            low + fraction.clamp(0.0, 1.0) * (high - low)
        };
        value.clamp(low, high)
    };
    let tokens = style::tokens(ui);
    let (rect, response) = ui.allocate_exact_size(CELL, Sense::click_and_drag());
    let hover = format!("{label}: {}{unit}\n{tip}", value_text(*value, unit));
    let response = response.on_hover_text(hover);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Slider);
        node.set_min_numeric_value(f64::from(low));
        node.set_max_numeric_value(f64::from(high));
        node.set_numeric_value(f64::from(*value));
    });
    let mut edit = KnobEdit::default();
    if response.double_clicked() {
        if (*value - reset).abs() > f32::EPSILON {
            *value = reset;
            edit.changed = true;
        }
        edit.released = true;
    } else if response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().x);
        // A drag across the knob's whole width turns it a little; 240 px cover the range.
        let new = to_value(fraction(*value) + delta / 240.0);
        if (new - *value).abs() > f32::EPSILON {
            *value = new;
            edit.changed = true;
        }
    }
    if response.drag_stopped() {
        edit.released = true;
    }
    // The knob: a faint disc, its arc filled from the minimum, and a pointer.
    let centre = pos2(rect.center().x, rect.top() + CENTRE_Y);
    let painter = ui.painter();
    painter.circle_filled(centre, RADIUS, tokens.control);
    painter.circle_stroke(centre, RADIUS, Stroke::new(1.0, tokens.line));
    let angle = |share: f32| (share * 270.0 - 135.0).to_radians() - std::f32::consts::FRAC_PI_2;
    let at = fraction(*value);
    if at > 0.001 {
        let steps = 12;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|step| {
                let turn = at * (step as f32) / (steps as f32);
                centre + Vec2::angled(angle(turn)) * (RADIUS - 1.5)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.0, tokens.accent)));
    }
    painter.line_segment(
        [centre, centre + Vec2::angled(angle(at)) * (RADIUS - 3.0)],
        Stroke::new(1.5, tokens.accent),
    );
    // Its name and its value, under it.
    painter.text(
        pos2(rect.center().x, rect.top() + CENTRE_Y + RADIUS + 3.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(9.0),
        tokens.text_weak,
    );
    painter.text(
        pos2(rect.center().x, rect.top() + CENTRE_Y + RADIUS + 14.0),
        Align2::CENTER_TOP,
        value_text(*value, unit),
        FontId::proportional(9.0),
        tokens.text,
    );
    edit
}

/// A value as the knob shows it: two decimals (frequencies none), with its unit.
fn value_text(value: f32, unit: &str) -> String {
    if unit.trim() == "Hz" {
        format!("{value:.0}{unit}")
    } else {
        format!("{value:.2}{unit}")
    }
}

/// The knobs of one kind of string, in three groups. Returns whether any moved.
pub(super) fn sound_knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
    group(ui, "Strike", |ui| {
        changed |= round_knob(
            ui,
            &mut sound.pick_position,
            ranges::PICK_POSITION,
            "Pick position",
            "",
            0.2,
            false,
            "Where the pick strikes, as a share of the string from the bridge: near the \
             bridge is thin and twangy, toward the middle round and hollow.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.hardness,
            ranges::HARDNESS,
            "Hardness",
            "",
            0.9,
            false,
            "How hard the pick is: a soft thumb (0) to a hard pick (1). Harder strikes are \
             brighter.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.softness,
            ranges::SOFTNESS,
            "Roundness",
            "",
            0.0,
            false,
            "The shape of the strike: a sharp snap (0), bright and buzzy, to a rounded \
             pluck (1), soft. Lower notes sound less synthetic when it is rounder.",
        )
        .changed;
    });
    group(ui, "String", |ui| {
        changed |= round_knob(
            ui,
            &mut sound.sustain,
            ranges::SUSTAIN,
            "Sustain",
            " s",
            9.0,
            true,
            "How long a ringing note takes to die away (60 dB) at the low A string's pitch.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.sustain_exponent,
            ranges::SUSTAIN_EXPONENT,
            "Shorter when high",
            "",
            0.2,
            false,
            "How much shorter higher notes ring: 0 the same as low ones, 1 much shorter.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.brilliance,
            ranges::BRILLIANCE,
            "Brilliance",
            " s",
            1.8,
            true,
            "How long the high overtones (around 2 kHz) ring, on every string alike: short \
             is mellow, long is bright and metallic.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.stiffness,
            ranges::STIFFNESS,
            "Stiffness",
            "",
            -0.5,
            false,
            "A stiff string's high overtones run sharp: closer to -0.6 is more metallic and \
             bell-like, 0 is an ideal string.",
        )
        .changed;
        let mut stages = f32::from(sound.stiffness_stages);
        let limits = (
            ranges::STIFFNESS_STAGES.0 as f32,
            ranges::STIFFNESS_STAGES.1 as f32,
        );
        if round_knob(
            ui,
            &mut stages,
            limits,
            "Stiffness strength",
            "",
            3.0,
            false,
            "How many times the stiffness is applied (0 to 4).",
        )
        .changed
        {
            sound.stiffness_stages = stages.round().clamp(limits.0, limits.1) as u8;
            changed = true;
        }
        changed |= round_knob(
            ui,
            &mut sound.second,
            ranges::SECOND,
            "Second vibration",
            "",
            0.5,
            false,
            "A real string also vibrates in a second direction that decays faster; together \
             they beat and give a long note its living, two-stage decay. 0 is one perfectly \
             even vibration (more synthetic).",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.detune,
            ranges::DETUNE,
            "Detune",
            " ct",
            1.5,
            false,
            "How far the second vibration is detuned: the speed of the beating.",
        )
        .changed;
    });
    group(ui, "Tone", |ui| {
        changed |= round_knob(
            ui,
            &mut sound.bridge,
            ranges::BRIDGE,
            "Bridge",
            "",
            0.85,
            false,
            "How much of what is heard is the force the string puts on the bridge (or a \
             magnetic pickup's signal) rather than its swing: 0 full and round, 1 lean in \
             the lows like a real guitar. Takes away the synth-bass character of low notes.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.low_cut,
            ranges::LOW_CUT,
            "Low cut",
            " Hz",
            110.0,
            true,
            "The body or pickup barely passes the lowest frequencies: below this they are \
             rolled off.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.tone,
            ranges::TONE,
            "Tone",
            " Hz",
            16_000.0,
            true,
            "The pickup's or body's tone: overtones above this frequency are rolled off.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.resonance,
            ranges::RESONANCE,
            "Resonance",
            "",
            1.8,
            false,
            "A peak at the tone frequency, like a pickup's resonance: 0.5 flat, 4 a strong \
             nasal peak.",
        )
        .changed;
        changed |= round_knob(
            ui,
            &mut sound.level,
            ranges::LEVEL,
            "Level",
            "",
            0.4,
            false,
            "How loud these strings are against the rest of the mix.",
        )
        .changed;
    });
    changed
}

/// A named row of knobs.
fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title).strong());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        add(ui);
    });
    ui.add_space(2.0);
}
