//! The knobs of an amp block in the Sound chain window (its voice, chosen by clicking its
//! name, then Gain, the tone stack, the speaker and the output level as round knobs), of a
//! drive pedal, and of a compressor (its ratio buttons, its knobs and its gain-reduction
//! meter).

use egui::{RichText, vec2};
use gscore_guitar::knob_ranges;

use crate::app::string_sound::round_knob;
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
    let tokens = crate::app::style::tokens(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(150.0, 14.0), egui::Sense::hover());
    let reduction = if reduction.is_finite() {
        reduction.clamp(-METER_FLOOR_DB, 0.0)
    } else {
        0.0
    };
    ui.painter().rect_filled(rect, 3.0, tokens.panel);
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, tokens.line),
        egui::StrokeKind::Inside,
    );
    let filled = rect.width() * (-reduction / METER_FLOOR_DB);
    if filled > 0.5 {
        let bar = egui::Rect::from_min_size(rect.min, vec2(filled, rect.height()));
        ui.painter().rect_filled(bar, 3.0, tokens.solo);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{reduction:.1} dB"),
        egui::FontId::proportional(9.0),
        tokens.text,
    );
    response.on_hover_text(
        "The compressor's gain reduction: how far it turns the signal down right now",
    );
}

/// The 4-band EQ's knobs: the low cut, each band's frequency and gain (the bells' width
/// too), and the high cut. Band 1 is a low shelf and band 4 a high shelf.
pub(super) fn eq_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = false;
    changed |= round_knob(
        ui,
        &mut block.low_cut,
        knob_ranges::LOW_CUT,
        "Low cut",
        " Hz",
        30.0,
        true,
        "Takes away everything below this frequency: rumble, a kick's mud, a boom.",
    )
    .changed;
    let default = gscore_guitar::EqKnobs::default();
    for band in 0..gscore_guitar::BANDS {
        let shape = gscore_guitar::BandShape::of(band);
        let number = band.saturating_add(1);
        let (Some(freq), Some(gain), Some(q)) = (
            block.eq_band_freq.get_mut(band),
            block.eq_band_gain.get_mut(band),
            block.eq_band_q.get_mut(band),
        ) else {
            continue;
        };
        let (default_freq, default_q) = (
            default.band_freq.get(band).copied().unwrap_or(1_000.0),
            default.band_q.get(band).copied().unwrap_or(1.0),
        );
        changed |= round_knob(
            ui,
            freq,
            knob_ranges::BAND_FREQ,
            &format!("{number} Freq"),
            " Hz",
            default_freq,
            true,
            &format!("Band {number} ({}): where it acts.", shape.label()),
        )
        .changed;
        changed |= round_knob(
            ui,
            gain,
            knob_ranges::EQ_GAIN,
            &format!("{number} Gain"),
            " dB",
            0.0,
            false,
            &format!(
                "Band {number} ({}): up to make this range stand out, down to take it away.",
                shape.label()
            ),
        )
        .changed;
        if shape == gscore_guitar::BandShape::Bell {
            changed |= round_knob(
                ui,
                q,
                knob_ranges::BAND_Q,
                &format!("{number} Q"),
                "",
                default_q,
                false,
                &format!("Band {number}'s width: high for a narrow notch, low for a wide lift."),
            )
            .changed;
        }
    }
    changed |= round_knob(
        ui,
        &mut block.high_cut,
        knob_ranges::HIGH_CUT,
        "High cut",
        " Hz",
        18_000.0,
        true,
        "Takes away everything above this frequency: fizz, hiss, a cymbal's harshness.",
    )
    .changed;
    changed
}
