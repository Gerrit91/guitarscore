//! The 4-band EQ block's editor: its frequency response with a handle per band and cut,
//! and its knobs in columns from the low cut through the bands to the high cut.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use gscore_guitar::{BandShape, knob_ranges};

use crate::app::string_sound::{CELL, round_knob};
use crate::app::style;
use crate::chain_settings::{BandShapeSetting, BlockSetting};

/// The drawn range of the EQ's curve: 20 Hz to 20 kHz across, ±18 dB up and down.
const EQ_LOW_HZ: f32 = 20.0;
const EQ_HIGH_HZ: f32 = 20_000.0;
const EQ_DB: f32 = 18.0;
/// The curve's height and how many points draw it.
const EQ_CURVE_HEIGHT: f32 = 172.0;
const EQ_CURVE_STEPS: usize = 240;
/// The radius of a handle's dot and the reach of the mouse around it.
const EQ_HANDLE: f32 = 4.5;
const EQ_HANDLE_HIT: f32 = 9.0;
/// The rate the curve is drawn at (the filters are the same at 44.1 and 48 kHz; this is
/// the range GuitarScore's output uses).
const EQ_RATE: f32 = 48_000.0;

/// The grey of the cuts' handles and curves.
const CUT_COLOR: Color32 = Color32::from_rgb(0x92, 0x9A, 0xA8);
/// The bands' colors, low to high: band 1 blue, 2 green, 3 amber, 4 pink.
const BAND_COLORS: [Color32; gscore_guitar::BANDS] = [
    Color32::from_rgb(0x5F, 0xA8, 0xFF),
    Color32::from_rgb(0x54, 0xC9, 0x8A),
    Color32::from_rgb(0xE9, 0xA8, 0x2E),
    Color32::from_rgb(0xD8, 0x73, 0xC0),
];

/// Band `band`'s color (the cuts' grey for a band out of range).
fn band_color(band: usize) -> Color32 {
    BAND_COLORS.get(band).copied().unwrap_or(CUT_COLOR)
}

/// How far `frequency` sits across the curve: 0 at 20 Hz, 1 at 20 kHz (log scale).
fn frequency_share(frequency: f32) -> f32 {
    (frequency.clamp(EQ_LOW_HZ, EQ_HIGH_HZ) / EQ_LOW_HZ).ln() / (EQ_HIGH_HZ / EQ_LOW_HZ).ln()
}

/// The frequency at `share` of the way across the curve.
fn share_frequency(share: f32) -> f32 {
    EQ_LOW_HZ * (EQ_HIGH_HZ / EQ_LOW_HZ).powf(share.clamp(0.0, 1.0))
}

/// Where `frequency` sits across the curve.
fn x_of(frequency: f32, rect: Rect) -> f32 {
    rect.left() + rect.width() * frequency_share(frequency)
}

/// The frequency under `x` on the curve.
fn frequency_at(x: f32, rect: Rect) -> f32 {
    if rect.width() <= 0.0 {
        return EQ_LOW_HZ;
    }
    share_frequency((x - rect.left()) / rect.width())
}

/// Where `gain_db` sits up or down the curve.
fn y_of(gain_db: f32, rect: Rect) -> f32 {
    rect.center().y - (gain_db / EQ_DB).clamp(-1.0, 1.0) * rect.height() / 2.0
}

/// The gain under `y` on the curve.
fn gain_at_y(y: f32, rect: Rect) -> f32 {
    if rect.height() <= 0.0 {
        return 0.0;
    }
    ((rect.center().y - y) / (rect.height() / 2.0) * EQ_DB).clamp(-EQ_DB, EQ_DB)
}

