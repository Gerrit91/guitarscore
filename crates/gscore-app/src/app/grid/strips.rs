//! The mixer view of the bottom panel: one channel strip per track, the reverb and
//! compressor buses and a master strip, with a fader, a decibel scale, an activity meter,
//! pan and the sends.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{MASTER_UNITY, MasterMix, TrackIndex};

use crate::chain_settings::DrumPart;
use crate::settings::{ChorusBus, CompressorBus, ReverbBus};

use super::mixer::{self, FLOOR_DB, TRACK_UNITY, db_value, volume_from_db};
use super::rows::TrackRow;
use super::{GridAction, GridView, ROW_HEIGHT};
use crate::app::sound_chain::DraggedBlock;
use crate::app::{icons, instrument, style};

/// One strip's width (four knobs across the top: pan, reverb, chorus, comp).
const STRIP_WIDTH: f32 = 140.0;
/// A drum part's strip, narrower than a track's (its fader still fits: the fixed parts of
/// the fader take 100 points).
const PART_STRIP_WIDTH: f32 = 116.0;
/// Height of the knob row at the top of a strip (pan, reverb, chorus and their labels).
const TOP_HEIGHT: f32 = 40.0;
/// A knob's diameter, and the distance between knob centres.
const KNOB_SIZE: f32 = 20.0;
const KNOB_SPACING: f32 = 32.0;
/// Height of the fixed block under the fader (dB, mute/solo, name, preset, color).
const BOTTOM_HEIGHT: f32 = 80.0;
/// The gain marks beside a fader, in dB. The fader's travel is logarithmic (see
/// [`fader_fraction`]), so they are not equally spaced; marks above a fader's top (a track
/// tops out at 0 dB, the master at +4.7 dB) are left out, and labels that would overlap
/// the one above are skipped.
const GAIN_MARKS: [f32; 12] = [
    6.0, 3.0, 0.0, -3.0, -6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -48.0, -60.0,
];
/// Least vertical distance between two gain labels, in points.
const LABEL_GAP: f32 = 14.0;
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
    /// The drum tracks whose part strips are open.
    open_parts: std::collections::BTreeSet<usize>,
}

impl MixerState {
    /// Whether the part strips of `track` are open.
    fn parts_open(&self, track: usize) -> bool {
        self.open_parts.contains(&track)
    }

    /// Opens or closes the part strips of `track`.
    fn toggle_parts(&mut self, track: usize) {
        if !self.open_parts.remove(&track) {
            self.open_parts.insert(track);
        }
    }
}

