//! The mixer view of the bottom panel: one channel strip per track (and a master strip),
//! with a fader, a decibel scale, an activity meter, pan and a disabled reverb knob.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{MASTER_UNITY, MasterMix, TrackIndex};

use super::mixer::{self, FLOOR_DB, UNITY_VOLUME, db_value, volume_from_db};
use super::rows::TrackRow;
use super::{GridAction, ROW_HEIGHT};
use crate::app::instrument::Instrument;

/// One strip's width.
const STRIP_WIDTH: f32 = 124.0;
/// Height of the pan/reverb row at the top of a strip.
const TOP_HEIGHT: f32 = 30.0;
/// Height of the fixed block under the fader (dB, mute/solo, name, preset, color).
const BOTTOM_HEIGHT: f32 = 80.0;
/// The decibel marks beside the fader (gain) and the meter. The fader uses a logarithmic
/// taper (0 dB is about 37% below the top), so the gain labels are not equally spaced.
const GAIN_MARKS: [f32; 12] = [
    12.0, 6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -42.0, -48.0, -54.0,
];
const METER_MARKS: [f32; 11] = [
    0.0, -6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -42.0, -48.0, -54.0, -60.0,
];

/// The dB field being typed into.
#[derive(Debug)]
struct DbEdit {
    /// Track index, or `usize::MAX` for the master.
    target: usize,
    text: String,
    focused: bool,
}

/// State the mixer view keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct MixerState {
    /// The decibel field being edited, if any.
    editing_db: Option<DbEdit>,
}

/// The mixer strips, tracks then master.
pub(super) fn strips(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: MasterMix,
    levels: &[f32],
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    let height = ui.available_height().max(ROW_HEIGHT * 4.0);
    let all_muted = !rows.is_empty() && rows.iter().all(|row| row.muted);
    let all_soloed = !rows.is_empty() && rows.iter().all(|row| row.soloed);
    egui::ScrollArea::horizontal()
        .id_salt("mixer_strips")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (index, row) in rows.iter().enumerate() {
                    track_strip(
                        ui,
                        index,
                        row,
                        levels.get(index).copied().unwrap_or(0.0),
                        height,
                        state,
                        actions,
                    );
                }
                let master_level = levels.iter().copied().fold(0.0_f32, f32::max);
                master_strip(
                    ui,
                    master,
                    master_level,
                    height,
                    rows.len(),
                    all_muted,
                    all_soloed,
                    state,
                    actions,
                );
            });
        });
}

fn track_strip(
    ui: &mut egui::Ui,
    index: usize,
    row: &TrackRow,
    level: f32,
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(STRIP_WIDTH, height), Sense::hover());
    let visuals = ui.visuals().clone();
    if index % 2 == 1 {
        ui.painter().rect_filled(rect, 0.0, visuals.faint_bg_color);
    }
    ui.painter().line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (top, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    knobs(&mut strip, top, index, row.pan, &row.name, actions);
    fader(
        &mut strip,
        fader_area,
        index,
        row.volume,
        row.pan,
        level,
        &format!("Volume {}", row.name),
        |volume, merge| {
            actions.push(GridAction::Command {
                command: Command::SetTrackVolume {
                    track: TrackIndex::new(index),
                    volume,
                },
                merge,
            });
        },
    );
    let db_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top()),
        vec2(bottom.width(), 16.0),
    );
    db_field(
        &mut strip,
        db_rect,
        state,
        index,
        row.volume,
        &format!("Level {}", row.name),
        |volume| {
            actions.push(GridAction::Command {
                command: Command::SetTrackVolume {
                    track: TrackIndex::new(index),
                    volume,
                },
                merge: false,
            });
        },
    );
    let buttons = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 18.0),
        vec2(bottom.width(), 20.0),
    );
    mute_solo(&mut strip, buttons, index, row, actions);
    let (red, green, blue) = row.color.parts();
    labels(
        &mut strip,
        bottom,
        &row.name,
        &row.preset,
        row.instrument,
        Color32::from_rgb(red, green, blue),
    );
}

