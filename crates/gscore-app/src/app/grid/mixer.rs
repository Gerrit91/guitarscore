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

/// The mute button's slot in a row (track rows and the master row share it, so the
/// buttons form one column).
pub(super) fn mute_rect(row: Rect) -> Rect {
    Rect::from_center_size(
        pos2(controls_rect(row).left() + 10.0, row.center().y),
        Vec2::splat(18.0),
    )
}

/// The solo button's slot in a row, right of [`mute_rect`].
pub(super) fn solo_rect(row: Rect) -> Rect {
    Rect::from_center_size(
        pos2(controls_rect(row).left() + 32.0, row.center().y),
        Vec2::splat(18.0),
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

/// How far a color recedes when its track is not heard (muted, or unsoloed while another
/// track is soloed): dark enough to set it apart at a glance, bright enough to stay
/// recognizable as the track's own color.
const SILENT_DIM: f32 = 0.38;

/// [`color`], darkened while its track is silent, so the score and the mixer show at a
/// glance what plays and what does not.
pub(super) fn sounding_color(color: Color32, audible: bool) -> Color32 {
    if audible {
        color
    } else {
        color.gamma_multiply(SILENT_DIM)
    }
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

/// The MIDI volume at which a track plays at full level (0 dB).
pub(super) const TRACK_UNITY: u8 = 127;
/// The bottom of the meter scale, in dBFS.
pub(super) const FLOOR_DB: f32 = -60.0;

/// The gain in dB of a volume, where `unity` plays at 0 dB: [`TRACK_UNITY`] for a track,
/// [`gscore_model::MASTER_UNITY`] for the master.
///
/// The synthesizer (like the General MIDI recommendation) turns channel volume `v` into an
/// amplitude of `(v / 127)²`, so the gain is `40·log10(v / 127)`: 127 is 0 dB, 64 about
/// −12 dB, 32 about −24 dB. The master multiplies every track's volume by
/// `master / MASTER_UNITY`, which is `40·log10(master / MASTER_UNITY)` dB on top (+4.7 dB at
/// 127, until a track reaches the MIDI maximum).
pub(super) fn db_value(volume: u8, unity: u8) -> f32 {
    if volume == 0 || unity == 0 {
        return f32::NEG_INFINITY;
    }
    40.0 * (f32::from(volume) / f32::from(unity)).log10()
}

/// The volume for a gain in dB (see [`db_value`]), rounded and clamped to 0..=127.
pub(super) fn volume_from_db(db: f32, unity: u8) -> u8 {
    if db.is_nan() || db == f32::NEG_INFINITY {
        return 0;
    }
    let volume = f32::from(unity) * 10.0_f32.powf(db.min(40.0) / 40.0);
    volume.round().clamp(0.0, 127.0) as u8
}

/// The gain as text: "0.0 dB", "−12.0 dB", "+4.7 dB", "−∞ dB".
pub(super) fn db_text(volume: u8, unity: u8) -> String {
    let db = db_value(volume, unity);
    if !db.is_finite() {
        return "−∞ dB".to_owned();
    }
    if db > 0.05 {
        format!("+{db:.1} dB")
    } else if db > -0.05 {
        "0.0 dB".to_owned()
    } else {
        format!("−{:.1} dB", -db)
    }
}

/// Paints the gain in decibels beside its slider; highlighted while the slider is dragged.
pub(super) fn paint_db(ui: &egui::Ui, row: Rect, volume: u8, unity: u8, active: bool) {
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
        db_text(volume, unity),
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
    let hover = format!("Pan {} — drag left or right", pan_label(pan));
    knob(ui, rect, pan, ("pan", index), label, &hover, 64, false)
}

/// Draws a reverb or chorus send knob (0 dry to 127), filled up to its value; a double
/// click sets it to `reset`.
pub(in crate::app) fn send_knob(
    ui: &mut egui::Ui,
    rect: Rect,
    value: u8,
    id: (&'static str, usize),
    label: &str,
    what: &str,
    reset: u8,
) -> PanKnob {
    let hover = format!("{what} {value} of 127 — drag left or right (double-click: {reset})");
    knob(ui, rect, value, id, label, &hover, reset, true)
}

/// A round knob over 0-127, dragged sideways. `fill` paints an arc from the minimum up to
/// the value (for amounts); without it only the pointer shows (for pan).
#[allow(clippy::too_many_arguments)]
fn knob(
    ui: &mut egui::Ui,
    rect: Rect,
    value: u8,
    id: (&'static str, usize),
    label: &str,
    hover: &str,
    reset: u8,
    fill: bool,
) -> PanKnob {
    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click_and_drag());
    let boundary = response.drag_started() || response.drag_stopped();
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Slider);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(127.0);
        node.set_numeric_value(f64::from(value));
    });
    response.clone().on_hover_text(hover);
    if response.double_clicked() {
        return PanKnob {
            change: (value != reset).then_some((reset, false)),
            boundary: true,
        };
    }
    let mut changed = None;
    if response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().x);
        let step = (delta / 2.0).round() as i32;
        let new = i32::from(value).saturating_add(step).clamp(0, 127);
        if let Ok(new) = u8::try_from(new)
            && new != value
        {
            changed = Some((new, !response.drag_started()));
        }
    }
    let center = rect.center();
    let radius = rect.width() * 0.5 - 1.0;
    let visuals = ui.visuals();
    ui.painter()
        .circle_filled(center, radius, visuals.faint_bg_color);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, visuals.weak_text_color()));
    // -135 degrees (minimum) to +135 degrees (maximum), with 64 pointing up.
    let to_angle = |value: u8| {
        (f32::from(value) / 127.0)
            .mul_add(270.0, -135.0)
            .to_radians()
            - std::f32::consts::FRAC_PI_2
    };
    let angle = to_angle(value);
    if fill && value > 0 {
        let start = to_angle(0);
        let steps = 16;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|step| {
                let at = start + (angle - start) * (step as f32) / (steps as f32);
                center + Vec2::angled(at) * (radius - 1.0)
            })
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(2.0, visuals.selection.bg_fill),
        ));
    }
    let tip = center + Vec2::angled(angle) * (radius - 2.0);
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
    use gscore_model::MASTER_UNITY;

    use super::{TRACK_UNITY, db_text, db_value, volume_from_db};

    #[test]
    fn track_volumes_are_shown_as_the_synthesizer_plays_them() {
        assert_eq!(db_text(0, TRACK_UNITY), "−∞ dB");
        assert_eq!(db_text(127, TRACK_UNITY), "0.0 dB");
        // (64/127)² of the amplitude: about −11.9 dB.
        assert_eq!(db_text(64, TRACK_UNITY), "−11.9 dB");
        assert_eq!(db_text(32, TRACK_UNITY), "−23.9 dB");
    }

    #[test]
    fn the_master_is_unity_at_its_default_and_has_headroom() {
        assert_eq!(db_text(MASTER_UNITY, MASTER_UNITY), "0.0 dB");
        assert_eq!(db_text(127, MASTER_UNITY), "+4.7 dB");
        assert!(db_text(48, MASTER_UNITY).starts_with('−'));
    }

    #[test]
    fn decibels_map_back_to_volumes() {
        assert_eq!(volume_from_db(0.0, TRACK_UNITY), 127);
        assert_eq!(volume_from_db(-12.0, TRACK_UNITY), 64);
        assert_eq!(volume_from_db(-24.0, TRACK_UNITY), 32);
        assert_eq!(volume_from_db(6.0, TRACK_UNITY), 127, "clamped");
        assert_eq!(volume_from_db(f32::NEG_INFINITY, TRACK_UNITY), 0);
        assert_eq!(volume_from_db(-200.0, TRACK_UNITY), 0);
        assert_eq!(volume_from_db(0.0, MASTER_UNITY), MASTER_UNITY);
        for volume in 1..=127_u8 {
            assert_eq!(
                volume_from_db(db_value(volume, TRACK_UNITY), TRACK_UNITY),
                volume
            );
        }
    }
}