/// The mixer strips, tracks then master.
#[allow(clippy::too_many_arguments)]
pub(super) fn strips(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: MasterMix,
    buses: Buses,
    readings: &Readings,
    comp_sends: &[u8],
    parts: &[Vec<PartRow>],
    view: GridView,
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
                        comp_sends.get(index).copied().unwrap_or(0),
                        readings.tracks.get(index).copied().unwrap_or_default(),
                        height,
                        view,
                        state,
                        actions,
                    );
                    if row.drums && state.parts_open(index) {
                        for part in parts.get(index).map(Vec::as_slice).unwrap_or_default() {
                            part_strip(
                                ui,
                                index,
                                row,
                                part,
                                readings
                                    .parts
                                    .get(index)
                                    .and_then(|parts| parts.get(part.part.index()))
                                    .copied()
                                    .unwrap_or_default(),
                                height,
                                state,
                                actions,
                            );
                        }
                    }
                }
                reverb_strip(ui, buses.reverb, readings.bus, height, state, actions);
                compressor_strip(
                    ui,
                    buses.compressor,
                    readings.compressor,
                    height,
                    state,
                    actions,
                );
                chorus_strip(ui, buses.chorus, readings.chorus, height, state, actions);
                master_strip(
                    ui,
                    master,
                    readings.master,
                    height,
                    rows.len(),
                    all_muted,
                    all_soloed,
                    state,
                    actions,
                );
                metronome_strip(ui, view.metronome, height, state, actions);
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn track_strip(
    ui: &mut egui::Ui,
    index: usize,
    row: &TrackRow,
    comp_send: u8,
    level: Reading,
    height: f32,
    view: GridView,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    // The strip's background is the track's: clicking it selects the track, as the track's
    // row in the song grid does (the cursor moves there and the track panel shows it).
    let (rect, response) = ui.allocate_exact_size(vec2(STRIP_WIDTH, height), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("Track {}: {}", index.saturating_add(1), row.name));
        node.set_role(egui::Role::Button);
    });
    if response.clicked() {
        actions.push(GridAction::MoveCursor {
            track: index,
            measure: view.cursor_measure,
        });
    }
    // A block dragged from a chain board onto this strip goes into the track's chain.
    if let Some(drag) = response.dnd_release_payload::<DraggedBlock>() {
        actions.push(GridAction::PasteBlock {
            track: index,
            part: None,
            block: drag.block.clone(),
        });
    }
    let visuals = ui.visuals().clone();
    if index % 2 == 1 {
        ui.painter().rect_filled(rect, 0.0, visuals.faint_bg_color);
    }
    if index == view.shown_track {
        ui.painter()
            .rect_filled(rect, 3.0, visuals.selection.bg_fill.gamma_multiply(0.18));
    }
    if response.dnd_hover_payload::<DraggedBlock>().is_some() {
        ui.painter().rect_stroke(
            rect,
            3.0,
            Stroke::new(2.0, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (top, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    knobs(
        &mut strip,
        top,
        index,
        row.pan,
        Some((row.reverb, row.chorus, comp_send)),
        &row.name,
        actions,
    );
    fader(
        &mut strip,
        fader_area,
        index,
        row.volume,
        level,
        TRACK_UNITY,
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
        TRACK_UNITY,
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
        Symbol::Instrument(row.instrument),
        Color32::from_rgb(red, green, blue),
        row.audible,
        16.0,
    );
    // The way into this track's sound chain, on the name's row.
    let stomp = Rect::from_min_size(
        pos2(bottom.right() - 14.0, bottom.top() + 41.0),
        vec2(14.0, 14.0),
    );
    if stomp_button(
        &mut strip,
        stomp,
        index,
        &format!("Sound chain for {}", row.name),
        "Open this track's sound chain",
    ) {
        actions.push(GridAction::OpenSoundChain { track: index });
    }
    // A drum track's kit opens beside it as part strips.
    if row.drums {
        let arrow = Rect::from_min_size(
            pos2(bottom.right() - 14.0, bottom.top() + 56.0),
            vec2(14.0, 13.0),
        );
        if parts_arrow(&mut strip, arrow, index, state.parts_open(index)) {
            state.toggle_parts(index);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn master_strip(
    ui: &mut egui::Ui,
    master: MasterMix,
    level: Reading,
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
    knobs(
        &mut strip,
        top,
        usize::MAX,
        master.pan,
        None,
        "Master",
        actions,
    );
    fader(
        &mut strip,
        fader_area,
        usize::MAX,
        master.volume,
        level,
        MASTER_UNITY,
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
        MASTER_UNITY,
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
        Symbol::Waveform,
        visuals.selection.bg_fill,
        !all_muted,
        16.0,
    );
    let stomp = Rect::from_min_size(
        pos2(bottom.right() - 14.0, bottom.top() + 41.0),
        vec2(14.0, 14.0),
    );
    if stomp_button(
        &mut strip,
        stomp,
        usize::MAX,
        "Sound chain for the master",
        "Open the master's sound chain (after the tracks and buses)",
    ) {
        actions.push(GridAction::OpenMasterChain);
    }
}

/// The mixer's buses as the strips show them.
#[derive(Debug, Clone)]
pub(super) struct Buses {
    pub(super) reverb: ReverbBus,
    pub(super) compressor: CompressorBus,
    pub(super) chorus: ChorusBus,
}

/// What a bus strip asked for.
#[derive(Debug)]
enum BusChange {
    /// Open the bus's own window (what it runs and its knobs).
    Edit,
    /// Its level.
    Level { level: u8 },
    /// Its mute was clicked.
    Mute(bool),
}

/// What a bus strip shows.
struct BusStrip<'a> {
    /// Its name under the strip.
    name: &'a str,
    /// The tooltip of the button into its window.
    edit_tip: &'a str,
    /// The id of its controls (the master's is `usize::MAX`).
    id: usize,
    level: u8,
    muted: bool,
    meter: Reading,
    height: f32,
}

/// A bus's strip: the way into its window, its fader, its level field and its mute. The
/// tracks send into it after their faders; its level decides how much comes back.
fn bus_strip(
    ui: &mut egui::Ui,
    strip: BusStrip<'_>,
    state: &mut MixerState,
    changes: &mut Vec<BusChange>,
) {
    let BusStrip {
        name,
        edit_tip,
        id,
        level,
        muted,
        meter,
        height,
    } = strip;
    let (rect, _) = ui.allocate_exact_size(vec2(STRIP_WIDTH, height), Sense::hover());
    let visuals = ui.visuals().clone();
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (_, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    fader(
        &mut strip,
        fader_area,
        id,
        level,
        meter,
        127,
        &format!("{name} bus level"),
        |level, _| changes.push(BusChange::Level { level }),
    );
    let db_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top()),
        vec2(bottom.width(), 16.0),
    );
    db_field(
        &mut strip,
        db_rect,
        state,
        id,
        level,
        127,
        &format!("{name} bus level"),
        |level| changes.push(BusChange::Level { level }),
    );
    let buttons = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 18.0),
        vec2(bottom.width(), 20.0),
    );
    let response = strip
        .put(buttons, style::mute_button(muted))
        .on_hover_text(if muted {
            format!(
                "The {} bus is muted: click to hear it again",
                name.to_lowercase()
            )
        } else {
            format!(
                "Mute the {} bus (its tail keeps running)",
                name.to_lowercase()
            )
        });
    strip.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("Mute {} bus", name.to_lowercase()));
    });
    if response.clicked() {
        changes.push(BusChange::Mute(!muted));
    }
    labels(
        &mut strip,
        bottom,
        name,
        "Mixer bus",
        Symbol::Waveform,
        visuals.selection.bg_fill,
        !muted,
        16.0,
    );
    // The way into the bus's own window, on the name's row like every other strip's.
    let stomp = Rect::from_min_size(
        pos2(bottom.right() - 14.0, bottom.top() + 41.0),
        vec2(14.0, 14.0),
    );
    if stomp_button(
        &mut strip,
        stomp,
        id,
        &format!("Sound chain for the {} bus", name.to_lowercase()),
        edit_tip,
    ) {
        changes.push(BusChange::Edit);
    }
}

