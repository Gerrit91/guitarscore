//! The knobs of an amp block in the Sound chain window (its voice, chosen by clicking its
//! name, then Gain, the tone stack, the speaker and the output level as round knobs), of a
//! drive pedal, and of a compressor (its ratio buttons, its knobs and its gain-reduction
//! meter). The EQ's own editor is in [`super::eq_editor`].

use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, vec2};
use gscore_guitar::knob_ranges;

use crate::app::string_sound::round_knob;
use crate::app::style;
use crate::chain_settings::{AmpVoiceSetting, BlockSetting, CompRatioSetting};

/// What each voice is, for its tooltip.
fn voice_tip(voice: AmpVoiceSetting) -> &'static str {
    match voice {
        AmpVoiceSetting::Clean => "One stage, little gain: clean until picked hard.",
        AmpVoiceSetting::Crunch => "Two stages: the crunch of a pushed British amp.",
        AmpVoiceSetting::Lead => {
            "Three stages, pushed mids and a tight low end: high-gain lead and rhythm."
        }
    }
}

/// The amp's voice and knobs; whether anything changed.
pub(super) fn amp_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = false;
    // The voices stacked like a channel switch, left of the knobs.
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for voice in AmpVoiceSetting::ALL {
            let chosen = block.voice == voice;
            let text = RichText::new(voice.label());
            if ui
                .selectable_label(chosen, if chosen { text.strong() } else { text })
                .on_hover_text(voice_tip(voice))
                .clicked()
                && !chosen
            {
                block.voice = voice;
                changed = true;
            }
        }
    });
    ui.add_space(8.0);
    let knobs: [(&mut f32, &str, &str); 4] = [
        (
            &mut block.drive,
            "Gain",
            "How hard the preamp is driven: how much it distorts, and how long notes sustain.",
        ),
        (
            &mut block.bass,
            "Bass",
            "The lows after the preamp, ±12 dB (0.5 flat).",
        ),
        (
            &mut block.middle,
            "Mid",
            "The mids around 700 Hz, ±12 dB (0.5 flat): down scoops, up cuts through.",
        ),
        (
            &mut block.treble,
            "Treble",
            "The highs above 3 kHz, ±12 dB (0.5 flat).",
        ),
    ];
    for (value, label, tip) in knobs {
        changed |= round_knob(ui, value, knob_ranges::AMP, label, "", 0.5, false, tip).changed;
    }
    changed |= round_knob(
        ui,
        &mut block.speaker,
        knob_ranges::AMP,
        "Speaker",
        "",
        1.0,
        false,
        "The built-in 4×12 speaker: 1 through it, 0 direct. Turn it to 0 when a Cab block with \
         a cabinet's impulse response follows the amp.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.gain_db,
        knob_ranges::LEVEL,
        "Level",
        " dB",
        0.0,
        false,
        "The amp's output level.",
    )
    .changed;
    changed
}

/// A drive pedal's knobs; whether anything changed.
pub(super) fn drive_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = round_knob(
        ui,
        &mut block.drive,
        knob_ranges::AMP,
        "Drive",
        "",
        0.3,
        false,
        "How hard the pedal clips the mids and highs (the lows pass it clean). In front of a \
         lead amp keep it low and raise Level: a tight, chugging low end.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.tone,
        knob_ranges::AMP,
        "Tone",
        "",
        0.5,
        false,
        "The pedal's tone: 0 dark, 1 bright.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.gain_db,
        knob_ranges::LEVEL,
        "Level",
        " dB",
        0.0,
        false,
        "The pedal's output: how hard it hits what follows.",
    )
    .changed;
    changed
}

/// The deepest gain reduction the meter shows, in dB.
pub(super) const METER_FLOOR_DB: f32 = 20.0;

/// What each ratio is, for its tooltip.
fn ratio_tip(ratio: CompRatioSetting) -> &'static str {
    match ratio {
        CompRatioSetting::Four => "4:1, soft and early: gentle control, the most natural.",
        CompRatioSetting::Eight => "8:1: the classic all-round setting for guitar and vocals.",
        CompRatioSetting::Twelve => "12:1: firm, for a part that has to sit still.",
        CompRatioSetting::Twenty => "20:1: nearly a limiter, hard and tight.",
        CompRatioSetting::All => {
            "Every button in at once (the famous trick): harder than 20, overshooting, \
             gritty. The parallel bus's slam."
        }
    }
}