#[allow(clippy::too_many_arguments)]
fn master_strip(
    ui: &mut egui::Ui,
    master: MasterMix,
    level: f32,
    height: f32,
    track_count: usize,
    all_muted: bool,
    all_soloed: bool,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(STRIP_WIDTH, height), Sense::hover());
    let visuals = ui.visuals().clone();
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (top, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    knobs(&mut strip, top, usize::MAX, master.pan, "Master", actions);
    let scaled = level * f32::from(master.volume) / f32::from(MASTER_UNITY);
    fader(
        &mut strip,
        fader_area,
        usize::MAX,
        master.volume,
        master.pan,
        scaled,
        "Master volume",
        |volume, merge| {
            actions.push(GridAction::Command {
                command: Command::SetMasterVolume { volume },
                merge,
            });
        },
    );
    let db_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top()),
        vec2(bottom.width(), 16.0),
    );
    db_field(
        &mut strip,
        db_rect,
        state,
        usize::MAX,
        master.volume,
        "Master level",
        |volume| {
            actions.push(GridAction::Command {
                command: Command::SetMasterVolume { volume },
                merge: false,
            });
        },
    );
    let buttons = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 18.0),
        vec2(bottom.width(), 20.0),
    );
    mute_solo_all(
        &mut strip,
        buttons,
        track_count,
        all_muted,
        all_soloed,
        actions,
    );
    labels(
        &mut strip,
        bottom,
        "Master",
        "All tracks",
        Instrument::Other,
        visuals.selection.bg_fill,
    );
}

/// The top row, fader area and fixed bottom block of a strip.
fn layout(inner: Rect) -> (Rect, Rect, Rect) {
    let top = Rect::from_min_size(inner.min, vec2(inner.width(), TOP_HEIGHT));
    let bottom = Rect::from_min_max(
        pos2(inner.left(), inner.bottom() - BOTTOM_HEIGHT),
        inner.max,
    );
    let fader = Rect::from_min_max(
        pos2(inner.left(), top.bottom() + 6.0),
        pos2(inner.right(), bottom.top() - 6.0),
    );
    (top, fader, bottom)
}

fn knobs(
    ui: &mut egui::Ui,
    area: Rect,
    index: usize,
    pan: u8,
    name: &str,
    actions: &mut Vec<GridAction>,
) {
    let start = area.center().x - 25.0;
    let pan_rect = Rect::from_min_size(pos2(start, area.center().y - 10.0), vec2(20.0, 20.0));
    let knob = mixer::pan_knob(ui, pan_rect, pan, index, &format!("Pan {name}"));
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((pan, merge)) = knob.change {
        if index == usize::MAX {
            actions.push(GridAction::Command {
                command: Command::SetMasterPan { pan },
                merge,
            });
        } else {
            actions.push(GridAction::Command {
                command: Command::SetTrackPan {
                    track: TrackIndex::new(index),
                    pan,
                },
                merge,
            });
        }
    }
    // Reverb is not applied by the current playback engine: draw it greyed out.
    let reverb = Rect::from_min_size(
        pos2(pan_rect.right() + 10.0, area.center().y - 10.0),
        vec2(20.0, 20.0),
    );
    let response = ui.interact(
        reverb,
        ui.scope_id().with(("reverb", index)),
        Sense::hover(),
    );
    let visuals = ui.visuals();
    ui.painter()
        .circle_filled(reverb.center(), 9.0, visuals.faint_bg_color);
    ui.painter().circle_stroke(
        reverb.center(),
        9.0,
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.5)),
    );
    ui.painter().text(
        reverb.center(),
        Align2::CENTER_CENTER,
        "R",
        FontId::proportional(9.0),
        visuals.weak_text_color().gamma_multiply(0.7),
    );
    response.on_hover_text("Reverb is not applied by playback yet");
}