/// The reverb bus: the tracks send their mixer reverb amount into it, and its level (and
/// mute) decide how much of its tail goes into the mix. Its knobs are the room's.
fn reverb_strip(
    ui: &mut egui::Ui,
    bus: ReverbBus,
    meter: Reading,
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    /// The id of the bus's controls (the master's is `usize::MAX`).
    const REVERB: usize = usize::MAX - 2;
    let mut changes = Vec::new();
    bus_strip(
        ui,
        BusStrip {
            name: "Reverb",
            edit_tip: "The bus's reverb: the computed one, or a room's impulse response",
            id: REVERB,
            level: bus.level,
            muted: bus.muted,
            meter,
            height,
        },
        state,
        &mut changes,
    );
    for change in changes {
        let mut bus = bus.clone();
        let action = match change {
            BusChange::Edit => GridAction::OpenReverbBus,
            BusChange::Level { level } => {
                bus.level = level;
                GridAction::ReverbBus { bus }
            }
            BusChange::Mute(muted) => {
                bus.muted = muted;
                GridAction::ReverbBus { bus }
            }
        };
        actions.push(action);
    }
}

/// The compressor bus (parallel compression): each track's Comp send feeds it, and its
/// level blends the compressed sum back in under the dry tracks.
fn compressor_strip(
    ui: &mut egui::Ui,
    bus: CompressorBus,
    meter: Reading,
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    /// The id of the bus's controls (the master's is `usize::MAX`).
    const COMPRESSOR: usize = usize::MAX - 3;
    let mut changes = Vec::new();
    bus_strip(
        ui,
        BusStrip {
            name: "Comp",
            edit_tip: "The bus's compressor: a CA-76, driven hard, blended in under the tracks",
            id: COMPRESSOR,
            level: bus.level,
            muted: bus.muted,
            meter,
            height,
        },
        state,
        &mut changes,
    );
    for change in changes {
        let mut bus = bus.clone();
        let action = match change {
            BusChange::Edit => GridAction::OpenCompressorBus,
            BusChange::Level { level } => {
                bus.level = level;
                GridAction::CompressorBus { bus }
            }
            BusChange::Mute(muted) => {
                bus.muted = muted;
                GridAction::CompressorBus { bus }
            }
        };
        actions.push(action);
    }
}

/// The chorus bus: each track's Chorus knob is its send, and its level blends the chorused
/// signal back in under the dry tracks.
fn chorus_strip(
    ui: &mut egui::Ui,
    bus: ChorusBus,
    meter: Reading,
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    /// The id of the bus's controls (the master's is `usize::MAX`).
    const CHORUS: usize = usize::MAX - 4;
    let mut changes = Vec::new();
    bus_strip(
        ui,
        BusStrip {
            name: "Chorus",
            edit_tip: "The bus's chorus: a slow, wide drift every track's Chorus knob sends into",
            id: CHORUS,
            level: bus.level,
            muted: bus.muted,
            meter,
            height,
        },
        state,
        &mut changes,
    );
    for change in changes {
        let mut bus = bus.clone();
        let action = match change {
            BusChange::Edit => GridAction::OpenChorusBus,
            BusChange::Level { level } => {
                bus.level = level;
                GridAction::ChorusBus { bus }
            }
            BusChange::Mute(muted) => {
                bus.muted = muted;
                GridAction::ChorusBus { bus }
            }
        };
        actions.push(action);
    }
}

/// One part of a drum kit as its strip shows it.
#[derive(Debug, Clone)]
pub(super) struct PartRow {
    pub(super) part: DrumPart,
    /// Its mixing level (0-127, 127 unity).
    pub(super) level: u8,
    /// Its pan inside the kit (0 left, 64 centre, 127 right).
    pub(super) pan: u8,
    pub(super) muted: bool,
    pub(super) soloed: bool,
    /// Whether anything in the kit is soloed: then only the soloed parts play.
    pub(super) any_soloed: bool,
}

impl PartRow {
    /// Whether the part is heard: not muted, and soloed if anything is soloed.
    fn audible(&self) -> bool {
        !self.muted && (!self.any_soloed || self.soloed)
    }
}

