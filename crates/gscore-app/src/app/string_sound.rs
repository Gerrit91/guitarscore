//! The knobs of the modelled strings and of the chain's blocks, as the Sound chain window
//! shows them: round knobs with their name and value under them, the details on hover.

use egui::{Align2, FontId, RichText, Sense, Stroke, Vec2, pos2, vec2};
use gscore_guitar::ranges;
use gscore_play::StringKind;

use crate::app::style;
use crate::settings::{PickupSetting, StringSound};

/// The space one knob takes: the knob, its name and its value.
pub(super) const CELL: Vec2 = vec2(78.0, 60.0);
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
    let (rect, _) = ui.allocate_exact_size(CELL, Sense::hover());
    // The knob itself (its circle and name) and, under it, its value: two separate
    // targets, so clicking the value opens the field instead of turning the knob.
    let knob_area = egui::Rect::from_min_max(
        rect.min,
        pos2(rect.right(), rect.top() + CENTRE_Y + RADIUS + 12.0),
    );
    let value_rect = egui::Rect::from_center_size(
        pos2(rect.center().x, rect.top() + CENTRE_Y + RADIUS + 18.0),
        vec2(CELL.x - 6.0, 13.0),
    );
    let response = ui.interact(
        knob_area,
        ui.scope_id().with(("knob", label)),
        Sense::click_and_drag(),
    );
    let hover = format!("{label}: {}{unit}\n{tip}", value_text(*value, unit));
    let response = response.on_hover_text(hover);
    if response.clicked() && !response.double_clicked() {
        response.request_focus();
    }
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
    // Its name under it, and its value: clicking the value opens a field to type one.
    painter.text(
        pos2(rect.center().x, rect.top() + CENTRE_Y + RADIUS + 3.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(9.0),
        tokens.text_weak,
    );
    let edit_id = response.id.with("typed_value");
    // Kept across frames (a temp value would be dropped at the end of each).
    let mut typing: Option<(String, bool)> = ui.ctx().data_mut(|data| data.get_persisted(edit_id));
    if let Some((text, focused)) = typing.as_mut() {
        let field = ui.put(
            value_rect,
            egui::TextEdit::singleline(text)
                .font(FontId::proportional(9.0))
                .align(Align2::CENTER_CENTER)
                .margin(egui::Margin::symmetric(1, 0)),
        );
        ui.ctx().accesskit_node_builder(field.id, |node| {
            node.set_label(format!("{label} value"));
        });
        let had_focus = *focused;
        if !had_focus {
            field.request_focus();
            *focused = true;
        }
        let done = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if cancel {
            typing = None;
        } else if done || (had_focus && field.lost_focus()) {
            if let Ok(number) = text.trim().replace(',', ".").parse::<f32>() {
                let new = number.clamp(low, high);
                if (new - *value).abs() > f32::EPSILON {
                    *value = new;
                    edit.changed = true;
                }
            }
            typing = None;
        }
    } else {
        let value_response = ui
            .interact(value_rect, response.id.with("value"), Sense::click())
            .on_hover_text("Click to type a value");
        ui.ctx().accesskit_node_builder(value_response.id, |node| {
            node.set_label(format!("{label} value"));
        });
        ui.painter().text(
            pos2(rect.center().x, rect.top() + CENTRE_Y + RADIUS + 14.0),
            Align2::CENTER_TOP,
            value_text(*value, unit),
            FontId::proportional(9.0),
            tokens.text,
        );
        if value_response.clicked() {
            typing = Some((format!("{value:.2}"), false));
        }
    }
    if let Some(typing) = typing {
        ui.ctx()
            .data_mut(|data| data.insert_persisted(edit_id, typing));
    } else {
        ui.ctx()
            .data_mut(|data| data.remove::<(String, bool)>(edit_id));
    }
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
    group(ui, "Pickup", |ui| changed |= pickup_knobs(ui, sound));
    // Not "Tone": that is the saved tones' word in the same window.
    group(ui, "Output", |ui| {
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
            "The pickup's or body's tone: overtones above this frequency are rolled off. \
             With a pickup, its coil's resonance: about 3 kHz for a humbucker, 4-5 kHz for a \
             single coil.",
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

/// What each pickup is, for its tooltip.
fn pickup_tip(pickup: PickupSetting) -> &'static str {
    match pickup {
        PickupSetting::Unset | PickupSetting::None => {
            "No magnetic pickup: the string is heard at the bridge, as an acoustic guitar's \
             body hears it."
        }
        PickupSetting::SingleCoil => {
            "One coil: clear, bright and glassy. Sets Tone and Resonance to its coil's peak \
             (4.5 kHz)."
        }
        PickupSetting::Humbucker => {
            "Two coils side by side: fatter and darker, the classic rock sound. Sets Tone and \
             Resonance to its coils' peak (3 kHz)."
        }
    }
}

/// The pickup: which kind (choosing one sets the tone filter to its coil's resonance) and
/// where it sits; whether anything changed.
fn pickup_knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for (pickup, label) in PickupSetting::CHOICES {
            let chosen = sound.pickup == pickup
                || (pickup == PickupSetting::None && sound.pickup == PickupSetting::Unset);
            let text = RichText::new(label);
            if ui
                .selectable_label(chosen, if chosen { text.strong() } else { text })
                .on_hover_text(pickup_tip(pickup))
                .clicked()
                && !chosen
            {
                sound.pickup = pickup;
                match pickup {
                    PickupSetting::SingleCoil => (sound.tone, sound.resonance) = (4_500.0, 2.2),
                    PickupSetting::Humbucker => (sound.tone, sound.resonance) = (3_000.0, 1.8),
                    PickupSetting::None | PickupSetting::Unset => {}
                }
                changed = true;
            }
        }
    });
    if matches!(sound.pickup, PickupSetting::None | PickupSetting::Unset) {
        return changed;
    }
    ui.add_space(8.0);
    changed |= round_knob(
        ui,
        &mut sound.pickup_position,
        ranges::PICKUP_POSITION,
        "Position",
        "",
        gscore_guitar::pickup_positions::BRIDGE,
        false,
        "Where the pickup sits, as a share of the string from the bridge: close to the \
         bridge (0.09) nasal and biting, in the middle (0.17) hollow, at the neck (0.26) \
         round and warm.",
    )
    .changed;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for (label, position) in [
            ("Bridge", gscore_guitar::pickup_positions::BRIDGE),
            ("Middle", gscore_guitar::pickup_positions::MIDDLE),
            ("Neck", gscore_guitar::pickup_positions::NECK),
        ] {
            let chosen = (sound.pickup_position - position).abs() < 1e-3;
            if ui
                .selectable_label(chosen, label)
                .on_hover_text(format!("The {} pickup's place", label.to_lowercase()))
                .clicked()
            {
                sound.pickup_position = position;
                changed = true;
            }
        }
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