#[allow(clippy::too_many_arguments)]
fn fader(
    ui: &mut egui::Ui,
    area: Rect,
    index: usize,
    value: u8,
    pan: u8,
    level: f32,
    label: &str,
    mut on_change: impl FnMut(u8, bool),
) {
    let gain_scale = Rect::from_min_size(pos2(area.left(), area.top()), vec2(28.0, area.height()));
    let fader_rect = Rect::from_min_size(
        pos2(area.left() + 30.0, area.top()),
        vec2(30.0, area.height()),
    );
    let meter = Rect::from_min_size(
        pos2(area.left() + 64.0, area.top()),
        vec2(14.0, area.height()),
    );
    let meter_scale = Rect::from_min_size(
        pos2(area.left() + 80.0, area.top()),
        vec2(28.0, area.height()),
    );
    let response = ui.interact(
        fader_rect.expand2(vec2(7.0, 0.0)),
        ui.scope_id().with(("fader", index)),
        Sense::click_and_drag(),
    );
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(value), label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Slider);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(127.0);
        node.set_numeric_value(f64::from(value));
    });
    let response = response.on_hover_text(format!("{label}: {value} ({})", mixer::db_text(value)));
    if response.clicked() {
        response.request_focus();
    }
    let mut new = value;
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.clicked() || response.dragged())
    {
        let fraction = ((area.bottom() - pointer.y) / area.height()).clamp(0.0, 1.0);
        let candidate = volume_at_fraction(fraction);
        if candidate != value {
            let merge = response.dragged() && !response.drag_started();
            on_change(candidate, merge);
        }
        new = candidate;
    }
    if response.has_focus() {
        let step = if ui.input(|input| input.modifiers.shift) {
            10
        } else {
            1
        };
        let mut keyboard = value;
        let mut moved = false;
        if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
            keyboard = value.saturating_add(step);
            moved = true;
        } else if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
            keyboard = value.saturating_sub(step);
            moved = true;
        } else if ui.input(|input| input.key_pressed(egui::Key::PageUp)) {
            keyboard = value.saturating_add(16);
            moved = true;
        } else if ui.input(|input| input.key_pressed(egui::Key::PageDown)) {
            keyboard = value.saturating_sub(16);
            moved = true;
        } else if ui.input(|input| input.key_pressed(egui::Key::Home)) {
            keyboard = 0;
            moved = true;
        } else if ui.input(|input| input.key_pressed(egui::Key::End)) {
            keyboard = 127;
            moved = true;
        }
        if moved && keyboard != value {
            on_change(keyboard.min(127), false);
            new = keyboard.min(127);
        }
    }
    let visuals = ui.visuals();
    let rail = Rect::from_center_size(
        pos2(fader_rect.center().x, area.center().y),
        vec2(2.0, area.height()),
    );
    ui.painter()
        .rect_filled(rail, 0.0, visuals.weak_text_color().gamma_multiply(0.6));
    let handle_y = area.bottom() - area.height() * fader_fraction(new);
    let handle = Rect::from_center_size(pos2(rail.center().x, handle_y), vec2(22.0, 12.0));
    ui.painter()
        .rect_filled(handle, 2.0, Color32::from_rgb(0xE8, 0xE8, 0xE8));
    let outline = if response.has_focus() {
        Stroke::new(1.5, visuals.selection.bg_fill)
    } else {
        Stroke::new(1.0, visuals.weak_text_color())
    };
    ui.painter()
        .rect_stroke(handle, 2.0, outline, egui::StrokeKind::Inside);
    ui.painter().line_segment(
        [
            pos2(handle.left() + 6.0, handle.center().y),
            pos2(handle.right() - 6.0, handle.center().y),
        ],
        Stroke::new(1.0, Color32::from_rgb(0x30, 0x30, 0x30)),
    );
    paint_gain_scale(ui, gain_scale, area);
    paint_meter_scale(ui, meter_scale, area);
    paint_meter(ui, meter, level, pan);
    let meter_response = ui.interact(meter, ui.scope_id().with(("meter", index)), Sense::hover());
    meter_response.on_hover_text("Estimated output level, left and right");
}