/// A light wash of `color` for the strips of a drum kit's parts: its color raised toward
/// white, laid over the dark mixer at a low opacity, so the parts read as one group.
fn part_wash(color: Color32) -> Color32 {
    let lift = |channel: u8| -> u8 {
        let channel = u16::from(channel);
        u8::try_from((channel * 7 + 255 * 3) / 10).unwrap_or(255)
    };
    Color32::from_rgba_unmultiplied(lift(color.r()), lift(color.g()), lift(color.b()), 72)
}

/// A drum part's strip: its pan, meter, fader, mute, solo and the way into its sound
/// chain.
#[allow(clippy::too_many_arguments)]
fn part_strip(
    ui: &mut egui::Ui,
    track: usize,
    row: &TrackRow,
    part: &PartRow,
    level: Reading,
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    let id = part_id(track, part.part);
    let (rect, response) = ui.allocate_exact_size(vec2(PART_STRIP_WIDTH, height), Sense::hover());
    // A block dragged from a chain board onto this part's strip goes into its chain.
    if let Some(drag) = response.dnd_release_payload::<DraggedBlock>() {
        actions.push(GridAction::PasteBlock {
            track,
            part: Some(part.part),
            block: drag.block.clone(),
        });
    }
    let visuals = ui.visuals().clone();
    ui.painter()
        .rect_filled(rect, 0.0, visuals.faint_bg_color.gamma_multiply(0.7));
    // The kit's parts share a light wash of the drum track's color, so they read as one
    // group beside its strip.
    let (red, green, blue) = row.color.parts();
    ui.painter()
        .rect_filled(rect, 0.0, part_wash(Color32::from_rgb(red, green, blue)));
    if response.dnd_hover_payload::<DraggedBlock>().is_some() {
        ui.painter().rect_stroke(
            rect,
            3.0,
            Stroke::new(2.0, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    ui.painter().line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (top, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    // The part's pan where a track has its knobs (its name is in the labels below).
    let pan_rect = Rect::from_center_size(
        pos2(top.center().x, top.top() + KNOB_SIZE / 2.0 + 2.0),
        vec2(KNOB_SIZE, KNOB_SIZE),
    );
    let knob = mixer::pan_knob(
        &mut strip,
        pan_rect,
        part.pan,
        id,
        &format!("Pan {}", part.part.label()),
    );
    strip.painter().text(
        pos2(pan_rect.center().x, pan_rect.bottom() + 2.0),
        Align2::CENTER_TOP,
        "Pan",
        FontId::proportional(8.0),
        visuals.weak_text_color(),
    );
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((pan, merge)) = knob.change {
        actions.push(GridAction::DrumPart {
            track,
            part: part.part,
            level: part.level,
            pan,
            muted: part.muted,
            soloed: part.soloed,
            merge,
        });
    }
    fader(
        &mut strip,
        fader_area,
        id,
        part.level,
        level,
        127,
        &format!("{} level", part.part.label()),
        |level, merge| {
            actions.push(GridAction::DrumPart {
                track,
                part: part.part,
                level,
                pan: part.pan,
                muted: part.muted,
                soloed: part.soloed,
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
        id,
        part.level,
        127,
        &format!("{} level", part.part.label()),
        |level| {
            actions.push(GridAction::DrumPart {
                track,
                part: part.part,
                level,
                pan: part.pan,
                muted: part.muted,
                soloed: part.soloed,
                merge: false,
            });
        },
    );
    let buttons = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 18.0),
        vec2(bottom.width(), 20.0),
    );
    part_mute_solo(&mut strip, buttons, track, part, actions);
    let (red, green, blue) = row.color.parts();
    labels(
        &mut strip,
        bottom,
        part.part.label(),
        "Drum part",
        Symbol::Waveform,
        Color32::from_rgb(red, green, blue),
        part.audible(),
        16.0,
    );
    let stomp = Rect::from_min_size(
        pos2(bottom.right() - 14.0, bottom.top() + 41.0),
        vec2(14.0, 14.0),
    );
    if stomp_button(
        &mut strip,
        stomp,
        id,
        &format!("Sound chain for the {} part", part.part.label()),
        "Open this part's sound chain",
    ) {
        actions.push(GridAction::OpenSoundChainPart {
            track,
            part: part.part,
        });
    }
}

/// A stable control id for a drum part (away from the tracks', buses' and master's).
fn part_id(track: usize, part: DrumPart) -> usize {
    usize::MAX.saturating_sub(128 + track.saturating_mul(8) + part.index())
}

/// The mute and solo of one part of a drum kit.
fn part_mute_solo(
    ui: &mut egui::Ui,
    rect: Rect,
    track: usize,
    part: &PartRow,
    actions: &mut Vec<GridAction>,
) {
    let width = (rect.width() - 4.0) / 2.0;
    let mute_rect = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let solo_rect = Rect::from_min_size(
        pos2(rect.left() + width + 4.0, rect.top()),
        vec2(width, rect.height()),
    );
    let mute = ui.put(mute_rect, style::mute_button(part.muted));
    ui.ctx().accesskit_node_builder(mute.id, |node| {
        node.set_label(format!("Mute {}", part.part.label()));
    });
    if mute.on_hover_text("Mute this part of the kit").clicked() {
        actions.push(GridAction::DrumPart {
            track,
            part: part.part,
            level: part.level,
            pan: part.pan,
            muted: !part.muted,
            soloed: part.soloed,
            merge: false,
        });
    }
    let solo = ui.put(solo_rect, style::solo_button(part.soloed));
    ui.ctx().accesskit_node_builder(solo.id, |node| {
        node.set_label(format!("Solo {}", part.part.label()));
    });
    if solo.on_hover_text("Solo this part of the kit").clicked() {
        actions.push(GridAction::DrumPart {
            track,
            part: part.part,
            level: part.level,
            pan: part.pan,
            muted: part.muted,
            soloed: !part.soloed,
            merge: false,
        });
    }
}

/// The metronome's own strip: only its click volume, on a fader like the tracks' (the
/// switch stays in the toolbar). Full volume is the click as loud as it comes.
fn metronome_strip(
    ui: &mut egui::Ui,
    (volume, level): (f32, f32),
    height: f32,
    state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    /// The id of the metronome's controls (the master's is `usize::MAX`).
    const METRONOME: usize = usize::MAX - 1;
    let (rect, _) = ui.allocate_exact_size(vec2(STRIP_WIDTH, height), Sense::hover());
    let visuals = ui.visuals().clone();
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3)),
    );
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let (_, fader_area, bottom) = layout(inner);
    let mut strip = ui.new_child(UiBuilder::new().max_rect(inner));
    let value = (volume.clamp(0.0, 1.0) * 127.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = value as u8;
    let set = |value: u8| f32::from(value) / 127.0;
    fader(
        &mut strip,
        fader_area,
        METRONOME,
        value,
        Reading::estimated(level, 64),
        127,
        "Metronome volume",
        |value, _| actions.push(GridAction::SetMetronomeVolume(set(value))),
    );
    let db_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top()),
        vec2(bottom.width(), 16.0),
    );
    db_field(
        &mut strip,
        db_rect,
        state,
        METRONOME,
        value,
        127,
        "Metronome level",
        |value| actions.push(GridAction::SetMetronomeVolume(set(value))),
    );
    labels(
        &mut strip,
        bottom,
        "Metronome",
        "Click volume",
        Symbol::Metronome,
        visuals.selection.bg_fill,
        true,
        0.0,
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

/// A small stomp-box icon that opens a strip's sound chain. True when clicked.
fn stomp_button(ui: &mut egui::Ui, rect: Rect, id: usize, label: &str, tip: &str) -> bool {
    let response = ui.interact(rect, ui.scope_id().with(("stomp", id)), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Button);
    });
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 3.0, visuals.weak_bg_fill);
    }
    icons::paint(
        ui.painter(),
        rect.shrink(2.0),
        visuals.fg_stroke.color,
        icons::Icon::StompBox,
    );
    response.on_hover_text(tip).clicked()
}

