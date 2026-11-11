//! The acoustic body's row in the string sound: which body, how much of it and its size.

use egui::{RichText, vec2};
use gscore_guitar::ranges;

use super::string_sound::round_knob;
use crate::settings::{BodySetting, StringSound};

/// What each body is, for its tooltip.
fn body_tip(body: BodySetting) -> &'static str {
    match body {
        BodySetting::Unset | BodySetting::None => {
            "No body: a solid electric or a bass, the strings as the pickup hears them."
        }
        BodySetting::Dreadnought => {
            "A steel-string dreadnought: a deep air resonance near 100 Hz and a bright top."
        }
        BodySetting::Classical => "A classical guitar: a lighter top, warmer and rounder.",
    }
}

/// The body's choices and knobs; whether anything changed.
pub(super) fn body_knobs(ui: &mut egui::Ui, sound: &mut StringSound) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for body in BodySetting::CHOICES {
            let chosen = sound.body == body
                || (body == BodySetting::None && sound.body == BodySetting::Unset);
            let text = RichText::new(body.label());
            if ui
                .selectable_label(chosen, if chosen { text.strong() } else { text })
                .on_hover_text(body_tip(body))
                .clicked()
                && !chosen
            {
                sound.body = body;
                changed = true;
            }
        }
    });
    if matches!(sound.body, BodySetting::None | BodySetting::Unset) {
        return changed;
    }
    ui.add_space(8.0);
    changed |= round_knob(
        ui,
        &mut sound.body_mix,
        ranges::BODY_MIX,
        "Body",
        "",
        0.8,
        false,
        "How much of the body is heard: 0 the bare strings, 1 the whole guitar. The air \
         inside it and its wood ring along with every note.",
    )
    .changed;
    changed |= round_knob(
        ui,
        &mut sound.body_size,
        ranges::BODY_SIZE,
        "Size",
        "",
        0.5,
        false,
        "The body's size: 0 a small parlour guitar (higher, boxier), 0.5 the kind's own, \
         1 a jumbo (deeper).",
    )
    .changed;
    changed
}