/// A green/yellow/red level segment. `fill` is 0 (bottom) to 1 (top).
fn paint_segment(ui: &egui::Ui, rect: Rect, fill: f32) {
    let visuals = ui.visuals();
    ui.painter()
        .rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let fill = fill.clamp(0.0, 1.0);
    if fill <= 0.0 {
        return;
    }
    let filled = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - rect.height() * fill),
        rect.max,
    );
    let color = if fill > 0.9 {
        Color32::from_rgb(0xF2, 0x55, 0x4D)
    } else if fill > 0.72 {
        Color32::from_rgb(0xFD, 0xD8, 0x35)
    } else {
        Color32::from_rgb(0x43, 0xA0, 0x47)
    };
    ui.painter().rect_filled(filled, 1.0, color);
}

/// Left and right meter segments; the pan sets their balance.
fn paint_meter(ui: &egui::Ui, rect: Rect, level: f32, pan: u8) {
    let gap = 2.0;
    let width = (rect.width() - gap) / 2.0;
    let left = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let right = Rect::from_min_size(
        pos2(left.right() + gap, rect.top()),
        vec2(width, rect.height()),
    );
    let balance = f32::from(pan) / 127.0;
    let left_level = level * (1.0 - balance).sqrt();
    let right_level = level * balance.sqrt();
    paint_segment(ui, left, meter_fraction(left_level));
    paint_segment(ui, right, meter_fraction(right_level));
}

/// The gain scale left of the fader (+12 to -54 dB, then silence).
fn paint_gain_scale(ui: &egui::Ui, rect: Rect, area: Rect) {
    let visuals = ui.visuals();
    let weak = visuals.weak_text_color();
    let font = FontId::monospace(8.0);
    for db in GAIN_MARKS {
        let y = area.bottom() - area.height() * gain_fraction(db);
        ui.painter().line_segment(
            [pos2(rect.right() - 4.0, y), pos2(rect.right(), y)],
            Stroke::new(1.0, weak.gamma_multiply(0.6)),
        );
        ui.painter().text(
            pos2(rect.right() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{db:.0}"),
            font.clone(),
            weak,
        );
    }
    ui.painter().text(
        pos2(rect.right() - 6.0, area.bottom()),
        Align2::RIGHT_BOTTOM,
        "−∞",
        font,
        weak,
    );
}

/// The meter scale right of the meter (0 to -60 dBFS).
fn paint_meter_scale(ui: &egui::Ui, rect: Rect, area: Rect) {
    let visuals = ui.visuals();
    let weak = visuals.weak_text_color();
    let font = FontId::monospace(8.0);
    for db in METER_MARKS {
        let y = area.bottom() - area.height() * meter_db_fraction(db);
        ui.painter().line_segment(
            [pos2(rect.left(), y), pos2(rect.left() + 3.0, y)],
            Stroke::new(1.0, weak.gamma_multiply(0.6)),
        );
        ui.painter().text(
            pos2(rect.left() + 5.0, y),
            Align2::LEFT_CENTER,
            format!("{db:.0}"),
            font.clone(),
            weak,
        );
    }
    ui.painter().text(
        pos2(rect.right(), area.top()),
        Align2::RIGHT_TOP,
        "dB",
        FontId::monospace(7.0),
        weak,
    );
}

/// The editable decibel readout; click it to type a level.
fn db_field(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut MixerState,
    target: usize,
    volume: u8,
    label: &str,
    mut on_change: impl FnMut(u8),
) {
    if let Some(edit) = &mut state.editing_db
        && edit.target == target
    {
        let response = ui.put(
            rect,
            egui::TextEdit::singleline(&mut edit.text)
                .font(FontId::monospace(9.0))
                .desired_width(rect.width()),
        );
        let accessible = format!("{label} in decibels");
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label(&accessible);
        });
        if !edit.focused {
            response.request_focus();
            edit.focused = true;
        }
        let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        if escape {
            state.editing_db = None;
        } else if enter || response.lost_focus() {
            let text = edit.text.trim().replace(',', ".");
            let db = if text.is_empty() || text.eq_ignore_ascii_case("-inf") || text.contains('∞')
            {
                Some(-60.0)
            } else {
                text.parse::<f32>()
                    .ok()
                    .filter(|db| db.is_finite())
                    .map(|db| db.clamp(FLOOR_DB, 12.0))
            };
            if let Some(db) = db {
                let new = volume_from_db(db);
                if new != volume {
                    on_change(new);
                }
            }
            state.editing_db = None;
        }
        return;
    }
    let text = mixer::db_text(volume);
    let response = ui.put(
        rect,
        egui::Button::new(RichText::new(text).size(9.0)).frame(false),
    );
    let accessible = format!("{label} in decibels");
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(&accessible);
    });
    let response = response.on_hover_text("Click to type a level in dB");
    if response.clicked() {
        state.editing_db = Some(DbEdit {
            target,
            text: if volume == 0 {
                "-60".to_owned()
            } else {
                format!("{:.1}", db_value(volume))
            },
            focused: false,
        });
    }
}

