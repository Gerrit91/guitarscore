//! The knobs of an amp block in the Sound chain window (its voice, chosen by clicking its
//! name, then Gain, the tone stack, the speaker and the output level as round knobs) and
//! of a drive pedal.

use egui::{RichText, vec2};
use gscore_guitar::knob_ranges;

use crate::app::string_sound::round_knob;
use crate::chain_settings::{AmpVoiceSetting, BlockSetting};

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