/// The ▸/◂ arrow that opens or closes a drum track's part strips. True when clicked.
fn parts_arrow(ui: &mut egui::Ui, rect: Rect, track: usize, open: bool) -> bool {
    let response = ui.interact(rect, ui.scope_id().with(("parts", track)), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(if open {
            "Hide the drum parts"
        } else {
            "Show the drum parts"
        });
        node.set_role(egui::Role::Button);
    });
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 2.0, visuals.weak_bg_fill);
    }
    let center = rect.center();
    let half = 3.5;
    let points = if open {
        vec![
            pos2(center.x + half, center.y - half),
            pos2(center.x + half, center.y + half),
            pos2(center.x - half, center.y),
        ]
    } else {
        vec![
            pos2(center.x - half, center.y - half),
            pos2(center.x - half, center.y + half),
            pos2(center.x + half, center.y),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        visuals.fg_stroke.color,
        Stroke::NONE,
    ));
    response
        .on_hover_text(if open {
            "Hide this drum track's parts"
        } else {
            "Show this drum track's parts (kick, snare, hi-hat, toms, cymbals, percussion)"
        })
        .clicked()
}

/// The knob row: pan, and for a track its reverb and chorus sends (the master has only its
/// pan). Each knob has a small caption underneath.
fn knobs(
    ui: &mut egui::Ui,
    area: Rect,
    index: usize,
    pan: u8,
    sends: Option<(u8, u8, u8)>,
    name: &str,
    actions: &mut Vec<GridAction>,
) {
    let knob_y = area.top() + KNOB_SIZE / 2.0 + 2.0;
    let at = |dx: f32| {
        Rect::from_center_size(
            pos2(area.center().x + dx, knob_y),
            vec2(KNOB_SIZE, KNOB_SIZE),
        )
    };
    let caption = |ui: &egui::Ui, rect: Rect, text: &str| {
        ui.painter().text(
            pos2(rect.center().x, rect.bottom() + 2.0),
            Align2::CENTER_TOP,
            text,
            FontId::proportional(8.0),
            ui.visuals().weak_text_color(),
        );
    };
    // Four knobs side by side (pan, reverb, chorus, comp); the captions fit under them.
    let step = if sends.is_some() { 30.0 } else { KNOB_SPACING };
    let pan_rect = if sends.is_some() {
        at(-1.5 * step)
    } else {
        at(0.0)
    };
    let knob = mixer::pan_knob(ui, pan_rect, pan, index, &format!("Pan {name}"));
    caption(ui, pan_rect, "Pan");
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
    let Some((reverb, chorus, comp)) = sends else {
        return;
    };
    let track = TrackIndex::new(index);
    let reverb_rect = at(-0.5 * step);
    let knob = mixer::send_knob(
        ui,
        reverb_rect,
        reverb,
        ("reverb", index),
        &format!("Reverb {name}"),
        "Reverb send (into the reverb bus, whatever plays the track)",
        0,
    );
    caption(ui, reverb_rect, "Reverb");
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((reverb, merge)) = knob.change {
        actions.push(GridAction::Command {
            command: Command::SetTrackReverb { track, reverb },
            merge,
        });
    }
    let chorus_rect = at(0.5 * step);
    let knob = mixer::send_knob(
        ui,
        chorus_rect,
        chorus,
        ("chorus", index),
        &format!("Chorus {name}"),
        "Chorus send (into the shared chorus bus)",
        0,
    );
    caption(ui, chorus_rect, "Chorus");
    if knob.boundary {
        actions.push(GridAction::BreakMerge);
    }
    if let Some((chorus, merge)) = knob.change {
        actions.push(GridAction::Command {
            command: Command::SetTrackChorus { track, chorus },
            merge,
        });
    }
    // The send into the mixer's compressor bus (parallel compression): it is the song's,
    // kept in its notes, so it is no undoable command.
    let comp_rect = at(1.5 * step);
    let knob = mixer::send_knob(
        ui,
        comp_rect,
        comp,
        ("comp", index),
        &format!("Comp {name}"),
        "Send into the compressor bus (parallel compression)",
        0,
    );
    caption(ui, comp_rect, "Comp");
    if let Some((comp, _)) = knob.change {
        actions.push(GridAction::CompSend {
            track: index,
            value: comp,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn fader(
    ui: &mut egui::Ui,
    area: Rect,
    index: usize,
    value: u8,
    level: Reading,
    unity: u8,
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
    let response = response.on_hover_text(format!(
        "{label}: {} (volume {value})",
        mixer::db_text(value, unity)
    ));
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
    paint_gain_scale(ui, gain_scale, area, unity);
    paint_meter_scale(ui, meter_scale, area);
    paint_meter(ui, meter, level);
    let meter_response = ui.interact(meter, ui.scope_id().with(("meter", index)), Sense::hover());
    meter_response.on_hover_text(if level.measured {
        "Output level, left and right: the bar holds a peak briefly, the line shows the \
         highest of the last 1.5 s"
    } else {
        "Estimated output level, left and right (from the notes: the built-in synthesizer \
         measures the real one)"
    });
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

/// What one strip's meter shows: left and right bars with their peak marks, measured by
/// the built-in synthesizer or estimated from the notes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Reading {
    pub(super) level: gscore_audio::Level,
    pub(super) measured: bool,
}

impl Reading {
    /// A measured level.
    pub(super) fn measured(level: gscore_audio::Level) -> Self {
        Reading {
            level,
            measured: true,
        }
    }

    /// An estimate of one level (0 to 1), split into left and right by `pan`.
    pub(super) fn estimated(level: f32, pan: u8) -> Self {
        let balance = f32::from(pan.min(127)) / 127.0;
        let (left, right) = (level * (1.0 - balance).sqrt(), level * balance.sqrt());
        Reading {
            level: gscore_audio::Level {
                left,
                right,
                mark_left: left,
                mark_right: right,
            },
            measured: false,
        }
    }

    /// The louder bar (for the song grid's activity bars).
    pub(super) fn bar(&self) -> f32 {
        self.level.left.max(self.level.right)
    }
}

/// Every strip's reading: the tracks (by index), the reverb and compressor buses and the
/// master.
#[derive(Debug, Clone, Default)]
pub(super) struct Readings {
    pub(super) tracks: Vec<Reading>,
    /// Each track's drum parts, by track and then part (empty for a melodic track).
    pub(super) parts: Vec<Vec<Reading>>,
    pub(super) bus: Reading,
    pub(super) compressor: Reading,
    pub(super) chorus: Reading,
    pub(super) master: Reading,
}

/// Left and right meter segments, each with its peak mark (a line, when it stands above
/// the bar).
fn paint_meter(ui: &egui::Ui, rect: Rect, reading: Reading) {
    let gap = 2.0;
    let width = (rect.width() - gap) / 2.0;
    let left = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let right = Rect::from_min_size(
        pos2(left.right() + gap, rect.top()),
        vec2(width, rect.height()),
    );
    let level = reading.level;
    for (side, bar, mark) in [
        (left, level.left, level.mark_left),
        (right, level.right, level.mark_right),
    ] {
        let bar = meter_fraction(bar);
        paint_segment(ui, side, bar);
        let mark = meter_fraction(mark);
        if mark > bar + 0.005 {
            let y = side.bottom() - side.height() * mark;
            ui.painter().line_segment(
                [pos2(side.left(), y), pos2(side.right(), y)],
                Stroke::new(1.5, mark_color(mark)),
            );
        }
    }
}

/// A peak mark's color: the bar's color at that height (see [`paint_segment`]).
fn mark_color(fill: f32) -> Color32 {
    if fill > 0.9 {
        Color32::from_rgb(0xF2, 0x55, 0x4D)
    } else if fill > 0.72 {
        Color32::from_rgb(0xFD, 0xD8, 0x35)
    } else {
        Color32::from_rgb(0x43, 0xA0, 0x47)
    }
}

/// The gain scale left of the fader: where each mark's gain sits on the fader's travel
/// (for the fader whose 0 dB is volume `unity`), then −∞ at the bottom.
fn paint_gain_scale(ui: &egui::Ui, rect: Rect, area: Rect, unity: u8) {
    let visuals = ui.visuals();
    let weak = visuals.weak_text_color();
    let font = FontId::monospace(8.0);
    let top = db_value(127, unity);
    let mut placed: Vec<(f32, f32)> = Vec::with_capacity(GAIN_MARKS.len());
    for db in GAIN_MARKS {
        if db > top + 0.05 {
            continue;
        }
        let y = area.bottom() - area.height() * gain_fraction(db, unity);
        placed.push((db, y));
    }
    // Bottom up, so the quiet end (where the marks crowd) is thinned, not the top; the −∞
    // label at the bottom counts as the first one.
    let mut last_label = area.bottom() - 4.0;
    for &(db, y) in placed.iter().rev() {
        ui.painter().line_segment(
            [pos2(rect.right() - 4.0, y), pos2(rect.right(), y)],
            Stroke::new(1.0, weak.gamma_multiply(0.6)),
        );
        if last_label - y < LABEL_GAP {
            continue;
        }
        last_label = y;
        let text = if db > 0.05 {
            format!("+{db:.0}")
        } else {
            format!("{db:.0}")
        };
        ui.painter().text(
            pos2(rect.right() - 6.0, y),
            Align2::RIGHT_CENTER,
            text,
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
#[allow(clippy::too_many_arguments)]
fn db_field(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut MixerState,
    target: usize,
    volume: u8,
    unity: u8,
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
            // A typed minus sign may be the Unicode one the readout shows; "dB" is optional.
            let text = text
                .replace('−', "-")
                .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c.is_whitespace())
                .to_owned();
            let db = if text.is_empty() || text.contains("inf") || text.contains('∞') {
                Some(f32::NEG_INFINITY)
            } else {
                text.parse::<f32>().ok().filter(|db| db.is_finite())
            };
            if let Some(db) = db {
                // Above the fader's top is the top; very quiet is silence.
                let new = volume_from_db(db, unity);
                if new != volume {
                    on_change(new);
                }
            }
            state.editing_db = None;
        }
        return;
    }
    let text = mixer::db_text(volume, unity);
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
                "-inf".to_owned()
            } else {
                format!("{:.1}", db_value(volume, unity))
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
    let mute = ui.put(mute_rect, style::mute_button(row.muted));
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
    let solo = ui.put(solo_rect, style::solo_button(row.soloed));
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
    let mute = ui.put(mute_rect, style::mute_button(all_muted));
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
    let solo = ui.put(solo_rect, style::solo_button(all_soloed));
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

/// The small symbol before a strip's second line.
#[derive(Debug, Clone, Copy)]
enum Symbol {
    /// A track's instrument.
    Instrument(gscore_model::InstrumentSymbol),
    /// The metronome strip.
    Metronome,
    /// The master strip, the sum of every track: a short waveform.
    Waveform,
}

/// A short waveform filling `rect`: a few cycles that swell and fade.
fn paint_waveform(painter: &egui::Painter, rect: Rect, ink: Color32) {
    const STEPS: usize = 40;
    let points = (0..=STEPS)
        .map(|step| {
            let t = step as f32 / STEPS as f32;
            let envelope = (std::f32::consts::PI * t).sin();
            let wave = (std::f32::consts::TAU * 2.5 * t).sin();
            pos2(
                rect.left() + rect.width() * t,
                rect.center().y - rect.height() * 0.48 * envelope * wave,
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.0, ink)));
}

#[allow(clippy::too_many_arguments)]
fn labels(
    ui: &mut egui::Ui,
    bottom: Rect,
    name: &str,
    preset: &str,
    symbol: Symbol,
    color: Color32,
    audible: bool,
    trailing: f32,
) {
    let visuals = ui.visuals().clone();
    let name_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 42.0),
        vec2(bottom.width() - trailing, 14.0),
    );
    // A silent track's name and color recede, so the strips show at a glance what plays.
    let name_ink = if audible {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    ui.painter().with_clip_rect(name_rect).text(
        pos2(name_rect.left(), name_rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(10.0),
        name_ink,
    );
    let name_response = ui.interact(name_rect, ui.scope_id().with("mixer_name"), Sense::hover());
    name_response.on_hover_text(name);

    let preset_rect = Rect::from_min_size(
        pos2(bottom.left(), bottom.top() + 58.0),
        vec2(bottom.width() - trailing, 12.0),
    );
    let icon = Rect::from_min_size(
        pos2(preset_rect.left(), preset_rect.center().y - 5.0),
        vec2(10.0, 10.0),
    );
    let ink = visuals.weak_text_color();
    match symbol {
        Symbol::Instrument(instrument) => instrument::paint(ui.painter(), icon, instrument, ink),
        Symbol::Metronome => icons::paint(ui.painter(), icon, ink, icons::Icon::Metronome),
        Symbol::Waveform => paint_waveform(ui.painter(), icon, ink),
    }
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
    ui.painter()
        .rect_filled(color_rect, 1.0, mixer::sounding_color(color, audible));
}

/// The fader travel for a volume, 0 (silence) to 1 (volume 127): `(v / 127)^(2/3)`. As the
/// gain is `40·log10(v / 127)` dB, the travel is `10^(dB / 60)` below the top: −6 dB sits
/// at 79%, −12 dB at 63%, −24 dB at 40% and −60 dB at 10%, so the loud end gets the most
/// room, as on a mixing desk.
fn fader_fraction(volume: u8) -> f32 {
    (f32::from(volume) / 127.0).powf(2.0 / 3.0).clamp(0.0, 1.0)
}

/// The volume at a fader position (the inverse of [`fader_fraction`]).
fn volume_at_fraction(fraction: f32) -> u8 {
    let volume = 127.0 * fraction.clamp(0.0, 1.0).powf(1.5);
    volume.round().clamp(0.0, 127.0) as u8
}

/// Where a gain `db` sits on the travel of the fader whose 0 dB is volume `unity`.
fn gain_fraction(db: f32, unity: u8) -> f32 {
    if !db.is_finite() {
        return 0.0;
    }
    // The exact (unrounded) volume, so the marks sit where the gain really is.
    let volume = f32::from(unity) * 10.0_f32.powf(db / 40.0);
    (volume / 127.0).powf(2.0 / 3.0).clamp(0.0, 1.0)
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
    use gscore_model::MASTER_UNITY;

    use super::{Reading, TRACK_UNITY, fader_fraction, gain_fraction, volume_at_fraction};

    #[test]
    fn an_estimate_is_split_by_the_pan_and_a_measurement_kept_as_it_is() {
        let middle = Reading::estimated(0.5, 64);
        assert!((middle.level.left - middle.level.right).abs() < 0.01);
        assert!(!middle.measured);
        let left = Reading::estimated(0.5, 0);
        assert_eq!(left.level.right, 0.0);
        assert_eq!(
            left.level.mark_left, left.level.left,
            "an estimate has no hold"
        );
        assert_eq!(left.bar(), 0.5);
        let level = gscore_audio::Level {
            left: 0.2,
            right: 0.4,
            mark_left: 0.6,
            mark_right: 0.4,
        };
        let measured = Reading::measured(level);
        assert!(measured.measured);
        assert_eq!(measured.level, level);
        assert_eq!(measured.bar(), 0.4);
    }

    #[test]
    fn the_fader_travel_is_logarithmic_with_room_at_the_loud_end() {
        assert!((fader_fraction(127) - 1.0).abs() < 1e-6);
        assert!(fader_fraction(0).abs() < 1e-6);
        assert!((gain_fraction(0.0, TRACK_UNITY) - 1.0).abs() < 1e-4);
        assert!((gain_fraction(-12.0, TRACK_UNITY) - 0.631).abs() < 0.01);
        assert!((gain_fraction(-60.0, TRACK_UNITY) - 0.1).abs() < 0.01);
        // The master's 0 dB (its default) sits below the top: it has headroom.
        let unity = gain_fraction(0.0, MASTER_UNITY);
        assert!((unity - fader_fraction(MASTER_UNITY)).abs() < 1e-4);
        assert!(unity < 0.9, "{unity}");
        for volume in 0..=127_u8 {
            assert_eq!(volume_at_fraction(fader_fraction(volume)), volume);
        }
    }
}