fn mute_solo(
    ui: &mut egui::Ui,
    rect: Rect,
    index: usize,
    row: &TrackRow,
    actions: &mut Vec<GridAction>,
) {
    let width = (rect.width() - 4.0) / 2.0;
    let mute_rect = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let solo_rect = Rect::from_min_size(
        pos2(rect.left() + width + 4.0, rect.top()),
        vec2(width, rect.height()),
    );
    let mute = ui.put(
        mute_rect,
        egui::Button::new("M").selected(row.muted).small(),
    );
    ui.ctx().accesskit_node_builder(mute.id, |node| {
        node.set_label(format!("Mute {}", row.name));
    });
    if mute.on_hover_text("Mute this track").clicked() {
        actions.push(GridAction::Command {
            command: Command::SetTrackMute {
                track: TrackIndex::new(index),
                mute: !row.muted,
            },
            merge: false,
        });
    }
    let solo = ui.put(
        solo_rect,
        egui::Button::new("S").selected(row.soloed).small(),
    );
    ui.ctx().accesskit_node_builder(solo.id, |node| {
        node.set_label(format!("Solo {}", row.name));
    });
    if solo.on_hover_text("Solo this track").clicked() {
        actions.push(GridAction::Command {
            command: Command::SetTrackSolo {
                track: TrackIndex::new(index),
                solo: !row.soloed,
            },
            merge: false,
        });
    }
}

fn mute_solo_all(
    ui: &mut egui::Ui,
    rect: Rect,
    track_count: usize,
    all_muted: bool,
    all_soloed: bool,
    actions: &mut Vec<GridAction>,
) {
    if track_count == 0 {
        return;
    }
    let width = (rect.width() - 4.0) / 2.0;
    let mute_rect = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let solo_rect = Rect::from_min_size(
        pos2(rect.left() + width + 4.0, rect.top()),
        vec2(width, rect.height()),
    );
    let mute = ui.put(
        mute_rect,
        egui::Button::new("M").selected(all_muted).small(),
    );
    ui.ctx().accesskit_node_builder(mute.id, |node| {
        node.set_label("Mute all");
    });
    if mute.on_hover_text("Mute all tracks").clicked() {
        let mute = !all_muted;
        actions.push(GridAction::Command {
            command: Command::Batch {
                commands: (0..track_count)
                    .map(|index| Command::SetTrackMute {
                        track: TrackIndex::new(index),
                        mute,
                    })
                    .collect(),
            },
            merge: false,
        });
    }
    let solo = ui.put(
        solo_rect,
        egui::Button::new("S").selected(all_soloed).small(),
    );
    ui.ctx().accesskit_node_builder(solo.id, |node| {
        node.set_label("Solo all");
    });
    if solo.on_hover_text("Solo all tracks").clicked() {
        let solo = !all_soloed;
        actions.push(GridAction::Command {
            command: Command::Batch {
                commands: (0..track_count)
                    .map(|index| Command::SetTrackSolo {
                        track: TrackIndex::new(index),
                        solo,
                    })
                    .collect(),
            },
            merge: false,
        });
    }
}

