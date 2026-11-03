//! Mixer widgets for a track row: a pan knob and the geometry the volume slider, mute and
//! solo buttons share. (The volume slider itself is egui's slider, placed by the row.)

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Vec2, pos2};

/// The x range the mixer controls get in a track row.
pub(super) fn controls_rect(row: Rect) -> Rect {
    Rect::from_min_max(
        pos2(row.right() - 224.0, row.top()),
        pos2(row.right() - 18.0, row.bottom()),
    )
}

/// The volume slider's slot in a row.
pub(super) fn volume_rect(row: Rect) -> Rect {
    let controls = controls_rect(row);
    Rect::from_min_size(
        pos2(controls.left() + 46.0, row.center().y - 8.0),
        Vec2::new(80.0, 16.0),
    )
}

/// The decibel readout between the volume slider and the pan knob.
fn db_rect(row: Rect) -> Rect {
    let controls = controls_rect(row);
    Rect::from_min_size(
        pos2(controls.left() + 132.0, row.center().y - 8.0),
        Vec2::new(48.0, 16.0),
    )
}

/// The pan knob's slot in a row.
pub(super) fn pan_rect(row: Rect) -> Rect {
    let controls = controls_rect(row);
    Rect::from_center_size(
        pos2(controls.left() + 192.0, row.center().y),
        Vec2::splat(16.0),
    )
}

/// The small activity indicator to the right of the pan knob.
pub(super) fn activity_rect(row: Rect) -> Rect {
    Rect::from_center_size(
        pos2(row.right() - 13.0, row.center().y),
        Vec2::new(6.0, 14.0),
    )
}

/// Paints a small green activity bar (the compact form of the mixer meter).
pub(super) fn paint_activity(ui: &egui::Ui, rect: Rect, level: f32) {
    let visuals = ui.visuals();
    ui.painter()
        .rect_filled(rect, 1.0, visuals.extreme_bg_color);
    let level = level.clamp(0.0, 1.0);
    if level <= 0.0 {
        return;
    }
    let filled = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - rect.height() * level),
        rect.max,
    );
    ui.painter()
        .rect_filled(filled, 1.0, Color32::from_rgb(0x43, 0xA0, 0x47));
}

/// The volume at 0 dB; the fader top (+12 dB) reaches MIDI volume 127.
pub(super) const UNITY_VOLUME: f32 = 32.0;
/// The bottom of the fader/meter scale.
pub(super) const FLOOR_DB: f32 = -60.0;

/// The gain in dB at a MIDI volume: 0 dB at [`UNITY_VOLUME`], +12 dB at 127.
pub(super) fn db_value(volume: u8) -> f32 {
    if volume == 0 {
        return f32::NEG_INFINITY;
    }
    20.0 * (f32::from(volume) / UNITY_VOLUME).log10()
}

/// The MIDI volume for a gain in dB (clamped to 0..127).
pub(super) fn volume_from_db(db: f32) -> u8 {
    if !db.is_finite() || db <= FLOOR_DB {
        return 0;
    }
    let volume = UNITY_VOLUME * 10.0_f32.powf(db / 20.0);
    volume.round().clamp(0.0, 127.0) as u8
}

/// The volume as a signed decibel string ("+12.0 dB", "0.0 dB", "−∞ dB").
pub(super) fn db_text(volume: u8) -> String {
    if volume == 0 {
        return "−∞ dB".to_owned();
    }
    let db = db_value(volume);
    if db > 0.05 {
        format!("+{db:.1} dB")
    } else {
        format!("{db:.1} dB")
    }
}

/// Paints the volume in decibels beside its slider; highlighted while the slider is dragged.
pub(super) fn paint_db(ui: &egui::Ui, row: Rect, volume: u8, active: bool) {
    let rect = db_rect(row);
    let visuals = ui.visuals();
    if active {
        ui.painter()
            .rect_filled(rect, 3.0, visuals.selection.bg_fill.gamma_multiply(0.25));
    }
    let color = if active {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    ui.painter().text(
        rect.right_center(),
        Align2::RIGHT_CENTER,
        db_text(volume),
        FontId::proportional(10.0),
        color,
    );
}

/// What a pan knob asks for: a new value (and whether it continues the current drag), and
/// whether a drag boundary was crossed.
pub(in crate::app) struct PanKnob {
    /// The new pan, and whether the command continues the current drag.
    pub(in crate::app) change: Option<(u8, bool)>,
    /// A drag started or ended: the next command starts a new undo entry.
    pub(in crate::app) boundary: bool,
}

/// Draws a pan knob named `label` for screen readers; a double click centers the pan.
pub(in crate::app) fn pan_knob(
    ui: &mut egui::Ui,
    rect: Rect,
    pan: u8,
    index: usize,
    label: &str,
) -> PanKnob {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("pan", index)),
        Sense::click_and_drag(),
    );
    let boundary = response.drag_started() || response.drag_stopped();
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Slider);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(127.0);
        node.set_numeric_value(f64::from(pan));
    });
    response
        .clone()
        .on_hover_text(format!("Pan {} — drag left or right", pan_label(pan)));
    if response.double_clicked() {
        return PanKnob {
            change: Some((64, false)),
            boundary: true,
        };
    }
    let mut changed = None;
    if response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().x);
        let step = (delta / 2.0).round() as i32;
        let value = i32::from(pan).saturating_add(step).clamp(0, 127);
        if let Ok(value) = u8::try_from(value)
            && value != pan
        {
            changed = Some((value, !response.drag_started()));
        }
    }
    let center = rect.center();
    let radius = rect.width() * 0.5 - 1.0;
    let visuals = ui.visuals();
    ui.painter()
        .circle_filled(center, radius, visuals.faint_bg_color);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, visuals.weak_text_color()));
    // -135 degrees (left) to +135 degrees (right), with 64 pointing up.
    let angle = (f32::from(pan) / 127.0).mul_add(270.0, -135.0).to_radians();
    let tip = center + Vec2::angled(angle - std::f32::consts::FRAC_PI_2) * (radius - 2.0);
    ui.painter()
        .line_segment([center, tip], Stroke::new(1.5, visuals.selection.bg_fill));
    PanKnob {
        change: changed,
        boundary,
    }
}

/// "C" (center), "L40" or "R30".
fn pan_label(pan: u8) -> String {
    match i32::from(pan).saturating_sub(64) {
        0 => "C".to_owned(),
        offset if offset < 0 => format!("L{}", offset.unsigned_abs()),
        offset => format!("R{offset}"),
    }
}

#[cfg(test)]
mod tests {
    use super::db_text;

    #[test]
    fn volumes_are_shown_in_decibels_with_headroom() {
        assert_eq!(db_text(0), "−∞ dB");
        assert_eq!(db_text(32), "0.0 dB");
        assert_eq!(db_text(64), "+6.0 dB");
        assert_eq!(db_text(127), "+12.0 dB");
    }
}
