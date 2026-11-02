//! Mixer widgets for a track row: a pan knob and the geometry the volume slider, mute and
//! solo buttons share. (The volume slider itself is egui's slider, placed by the row.)

use egui::{Rect, Sense, Stroke, Vec2, pos2};

/// The x range the mixer controls get in a track row.
pub(super) fn controls_rect(row: Rect) -> Rect {
    Rect::from_min_max(
        pos2(row.right() - 182.0, row.top()),
        pos2(row.right() - 18.0, row.bottom()),
    )
}

/// What a pan knob asks for: a new value (and whether it continues the current drag), and
/// whether a drag boundary was crossed.
pub(super) struct PanKnob {
    /// The new pan, and whether the command continues the current drag.
    pub change: Option<(u8, bool)>,
    /// A drag started or ended: the next command starts a new undo entry.
    pub boundary: bool,
}

/// Draws a pan knob; a double click centers the pan.
pub(super) fn pan_knob(ui: &mut egui::Ui, rect: Rect, pan: u8, index: usize) -> PanKnob {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("pan", index)),
        Sense::click_and_drag(),
    );
    let boundary = response.drag_started() || response.drag_stopped();
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label("Pan");
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