/// A frequency as the handles show it.
fn hz_text(hz: f32) -> String {
    if hz >= 1_000.0 {
        format!("{:.2} kHz", hz / 1_000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// A gain as the handles show it.
fn db_text(gain_db: f32) -> String {
    format!("{gain_db:+.1} dB")
}

/// Band `band`'s shape as the block has it (the settings file may hold a shape the band
/// cannot take; the engine fits it the same way).
fn block_shape(block: &BlockSetting, band: usize) -> BandShape {
    block
        .eq_band_shape
        .get(band)
        .copied()
        .map_or_else(|| BandShape::of(band), BandShapeSetting::shape)
}

/// The two shapes an outer band can be: its own shelf (low on band 1, high on band 4) and
/// a bell.
fn outer_shapes(band: usize) -> [BandShapeSetting; 2] {
    if band == 0 {
        [BandShapeSetting::LowShelf, BandShapeSetting::Bell]
    } else {
        [BandShapeSetting::HighShelf, BandShapeSetting::Bell]
    }
}

/// The EQ's values, as the engine takes them.
fn eq_values(block: &BlockSetting) -> gscore_guitar::EqKnobs {
    gscore_guitar::EqKnobs {
        low_cut: block.low_cut,
        high_cut: block.high_cut,
        band_freq: block.eq_band_freq,
        band_gain: block.eq_band_gain,
        band_q: block.eq_band_q,
        band_shape: std::array::from_fn(|band| block_shape(block, band)),
    }
}

/// One band's handle on the curve: drag it for frequency and gain, Shift+drag or the
/// wheel for its width (a bell's Q), double-click for the defaults.
#[allow(clippy::too_many_arguments)]
fn band_handle(
    ui: &mut egui::Ui,
    rect: Rect,
    band: usize,
    shape: BandShape,
    freq: &mut f32,
    gain: &mut f32,
    q: &mut f32,
    reset: (f32, f32),
) -> bool {
    let (reset_freq, reset_q) = reset;
    let number = band.saturating_add(1);
    let at = pos2(x_of(*freq, rect), y_of(*gain, rect));
    let response = ui.interact(
        Rect::from_center_size(at, vec2(2.0 * EQ_HANDLE_HIT, 2.0 * EQ_HANDLE_HIT)),
        ui.scope_id().with(("eq_band_handle", band)),
        Sense::click_and_drag(),
    );
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("Band {number} handle"));
        node.set_role(egui::Role::Slider);
    });
    let tip = if shape == BandShape::Bell {
        format!(
            "Band {number} ({})\n{} · {} · Q {:.2}\nDrag: frequency and gain. Shift+drag or \
             the wheel: width. Double-click: reset.",
            shape.label(),
            hz_text(*freq),
            db_text(*gain),
            *q
        )
    } else {
        format!(
            "Band {number} ({})\n{} · {}\nDrag: frequency and gain. Double-click: reset.",
            shape.label(),
            hz_text(*freq),
            db_text(*gain)
        )
    };
    let response = response.on_hover_text(tip);
    let mut changed = false;
    if response.double_clicked() {
        if (*freq - reset_freq).abs() > f32::EPSILON {
            *freq = reset_freq;
            changed = true;
        }
        if (*gain).abs() > f32::EPSILON {
            *gain = 0.0;
            changed = true;
        }
        if shape == BandShape::Bell && (*q - reset_q).abs() > f32::EPSILON {
            *q = reset_q;
            changed = true;
        }
    } else if response.dragged() {
        let shift = ui.input(|input| input.modifiers.shift);
        if shape == BandShape::Bell && shift {
            let delta = ui.input(|input| input.pointer.delta().x);
            let new =
                (*q * (delta / 80.0).exp()).clamp(knob_ranges::BAND_Q.0, knob_ranges::BAND_Q.1);
            if (new - *q).abs() > f32::EPSILON {
                *q = new;
                changed = true;
            }
        } else if let Some(pos) = response.interact_pointer_pos() {
            let new_freq = frequency_at(pos.x, rect);
            let new_gain = gain_at_y(pos.y, rect);
            if (new_freq - *freq).abs() > f32::EPSILON {
                *freq = new_freq;
                changed = true;
            }
            if (new_gain - *gain).abs() > f32::EPSILON {
                *gain = new_gain;
                changed = true;
            }
        }
    }
    if response.hovered() && shape == BandShape::Bell {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let new =
                (*q * (scroll / 200.0).exp()).clamp(knob_ranges::BAND_Q.0, knob_ranges::BAND_Q.1);
            if (new - *q).abs() > f32::EPSILON {
                *q = new;
                changed = true;
            }
        }
    }
    changed
}

/// A cut's handle: drag it left or right for its frequency, double-click for the default.
fn cut_handle(ui: &mut egui::Ui, rect: Rect, value: &mut f32, reset: f32, label: &str) -> bool {
    let at = pos2(x_of(*value, rect), y_of(0.0, rect));
    let response = ui.interact(
        Rect::from_center_size(at, vec2(2.0 * EQ_HANDLE_HIT, 2.0 * EQ_HANDLE_HIT)),
        ui.scope_id().with(("eq_cut_handle", label)),
        Sense::click_and_drag(),
    );
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("{label} handle"));
        node.set_role(egui::Role::Slider);
    });
    let response = response.on_hover_text(format!(
        "{label}\n{}\nDrag: frequency. Double-click: reset.",
        hz_text(*value)
    ));
    let mut changed = false;
    if response.double_clicked() {
        if (*value - reset).abs() > f32::EPSILON {
            *value = reset;
            changed = true;
        }
    } else if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let limits = if label == "Low cut" {
            knob_ranges::LOW_CUT
        } else {
            knob_ranges::HIGH_CUT
        };
        let new = frequency_at(pos.x, rect).clamp(limits.0, limits.1);
        if (new - *value).abs() > f32::EPSILON {
            *value = new;
            changed = true;
        }
    }
    changed
}