/// A compressor's ratio, input and output, attack, release and mix, and its gain-reduction
/// meter (as far down as the track's compressor turns it, `reduction` dB); whether
/// anything changed.
pub(super) fn compressor_knobs(
    ui: &mut egui::Ui,
    block: &mut BlockSetting,
    reduction: f32,
) -> bool {
    let mut changed = false;
    // The ratio buttons stacked like the amp's voices, left of the knobs.
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for ratio in CompRatioSetting::ALL {
            let chosen = block.comp_ratio == ratio;
            let text = RichText::new(ratio.label());
            if ui
                .selectable_label(chosen, if chosen { text.strong() } else { text })
                .on_hover_text(ratio_tip(ratio))
                .clicked()
                && !chosen
            {
                block.comp_ratio = ratio;
                changed = true;
            }
        }
    });
    ui.add_space(8.0);
    changed |= round_knob(
        ui,
        &mut block.input_db,
        knob_ranges::COMP_GAIN,
        "Input",
        " dB",
        0.0,
        false,
        "How hard the signal is driven into the fixed threshold: more input, more \
         compression (and more of the FET's color).",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.gain_db,
        knob_ranges::COMP_GAIN,
        "Output",
        " dB",
        0.0,
        false,
        "The make-up gain: brings back what the compression took off.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.attack_ms,
        knob_ranges::ATTACK,
        "Attack",
        " ms",
        0.3,
        true,
        "How quickly it clamps down: fast keeps the pick's click out of the way, slower \
         lets the attack through.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.release_ms,
        knob_ranges::RELEASE,
        "Release",
        " ms",
        300.0,
        true,
        "How quickly it lets go; it slows by itself after a long squeeze, so a held part \
         breathes instead of pumping.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.mix,
        knob_ranges::MIX,
        "Mix",
        "",
        1.0,
        false,
        "The share of the compressed signal: 1 all of it, 0.5 the dry signal blended back \
         in (parallel compression).",
    )
    .changed;
    gain_reduction_meter(ui, reduction);
    changed
}

/// The gain-reduction needle, as a horizontal bar: 0 dB at the left, as far down as
/// [`METER_FLOOR_DB`] at the right, filled from the left.
fn gain_reduction_meter(ui: &mut egui::Ui, reduction: f32) {
    let tokens = style::tokens(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(150.0, 14.0), Sense::hover());
    let reduction = if reduction.is_finite() {
        reduction.clamp(-METER_FLOOR_DB, 0.0)
    } else {
        0.0
    };
    ui.painter().rect_filled(rect, 3.0, tokens.panel);
    ui.painter().rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, tokens.line),
        egui::StrokeKind::Inside,
    );
    let filled = rect.width() * (-reduction / METER_FLOOR_DB);
    if filled > 0.5 {
        let bar = Rect::from_min_size(rect.min, vec2(filled, rect.height()));
        ui.painter().rect_filled(bar, 3.0, tokens.solo);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{reduction:.1} dB"),
        FontId::proportional(9.0),
        tokens.text,
    );
    response.on_hover_text(
        "The compressor's gain reduction: how far it turns the signal down right now",
    );
}

/// A chorus's knobs: its speed, its swing and, unless it runs all wet (a bus), the mix
/// under the dry sound.
pub(super) fn chorus_knobs(ui: &mut egui::Ui, block: &mut BlockSetting, all_wet: bool) -> bool {
    let mut changed = round_knob(
        ui,
        &mut block.chorus_rate,
        knob_ranges::CHORUS_RATE,
        "Rate",
        " Hz",
        0.6,
        true,
        "How fast the chorus swings: slow for a wide, dreamy drift, fast for a shiver.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut block.chorus_depth,
        knob_ranges::CHORUS_DEPTH,
        "Depth",
        " ms",
        3.0,
        false,
        "How far the delay swings: more is a wider, more obviously doubled sound.",
    )
    .changed;
    if !all_wet {
        changed |= round_knob(
            ui,
            &mut block.mix,
            knob_ranges::MIX,
            "Mix",
            "",
            0.5,
            false,
            "How much of the chorus under the dry sound: 0 none, 1 only the chorus.",
        )
        .changed;
    }
    changed
}