fn labels(
    ui: &mut egui::Ui,
    bottom: Rect,
    name: &str,
    preset: &str,
    instrument: Instrument,
    color: Color32,
) {
    let visuals = ui.visuals().clone();
    let name_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 42.0),
        vec2(bottom.width(), 14.0),
    );
    ui.painter().with_clip_rect(name_rect).text(
        pos2(name_rect.left(), name_rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(10.0),
        visuals.strong_text_color(),
    );
    let name_response = ui.interact(name_rect, ui.scope_id().with("mixer_name"), Sense::hover());
    name_response.on_hover_text(name);

    let preset_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 58.0),
        vec2(bottom.width(), 12.0),
    );
    let icon = Rect::from_min_size(
        pos2(preset_rect.left(), preset_rect.center().y - 5.0),
        vec2(10.0, 10.0),
    );
    instrument.paint(ui.painter(), icon, visuals.weak_text_color());
    let text_rect =
        Rect::from_min_max(pos2(icon.right() + 3.0, preset_rect.top()), preset_rect.max);
    ui.painter().with_clip_rect(text_rect).text(
        pos2(text_rect.left(), text_rect.center().y),
        Align2::LEFT_CENTER,
        preset,
        FontId::proportional(8.5),
        visuals.weak_text_color(),
    );
    let preset_response = ui.interact(
        preset_rect,
        ui.scope_id().with("mixer_preset"),
        Sense::hover(),
    );
    preset_response.on_hover_text(preset);

    let color_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.bottom() - 5.0),
        vec2(bottom.width(), 5.0),
    );
    ui.painter().rect_filled(color_rect, 1.0, color);
}

/// The fader travel for a MIDI volume: a logarithmic taper, 0 at silence and 1 at +12 dB.
fn fader_fraction(volume: u8) -> f32 {
    (f32::from(volume) / 127.0).cbrt().clamp(0.0, 1.0)
}

/// The MIDI volume at a fader position (0 = silence, 1 = +12 dB).
fn volume_at_fraction(fraction: f32) -> u8 {
    let volume = 127.0 * fraction.clamp(0.0, 1.0).powi(3);
    volume.round().clamp(0.0, 127.0) as u8
}

/// Where a gain `db` sits on the fader's logarithmic taper.
fn gain_fraction(db: f32) -> f32 {
    if !db.is_finite() || db <= FLOOR_DB {
        return 0.0;
    }
    let volume = UNITY_VOLUME * 10.0_f32.powf(db / 20.0);
    (volume / 127.0).cbrt().clamp(0.0, 1.0)
}

/// Where a meter level `db` sits on the dBFS scale (linear in decibels).
fn meter_db_fraction(db: f32) -> f32 {
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// The meter fill for a linear 0..1 estimate, in dBFS.
fn meter_fraction(level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    meter_db_fraction(20.0 * level.log10())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{db_value, volume_from_db};

    #[test]
    fn decibels_map_to_midi_volume_with_headroom() {
        assert_eq!(volume_from_db(-60.0), 0);
        assert_eq!(volume_from_db(0.0), 32);
        assert_eq!(volume_from_db(12.0), 127);
        assert!(db_value(32).abs() < 0.1);
        assert!((db_value(127) - 12.0).abs() < 0.1);
    }
}