/// The EQ's frequency response above its knobs: the sum of every band and both cuts, each
/// band's own curve faint behind it, the 0 dB line, and a handle per band and cut. Returns
/// whether a knob moved.
fn eq_curve(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let tokens = style::tokens(ui);
    let width = ui.available_width().max(CELL.x);
    let (rect, _) = ui.allocate_exact_size(vec2(width, EQ_CURVE_HEIGHT), Sense::hover());
    let defaults = gscore_guitar::EqKnobs::default();
    let mut changed = false;
    // The handles sit where the knobs have them now; drawing follows, so a drag shows its
    // result in the same frame.
    changed |= cut_handle(ui, rect, &mut block.low_cut, defaults.low_cut, "Low cut");
    for band in 0..gscore_guitar::BANDS {
        let shape = block_shape(block, band);
        let (Some(freq), Some(gain), Some(q)) = (
            block.eq_band_freq.get_mut(band),
            block.eq_band_gain.get_mut(band),
            block.eq_band_q.get_mut(band),
        ) else {
            continue;
        };
        let reset = (
            defaults.band_freq.get(band).copied().unwrap_or(1_000.0),
            defaults.band_q.get(band).copied().unwrap_or(1.0),
        );
        changed |= band_handle(ui, rect, band, shape, freq, gain, q, reset);
    }
    changed |= cut_handle(ui, rect, &mut block.high_cut, defaults.high_cut, "High cut");

    let response = gscore_guitar::EqResponse::new(EQ_RATE, eq_values(block));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, tokens.control.gamma_multiply(0.45));
    // The grid: a line per labeled decade and every 6 dB.
    for hz in [
        20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0,
    ] {
        let x = x_of(hz, rect);
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, tokens.line.gamma_multiply(0.7)),
        );
    }
    for db in [-18.0, -12.0, -6.0, 6.0, 12.0, 18.0] {
        let y = y_of(db, rect);
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, tokens.line.gamma_multiply(0.7)),
        );
    }
    let zero = y_of(0.0, rect);
    painter.line_segment(
        [pos2(rect.left(), zero), pos2(rect.right(), zero)],
        Stroke::new(1.0, tokens.text_weak.gamma_multiply(0.6)),
    );
    for (hz, text) in [(100.0, "100"), (1_000.0, "1k"), (10_000.0, "10k")] {
        painter.text(
            pos2(x_of(hz, rect) + 3.0, rect.bottom() - 3.0),
            Align2::LEFT_BOTTOM,
            text,
            FontId::proportional(8.0),
            tokens.text_weak,
        );
    }
    for (db, text) in [(18.0, "+18"), (0.0, "0"), (-18.0, "−18")] {
        let (y, anchor) = if db > 0.0 {
            (rect.top() + 1.0, Align2::LEFT_TOP)
        } else if db < 0.0 {
            (rect.bottom() - 1.0, Align2::LEFT_BOTTOM)
        } else {
            (y_of(db, rect), Align2::LEFT_CENTER)
        };
        painter.text(
            pos2(rect.left() + 3.0, y),
            anchor,
            text,
            FontId::proportional(8.0),
            tokens.text_weak,
        );
    }
    // Each band's own curve faint behind the sum, then both cuts, then the sum itself.
    let draw = |gain_db: &dyn Fn(f32) -> f32, color: Color32, thickness: f32| {
        let points: Vec<Pos2> = (0..=EQ_CURVE_STEPS)
            .map(|step| {
                let share = step as f32 / EQ_CURVE_STEPS as f32;
                let hz = share_frequency(share);
                pos2(rect.left() + share * rect.width(), y_of(gain_db(hz), rect))
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(thickness, color)));
    };
    for band in 0..gscore_guitar::BANDS {
        draw(
            &|hz| response.band_db(band, hz),
            band_color(band).gamma_multiply(0.55),
            1.0,
        );
    }
    draw(
        &|hz| response.low_cut_db(hz),
        CUT_COLOR.gamma_multiply(0.4),
        1.0,
    );
    draw(
        &|hz| response.high_cut_db(hz),
        CUT_COLOR.gamma_multiply(0.4),
        1.0,
    );
    draw(&|hz| response.total_db(hz), tokens.text, 1.6);
    // The handles last, where the knobs now put them.
    for band in 0..gscore_guitar::BANDS {
        let (Some(freq), Some(gain)) = (
            block.eq_band_freq.get(band).copied(),
            block.eq_band_gain.get(band).copied(),
        ) else {
            continue;
        };
        let at = pos2(x_of(freq, rect), y_of(gain, rect));
        painter.circle_filled(at, EQ_HANDLE, band_color(band));
        painter.circle_stroke(at, EQ_HANDLE, Stroke::new(1.5, tokens.panel));
    }
    for value in [block.low_cut, block.high_cut] {
        let at = pos2(x_of(value, rect), zero);
        painter.circle_filled(at, EQ_HANDLE - 1.0, CUT_COLOR);
        painter.circle_stroke(at, EQ_HANDLE - 1.0, Stroke::new(1.5, tokens.panel));
    }
    changed
}

/// One titled column of the EQ's knobs, tinted in its band's color.
fn eq_column<R>(
    ui: &mut egui::Ui,
    title: &str,
    color: Color32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.vertical(|ui| {
        ui.set_width(CELL.x);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(title)
                    .font(FontId::proportional(9.0))
                    .strong()
                    .color(color),
            );
        });
        add(ui)
    })
    .inner
}

/// The 4-band EQ: its frequency response with a handle per band and cut, then the knobs in
/// columns from the low cut through the bands to the high cut. Band 1 is a low shelf and
/// band 4 a high shelf; the middle two are bells.
pub(super) fn eq_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> bool {
    let mut changed = eq_curve(ui, block);
    ui.add_space(4.0);
    let defaults = gscore_guitar::EqKnobs::default();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        changed |= eq_column(ui, "Low cut", CUT_COLOR, |ui| {
            round_knob(
                ui,
                &mut block.low_cut,
                knob_ranges::LOW_CUT,
                "Low cut",
                " Hz",
                30.0,
                true,
                "Takes away everything below this frequency: rumble, a kick's mud, a boom.",
            )
            .changed
        });
        for band in 0..gscore_guitar::BANDS {
            let shape = block_shape(block, band);
            let number = band.saturating_add(1);
            let color = band_color(band);
            let default_freq = defaults.band_freq.get(band).copied().unwrap_or(1_000.0);
            let default_q = defaults.band_q.get(band).copied().unwrap_or(1.0);
            changed |= eq_column(ui, &format!("Band {number}"), color, |ui| {
                let (Some(freq), Some(gain), Some(q)) = (
                    block.eq_band_freq.get_mut(band),
                    block.eq_band_gain.get_mut(band),
                    block.eq_band_q.get_mut(band),
                ) else {
                    return false;
                };
                let mut moved = false;
                moved |= round_knob(
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
                moved |= round_knob(
                    ui,
                    gain,
                    knob_ranges::EQ_GAIN,
                    &format!("{number} Gain"),
                    " dB",
                    0.0,
                    false,
                    &format!(
                        "Band {number} ({}): up to make this range stand out, down to take \
                         it away.",
                        shape.label()
                    ),
                )
                .changed;
                if band == 0 || band.saturating_add(1) == gscore_guitar::BANDS {
                    // An outer band is a shelf or a bell; the row is its Shape knob.
                    ui.scope(|ui| {
                        ui.spacing_mut().button_padding = vec2(2.0, 1.0);
                        ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
                        ui.horizontal(|ui| {
                            for choice in outer_shapes(band) {
                                let label = if choice == BandShapeSetting::Bell {
                                    "Bell"
                                } else {
                                    "Shelf"
                                };
                                let chosen = shape == choice.shape();
                                let text = RichText::new(label).font(FontId::proportional(8.0));
                                let text = if chosen { text } else { text.color(color) };
                                if ui
                                    .selectable_label(chosen, text)
                                    .on_hover_text(format!(
                                        "Band {number}: make it a {}",
                                        choice.label().to_lowercase()
                                    ))
                                    .clicked()
                                    && !chosen
                                {
                                    if let Some(slot) = block.eq_band_shape.get_mut(band) {
                                        *slot = choice;
                                    }
                                    moved = true;
                                }
                            }
                        });
                    });
                }
                if shape == BandShape::Bell {
                    moved |= round_knob(
                        ui,
                        q,
                        knob_ranges::BAND_Q,
                        &format!("{number} Q"),
                        "",
                        default_q,
                        false,
                        &format!(
                            "Band {number}'s width: high for a narrow notch, low for a wide \
                             lift."
                        ),
                    )
                    .changed;
                }
                moved
            });
        }
        changed |= eq_column(ui, "High cut", CUT_COLOR, |ui| {
            round_knob(
                ui,
                &mut block.high_cut,
                knob_ranges::HIGH_CUT,
                "High cut",
                " Hz",
                18_000.0,
                true,
                "Takes away everything above this frequency: fizz, hiss, a cymbal's harshness.",
            )
            .changed
        });
    });
    changed
}
