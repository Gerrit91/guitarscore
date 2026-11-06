//! The Sound chain window: what plays the track and the effects after it, laid out like a
//! pedal board. Six boxes in a fixed row, joined by cables: what plays the track first
//! (MIDI synth or PM synthesis; their sound - the kind of string and its knobs - shows in
//! the same window), then five places, each empty or holding a block (a cabinet or body
//! impulse response, a room impulse response, the computed reverb, amp or drive pedal). A
//! block can be dragged to another place, the blocks between sliding one place over, so the
//! order the sound passes them in is the one the board shows. The selected place's choices
//! or knobs show below the board. One chain per kind of string; every change is heard at
//! once and kept in the settings.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use gscore_guitar::{BLOCKS, knob_ranges};
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use crate::app::string_sound::{kind_name, round_knob, sound_knobs};
use crate::app::style::{self, Tokens};
use crate::app::track_engine::TrackPlayer;
use crate::chain_settings::{BlockChoice, BlockSetting, ChainSetting};
use crate::playback::Heard;
use crate::settings::{
    ChainPreset, MAX_STRING_PRESET_NAME, MAX_STRING_PRESETS, StringSound, engine_from_id,
    engine_id, kind_id,
};

/// Width of the window's content.
const WIDTH: f32 = 700.0;
/// A box on the board, the gap between boxes (where the cable runs), the board's height.
const BOX: egui::Vec2 = vec2(86.0, 60.0);
const GAP: f32 = 36.0;
const BOARD_HEIGHT: f32 = 96.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct SoundChainWindow {
    /// The track it is for (its kind of string decides which chain shows).
    track: usize,
    /// The selected box: 0 the strings, 1 to 5 the places after them.
    selected: usize,
    pos: Option<Pos2>,
    /// The name being typed for a new preset, while saving.
    saving: Option<String>,
    /// The preset last chosen or saved (its kind and name), so it can be saved over after
    /// its knobs were moved.
    preset: Option<(StringKind, String)>,
}

/// What the user did this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    listen: bool,
    load: Option<usize>,
    chain: Option<ChainSetting>,
    /// A room's length moved and its knob was let go: build the room again.
    rebuild: bool,
    /// What plays the track was chosen in the first box.
    engine: Option<TrackEngine>,
    /// The strings' sound changed (in the first box's editor).
    sound: Option<StringSound>,
    /// A preset was chosen: its string sound, its chain and what plays the strings.
    preset: Option<ChosenPreset>,
    /// Save the sound and the chain under this name, or delete that preset (its kind of
    /// string and its name, since names are unique within a kind only).
    save_preset: Option<String>,
    delete_preset: Option<(String, String)>,
}

/// A preset chosen in the list: the whole sound it carries.
struct ChosenPreset {
    /// The kind of string the sound belongs to; `None` keeps the kind the window shows.
    kind: Option<StringKind>,
    sound: StringSound,
    chain: ChainSetting,
    /// What plays the strings, when the preset names it (older ones do not).
    engine: Option<TrackEngine>,
}

/// What editing the selected block asked for.
#[derive(Default)]
struct Edited {
    /// The chain with the edit, when something moved.
    chain: Option<ChainSetting>,
    /// Open the file dialog for the impulse response.
    load: bool,
    /// The room's length changed: its response must be prepared again.
    rebuild: bool,
}

/// What the knobs of a block asked for.
#[derive(Default, Clone, Copy)]
pub(super) struct KnobChanges {
    /// A knob moved: the new values are sent.
    pub(super) changed: bool,
    /// The room's length knob was let go: the room is built again.
    pub(super) rebuild: bool,
}

/// The block a box holds, as its two lines of text.
fn box_text(block: &BlockSetting) -> (String, String) {
    let detail = match block.choice {
        BlockChoice::Empty => String::new(),
        BlockChoice::Reverb => "computed".to_owned(),
        BlockChoice::Amp => block.voice.label().to_owned(),
        BlockChoice::Drive => "overdrive".to_owned(),
        BlockChoice::Cabinet | BlockChoice::Room => block
            .file
            .as_ref()
            .and_then(|file| file.file_stem())
            .map_or_else(
                || "no file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ),
    };
    let title = match block.choice {
        BlockChoice::Empty => "+".to_owned(),
        // "Cabinet / body" does not fit a box.
        BlockChoice::Cabinet => "Cab".to_owned(),
        other => other.label().to_owned(),
    };
    (title, detail)
}

/// `text` cut to fit about `chars` characters.
fn short(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// The first box: what plays the track (the internal synth, or physical modelling), the
/// kind of string and - for modelled strings - their sound, all in this window.
fn strings_editor(
    ui: &mut egui::Ui,
    player: &TrackPlayer,
    sound: &mut StringSound,
    actions: &mut Actions,
) -> bool {
    let mut changed = false;
    ui.label(RichText::new("Played by").strong());
    ui.horizontal_wrapped(|ui| {
        // PM keeps the kind it had (or the one the track's preset stands for).
        let modelled_kind = match player.engine {
            TrackEngine::Modelled(kind) => Some(kind),
            TrackEngine::SoundFont => player.preset_kind,
        };
        let pm = TrackEngine::Modelled(modelled_kind.unwrap_or(StringKind::Steel));
        let mut choice = |ui: &mut egui::Ui, engine: TrackEngine, label: &str, enabled: bool| {
            let response = ui
                .add_enabled(
                    enabled,
                    egui::Button::selectable(player.engine == engine, label),
                )
                .on_hover_text(match engine {
                    TrackEngine::SoundFont => {
                        "The internal synth plays the track with the SoundFont (General MIDI)"
                    }
                    TrackEngine::Modelled(_) => {
                        "Physical modelling: the strings are synthesized and played through \
                         the chain"
                    }
                });
            if response.clicked() {
                actions.engine = Some(engine);
            }
        };
        choice(ui, TrackEngine::SoundFont, "MIDI synth", true);
        choice(ui, pm, "PM Synthesis", player.strings);
    });
    if !player.strings {
        ui.label(
            RichText::new("Only string instruments can be played by modelled strings.").weak(),
        );
        return changed;
    }
    let Some(kind) = (match player.engine {
        TrackEngine::Modelled(kind) => Some(kind),
        TrackEngine::SoundFont => None,
    }) else {
        return changed;
    };
    // Which strings, then their sound: the kind's knobs, right here.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Strings").color(style::tokens(ui).text_weak));
        for candidate in [
            StringKind::Nylon,
            StringKind::Steel,
            StringKind::Electric,
            StringKind::Bass,
        ] {
            let selected = candidate == kind;
            if ui
                .add(egui::Button::selectable(selected, kind_name(candidate)))
                .clicked()
                && !selected
            {
                actions.engine = Some(TrackEngine::Modelled(candidate));
            }
        }
    });
    ui.add_space(2.0);
    changed |= sound_knobs(ui, sound);
    changed
}

/// The chain presets: the one being worked on, saving it (over it or under a new name),
/// choosing another and deleting. A preset is the whole chain plus the string sound it was
/// saved with, and the kind of string it belongs to.
fn preset_row(
    ui: &mut egui::Ui,
    kind: StringKind,
    sound: &StringSound,
    chain: &ChainSetting,
    presets: &[ChainPreset],
    window: &mut SoundChainWindow,
    actions: &mut Actions,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Preset").color(style::tokens(ui).text_weak));
        let built_in = || StringSound::from(gscore_audio::default_string_model(kind));
        // The preset being worked on: one whose sound and chain match exactly, or the one
        // last chosen or saved for this kind (its knobs may have moved since).
        let exact = presets
            .iter()
            .find(|preset| preset.sound == *sound && preset.chain == *chain)
            .map(|preset| preset.name.clone());
        let remembered = window
            .preset
            .as_ref()
            .filter(|(saved, _)| *saved == kind)
            .map(|(_, name)| name.clone());
        let active = exact.clone().or(remembered);
        let is_built_in = built_in() == *sound && *chain == ChainSetting::default();
        let shown = match (&exact, &active) {
            (Some(name), _) => name.clone(),
            (None, Some(name)) if !is_built_in => format!("{name} *"),
            (None, _) if is_built_in => "Built-in".to_owned(),
            (None, Some(name)) => name.clone(),
            (None, None) => "Changed".to_owned(),
        };
        let response = egui::ComboBox::from_id_salt("sound_chain_preset")
            .selected_text(shown.clone())
            .show_ui(ui, |ui| {
                if ui.selectable_label(false, "Built-in").clicked() {
                    window.preset = None;
                    actions.preset = Some(ChosenPreset {
                        kind: None,
                        sound: built_in(),
                        chain: ChainSetting::default(),
                        engine: None,
                    });
                }
                for preset in presets {
                    if ui.selectable_label(false, &preset.name).clicked() {
                        // A preset brings the kind of string it was made for: a modelled
                        // one plays it, and an older preset (saved before the engine was
                        // part of it) was made for the modelled strings of its kind.
                        let target = match engine_from_id(&preset.kind) {
                            Some(TrackEngine::Modelled(saved)) => saved,
                            _ => kind,
                        };
                        window.preset = Some((target, preset.name.clone()));
                        actions.preset = Some(ChosenPreset {
                            kind: Some(target),
                            sound: preset.sound,
                            chain: preset.chain.clone(),
                            engine: Some(
                                engine_from_id(&preset.engine)
                                    .unwrap_or(TrackEngine::Modelled(target)),
                            ),
                        });
                    }
                }
            });
        ui.ctx()
            .accesskit_node_builder(response.response.id, |node| {
                node.set_label("Preset");
                node.set_value(shown);
            });
        match &mut window.saving {
            None => {
                // Saving over the preset being worked on, and deleting it.
                if let Some(name) = active.clone() {
                    let save = ui
                        .button("Save")
                        .on_hover_text(format!("Save over \"{name}\" (its knobs as they are now)"));
                    ui.ctx().accesskit_node_builder(save.id, |node| {
                        node.set_label("Save preset");
                    });
                    if save.clicked() {
                        actions.save_preset = Some(name.clone());
                        window.preset = Some((kind, name.clone()));
                    }
                    let delete = ui
                        .button("Delete")
                        .on_hover_text(format!("Delete \"{name}\""));
                    ui.ctx().accesskit_node_builder(delete.id, |node| {
                        node.set_label("Delete preset");
                    });
                    if delete.clicked() {
                        actions.delete_preset = Some((kind_id(kind).to_owned(), name));
                        window.preset = None;
                    }
                }
                if ui
                    .button("Save as…")
                    .on_hover_text("Save this chain and its string sound under a new name")
                    .clicked()
                {
                    window.saving = Some(String::new());
                }
            }
            Some(name) => {
                let field = ui.add(
                    egui::TextEdit::singleline(name)
                        .char_limit(MAX_STRING_PRESET_NAME)
                        .hint_text("Name")
                        .desired_width(120.0),
                );
                ui.ctx()
                    .accesskit_node_builder(field.id, |node| node.set_label("Preset name"));
                let name_ok = !name.trim().is_empty();
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(name_ok, egui::Button::new("Save")).clicked()
                    || (enter && name_ok)
                {
                    let name = name.trim().to_owned();
                    actions.save_preset = Some(name.clone());
                    window.preset = Some((kind, name));
                    window.saving = None;
                } else if ui.button("Cancel").clicked() {
                    window.saving = None;
                }
            }
        }
    });
}

/// What the board asked for.
#[derive(Default)]
struct BoardActions {
    /// The box clicked: 0 the strings, 1 to 5 a place.
    select: Option<usize>,
    /// An empty box's effect was chosen: the place gets it.
    fill: Option<(usize, BlockChoice)>,
    /// A block's menu asked for it: its place is emptied.
    remove: Option<usize>,
    /// The mute under a place was clicked: its bypass flips.
    bypass: Option<usize>,
    /// A block was dropped on another place: (from, to), both places (0 to 4).
    moved: Option<(usize, usize)>,
}

/// Moves the block at place `from` to place `to`, the places in between sliding one place
/// toward `from` - the order the sound passes them in follows the board. False when
/// nothing moves: the same place, or a place off the board.
fn move_block(blocks: &mut [BlockSetting], from: usize, to: usize) -> bool {
    if from == to {
        return false;
    }
    let range = if from < to {
        blocks.get_mut(from..=to)
    } else {
        blocks.get_mut(to..=from)
    };
    let Some(range) = range else {
        return false;
    };
    if from < to {
        range.rotate_left(1);
    } else {
        range.rotate_right(1);
    }
    true
}

/// What an empty place can hold: the block, the name its menu shows, and what it does.
fn block_choices() -> [(BlockChoice, &'static str, &'static str); 5] {
    [
        (
            BlockChoice::Drive,
            "Drive",
            "An overdrive pedal: in front of an amp it tightens the lows and pushes the mids",
        ),
        (
            BlockChoice::Amp,
            "Amp",
            "A computed guitar amp: clean, crunch or lead, with a tone stack and a speaker",
        ),
        (
            BlockChoice::Cabinet,
            "Cabinet / body",
            "An impulse response from a WAV file: an acoustic guitar's body, or a cabinet \
             after an amp",
        ),
        (
            BlockChoice::Room,
            "Room",
            "A room's impulse response from a WAV file (stereo if recorded so): a convolution \
             reverb",
        ),
        (
            BlockChoice::Reverb,
            "Reverb",
            "A computed reverb, from a small room to a cathedral, no file needed",
        ),
    ]
}

/// What a box on the board shows.
struct BoxFace<'a> {
    /// An empty place: just its "+".
    empty: bool,
    /// The block passes the sound unchanged: weak text and "off" in the corner.
    bypassed: bool,
    title: &'a str,
    detail: &'a str,
}

/// One box on the board: its fill and outline, its two lines of text (cut to the box's
/// width, since an impulse response's file name is often longer, and clipped, so nothing
/// spills over a neighbour) and, while bypassed, "off" in its corner.
fn paint_box(
    painter: &egui::Painter,
    rect: Rect,
    face: &BoxFace<'_>,
    fill: Color32,
    outline: Stroke,
    tokens: &Tokens,
) {
    painter.rect_filled(rect, 6.0, fill);
    painter.rect_stroke(rect, 6.0, outline, egui::StrokeKind::Inside);
    let text = if face.bypassed || face.empty {
        tokens.text_weak
    } else {
        tokens.text
    };
    let inside = painter.with_clip_rect(rect.shrink(3.0));
    let title_size = if face.empty { 22.0 } else { 13.0 };
    inside.text(
        rect.center() - vec2(0.0, if face.empty { 0.0 } else { 8.0 }),
        Align2::CENTER_CENTER,
        short(face.title, 13),
        FontId::proportional(title_size),
        text,
    );
    if !face.detail.is_empty() {
        inside.text(
            rect.center() + vec2(0.0, 12.0),
            Align2::CENTER_CENTER,
            short(face.detail, 12),
            FontId::proportional(10.0),
            tokens.text_weak,
        );
    }
    if face.bypassed {
        painter.text(
            rect.left_top() + vec2(6.0, 4.0),
            Align2::LEFT_TOP,
            "off",
            FontId::proportional(10.0),
            tokens.text_weak,
        );
    }
}

/// The board: the boxes, the cables between them and, under each block, its mute. A block
/// can be dragged to another place; the block follows the mouse and the place it would
/// land in is outlined, until it is let go there.
fn board(
    ui: &mut egui::Ui,
    player: &TrackPlayer,
    chain: &ChainSetting,
    selected: usize,
) -> BoardActions {
    let tokens = style::tokens(ui);
    let (area, _) = ui.allocate_exact_size(vec2(WIDTH, BOARD_HEIGHT), Sense::hover());
    let count = BLOCKS.saturating_add(1);
    let row = BOX.x * count as f32 + GAP * BLOCKS as f32;
    let left = area.left() + (WIDTH - row) / 2.0;
    let top = area.top() + 4.0;
    let rect_of =
        |index: usize| Rect::from_min_size(pos2(left + index as f32 * (BOX.x + GAP), top), BOX);
    let painter = ui.painter_at(area);
    // Cables first, under the boxes: a straight line from one box's output jack to the next
    // box's input jack.
    let cable = Stroke::new(2.5, tokens.text_weak.gamma_multiply(0.8));
    for index in 0..BLOCKS {
        let from = rect_of(index).right_center();
        let to = rect_of(index.saturating_add(1)).left_center();
        painter.line_segment([from, to], cable);
        for end in [from, to] {
            painter.circle_filled(end, 3.5, tokens.text_weak);
        }
    }
    let mut actions = BoardActions::default();
    // The box being dragged (0 the strings, 1 to 5 a place), while one is.
    let dragging = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|index| *index);
    for index in 0..count {
        let rect = rect_of(index);
        let block = index
            .checked_sub(1)
            .and_then(|place| chain.blocks.get(place));
        let empty = block.is_some_and(|block| block.choice == BlockChoice::Empty);
        let bypassed = block.is_some_and(|block| block.bypass);
        // A block on the board can be dragged to another place; everything else is clicked.
        let draggable = block.is_some_and(|block| block.choice != BlockChoice::Empty);
        let sense = if draggable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let mut response = ui
            .interact(rect, ui.scope_id().with(("chain_box", index)), sense)
            .on_hover_text(if index == 0 {
                "What plays the track: click for its sound (the pick, sustain, tone of the \
                 strings)"
            } else if draggable {
                "Click to choose or adjust this block; drag it to another place to move it \
                 along the chain"
            } else {
                "Click to choose or adjust this block"
            });
        if draggable {
            // What is held while the block is dragged; the place it is let go on takes it.
            response.dnd_set_drag_payload(index);
            let held = dragging == Some(index);
            response = response.on_hover_cursor(if held {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        let (title, detail) = match block {
            None => match player.engine {
                TrackEngine::Modelled(kind) => ("Strings".to_owned(), kind_name(kind).to_owned()),
                TrackEngine::SoundFont => (
                    "MIDI".to_owned(),
                    format!(
                        "{}: {}",
                        player.program,
                        gscore_model::program_name(player.program)
                    ),
                ),
            },
            Some(block) => box_text(block),
        };
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label(format!("Block {index}: {title}"));
            node.set_role(egui::Role::Button);
        });
        // A place takes a block dragged from another place: it is outlined while the mouse
        // is over it, and the drop moves the block here.
        let mut landing = false;
        if index > 0 {
            if let Some(from) = response.dnd_hover_payload::<usize>() {
                landing = *from != index;
            }
            if let Some(from) = response.dnd_release_payload::<usize>() {
                actions.moved = Some((from.saturating_sub(1), index.saturating_sub(1)));
            }
        }
        let fill = if index == 0 {
            tokens.control_active
        } else if empty {
            tokens.panel
        } else if dragging == Some(index) {
            // The block itself is on the mouse: its place waits, empty-looking, for a drop.
            tokens.panel
        } else if response.hovered() {
            tokens.control_hover
        } else {
            tokens.control
        };
        let outline = if landing || index == selected {
            Stroke::new(2.0, tokens.accent)
        } else {
            Stroke::new(1.0, tokens.line)
        };
        paint_box(
            &painter,
            rect,
            &BoxFace {
                empty,
                bypassed,
                title: &title,
                detail: &detail,
            },
            fill,
            outline,
            tokens,
        );
        if index == 0 && response.clicked() {
            actions.select = Some(0);
        }
        if let Some(place) = index.checked_sub(1) {
            if empty {
                // An empty place offers the effects in a menu at the box; the window below
                // keeps showing what it showed.
                egui::Popup::menu(&response).show(|ui| {
                    for (choice, label, tip) in block_choices() {
                        if ui.button(label).on_hover_text(tip).clicked() {
                            actions.fill = Some((place, choice));
                            ui.close();
                        }
                    }
                });
            } else {
                if response.clicked() {
                    actions.select = Some(index);
                }
                // A block's own menu: right-click removes or bypasses it.
                let bypassed = chain.blocks.get(place).is_some_and(|block| block.bypass);
                egui::Popup::context_menu(&response).show(|ui| {
                    let tip = if bypassed {
                        "Let this block be heard again"
                    } else {
                        "Let the sound pass this block unchanged"
                    };
                    if ui
                        .button(if bypassed { "Hear it again" } else { "Bypass" })
                        .on_hover_text(tip)
                        .clicked()
                    {
                        actions.bypass = Some(place);
                        ui.close();
                    }
                    if ui
                        .button("Remove")
                        .on_hover_text("Empty this place of the board")
                        .clicked()
                    {
                        actions.remove = Some(place);
                        ui.close();
                    }
                });
            }
            // Under a block, its mute: the block passes the sound unchanged while it is on.
            if empty {
                continue;
            }
            let mute = Rect::from_center_size(
                pos2(rect.center().x, rect.bottom() + 12.0),
                vec2(26.0, 16.0),
            );
            let response = ui
                .put(mute, style::mute_button(bypassed))
                .on_hover_text(if bypassed {
                    "This block is bypassed: click to hear it again"
                } else {
                    "Bypass this block (let the sound pass it unchanged)"
                });
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(format!("Mute block {}", place.saturating_add(1)));
            });
            if response.clicked() {
                actions.bypass = Some(place);
            }
        }
    }
    // The block being dragged, at the mouse, above the window.
    if let Some(index) = dragging
        && let Some(block) = index
            .checked_sub(1)
            .and_then(|place| chain.blocks.get(place))
        && let Some(pointer) = ui.ctx().pointer_interact_pos()
    {
        let layer = egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("chain_drag"));
        let painter = ui.ctx().layer_painter(layer);
        let (title, detail) = box_text(block);
        paint_box(
            &painter,
            Rect::from_center_size(pointer, BOX),
            &BoxFace {
                empty: false,
                bypassed: block.bypass,
                title: &title,
                detail: &detail,
            },
            tokens.control_active,
            Stroke::new(2.0, tokens.accent),
            tokens,
        );
    }
    actions
}

/// The selected place's choices (when empty) or knobs.
fn editor(ui: &mut egui::Ui, chain: &ChainSetting, place: usize) -> Edited {
    let Some(block) = chain.blocks.get(place) else {
        return Edited::default();
    };
    let mut edited = block.clone();
    let mut load = false;
    let mut changed = false;
    let mut rebuild = false;
    if block.choice == BlockChoice::Empty {
        ui.label(
            RichText::new("This place is empty: click its box on the board to choose an effect.")
                .weak(),
        );
    } else {
        ui.label(RichText::new(block.choice.label()).strong());
        if block.choice.uses_file() {
            ui.horizontal(|ui| {
                let name = block.file.as_ref().map_or_else(
                    || "No impulse response loaded".to_owned(),
                    |file| file.display().to_string(),
                );
                ui.label(RichText::new(short(&name, 70)).weak());
                if ui.button("Load WAV…").clicked() {
                    load = true;
                }
            });
        }
        ui.add_space(4.0);
        let moved = block_knobs(ui, &mut edited, false);
        changed |= moved.changed;
        // A value from a hand-edited settings file comes back within its range.
        changed |= edited.length != block.length;
        rebuild = moved.rebuild;
    }
    let chain = changed.then(|| {
        let mut new_chain = chain.clone();
        if let Some(slot) = new_chain.blocks.get_mut(place) {
            *slot = edited;
        }
        new_chain
    });
    Edited {
        chain,
        load,
        rebuild,
    }
}

/// The knobs of the block in `block`, as round knobs. `all_wet` hides the mix (a bus runs
/// all wet: its sends decide how much each track gives it).
pub(super) fn block_knobs(
    ui: &mut egui::Ui,
    block: &mut BlockSetting,
    all_wet: bool,
) -> KnobChanges {
    let mut moved = KnobChanges::default();
    if all_wet {
        block.mix = 1.0;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        match block.choice {
            BlockChoice::Empty => {}
            BlockChoice::Amp => moved.changed |= super::amp_knobs::amp_knobs(ui, block),
            BlockChoice::Drive => moved.changed |= super::amp_knobs::drive_knobs(ui, block),
            BlockChoice::Cabinet | BlockChoice::Room => {
                // First the room itself: how long it lasts.
                if block.choice == BlockChoice::Room {
                    // A hand-edited settings file may hold anything; show it in range.
                    block.length = block.room_length();
                    let edit = round_knob(
                        ui,
                        &mut block.length,
                        gscore_guitar::LENGTH_RANGE,
                        "Length",
                        "×",
                        1.0,
                        true,
                        "The room's decay time against the recording's: 0.5 half as long, 4 \
                         four times as long (past the recording, a synthesized tail carries \
                         it on). Built again when you let go of the knob.",
                    );
                    moved.changed |= edit.changed;
                    moved.rebuild |= edit.released;
                }
                if !all_wet {
                    moved.changed |= round_knob(
                        ui,
                        &mut block.mix,
                        knob_ranges::MIX,
                        "Mix",
                        "",
                        0.25,
                        false,
                        "How much of the convolved sound: 0 dry, 1 only the response.",
                    )
                    .changed;
                }
                moved.changed |= round_knob(
                    ui,
                    &mut block.gain_db,
                    knob_ranges::GAIN,
                    "Gain",
                    " dB",
                    0.0,
                    false,
                    "The level of the convolved sound.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.low_cut,
                    knob_ranges::LOW_CUT,
                    "Low cut",
                    " Hz",
                    120.0,
                    true,
                    "Takes away the lows below this frequency (boom, rumble); on a room, \
                     only from the room's sound.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.high_cut,
                    knob_ranges::HIGH_CUT,
                    "High cut",
                    " Hz",
                    9_000.0,
                    true,
                    "Takes away the highs above this frequency (fizz, hiss); on a room, only \
                     from the room's sound.",
                )
                .changed;
                if block.choice == BlockChoice::Room {
                    moved.changed |= round_knob(
                        ui,
                        &mut block.width,
                        knob_ranges::WIDTH,
                        "Width",
                        "",
                        1.0,
                        false,
                        "Stereo width of the room: 0 mono, 1 as recorded.",
                    )
                    .changed;
                }
            }
            BlockChoice::Reverb => {
                moved.changed |= round_knob(
                    ui,
                    &mut block.size,
                    knob_ranges::SIZE,
                    "Size",
                    "",
                    0.5,
                    false,
                    "The room's size: a small room to a cathedral. How far apart its echoes \
                     are; how long it rings is Decay.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.decay,
                    knob_ranges::DECAY,
                    "Decay",
                    " s",
                    3.0,
                    true,
                    "How long the reverb rings: seconds until it has fallen by 60 dB.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.pre_delay,
                    knob_ranges::PRE_DELAY,
                    "Pre-delay",
                    " ms",
                    20.0,
                    false,
                    "The wait before the reverb starts: keeps the notes clear of it.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.damping,
                    knob_ranges::DAMPING,
                    "Damping",
                    "",
                    0.5,
                    false,
                    "How much sooner the highs die than the lows: 0 bright, 1 dull.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.modulation,
                    knob_ranges::MODULATION,
                    "Modulation",
                    "",
                    0.5,
                    false,
                    "How much the tail drifts: 0 still, 1 a lush shimmer. Keeps long tails \
                     from ringing metallic.",
                )
                .changed;
                // The room above, what comes out of it below.
                ui.end_row();
                moved.changed |= round_knob(
                    ui,
                    &mut block.width,
                    knob_ranges::WIDTH,
                    "Width",
                    "",
                    1.0,
                    false,
                    "Stereo width: 0 mono, 1 wide.",
                )
                .changed;
                if !all_wet {
                    moved.changed |= round_knob(
                        ui,
                        &mut block.mix,
                        knob_ranges::MIX,
                        "Mix",
                        "",
                        0.2,
                        false,
                        "How much of the room: 0 dry, 1 only the room.",
                    )
                    .changed;
                }
                moved.changed |= round_knob(
                    ui,
                    &mut block.low_cut,
                    knob_ranges::LOW_CUT,
                    "Low cut",
                    " Hz",
                    150.0,
                    true,
                    "Takes the lows out of the reverb (not the dry guitar): keeps it from \
                     booming and muddying the bass.",
                )
                .changed;
                moved.changed |= round_knob(
                    ui,
                    &mut block.high_cut,
                    knob_ranges::HIGH_CUT,
                    "High cut",
                    " Hz",
                    7_000.0,
                    true,
                    "Takes the highs out of the reverb (not the dry guitar): a darker, more \
                     distant room.",
                )
                .changed;
            }
        }
    });
    moved
}

impl GuitarScoreApp {
    /// Opens the Sound chain window for `track`.
    pub(super) fn open_sound_chain(&mut self, track: usize) {
        match &mut self.sound_chain {
            Some(window) => window.track = track,
            None => {
                self.sound_chain = Some(SoundChainWindow {
                    track,
                    selected: 0,
                    pos: None,
                    saving: None,
                    preset: None,
                });
            }
        }
    }

    /// The impulse response chosen for place `place` of `kind`'s chain: read (an error is
    /// shown) and heard at once.
    pub(super) fn use_impulse(&mut self, kind: StringKind, place: usize, path: PathBuf) {
        if let Err(error) = self.playback.impulse(&path) {
            self.notice = Some(error);
            return;
        }
        let mut chain = self.playback.string_chains().get(kind).clone();
        if let Some(block) = chain.blocks.get_mut(place) {
            block.file = Some(path);
        }
        self.playback.set_string_chain(kind, chain);
    }

    /// The Sound chain window, while it is open.
    pub(super) fn show_sound_chain(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.sound_chain.take() else {
            return;
        };
        let size = vec2(WIDTH + 32.0, 600.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut actions = Actions::default();
        let Some(player) = self.track_player(window.track) else {
            return;
        };
        // The chain of the kind that plays the track; for the SoundFont, the one its
        // preset stands for (shown, but not heard).
        let (kind, modelled) = match player.engine {
            TrackEngine::Modelled(kind) => (kind, true),
            TrackEngine::SoundFont => (player.preset_kind.unwrap_or(StringKind::Steel), false),
        };
        // Every preset, whatever kind of string it was saved for (choosing one also
        // switches the track to that kind when it names a modelled engine).
        let mut presets: Vec<ChainPreset> = self.settings.string_presets.clone();
        presets.sort_by_cached_key(|preset| preset.name.to_lowercase());
        // The title names the track's preset (the instrument in the track panel); the
        // saved chain presets are the list at the bottom. The board follows straight away.
        let title = format!(
            "Sound chain — Preset: {}",
            gscore_model::program_name(player.program)
        );
        egui::Window::new(&title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, &title, &mut window.pos) {
                    actions.close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        let chain = self.playback.string_chains().get(kind).clone();
                        let sound = self.playback.string_sounds().get(kind);
                        let board = board(ui, &player, &chain, window.selected);
                        if let Some(index) = board.select {
                            window.selected = index;
                        }
                        if let Some(place) = board.remove {
                            let mut changed = chain.clone();
                            if let Some(slot) = changed.blocks.get_mut(place) {
                                *slot = BlockSetting::default();
                            }
                            actions.chain = Some(changed);
                            window.selected = place.saturating_add(1);
                        }
                        if let Some((place, choice)) = board.fill {
                            let mut changed = chain.clone();
                            if let Some(slot) = changed.blocks.get_mut(place) {
                                *slot = BlockSetting::of(choice);
                            }
                            actions.chain = Some(changed);
                            window.selected = place.saturating_add(1);
                            if choice.uses_file() {
                                actions.load = Some(place);
                            }
                        }
                        if let Some(place) = board.bypass
                            && let Some(block) = chain.blocks.get(place)
                        {
                            let mut changed = chain.clone();
                            if let Some(slot) = changed.blocks.get_mut(place) {
                                slot.bypass = !block.bypass;
                            }
                            actions.chain = Some(changed);
                        }
                        // A block dragged to another place: the chain follows the board, so
                        // the sound passes the blocks in the order shown.
                        if let Some((from, to)) = board.moved {
                            let mut changed = chain.clone();
                            if move_block(&mut changed.blocks, from, to) {
                                actions.chain = Some(changed);
                                window.selected = to.saturating_add(1);
                            }
                        }
                        ui.separator();
                        match window.selected.checked_sub(1) {
                            None => {
                                let mut sound = sound;
                                if strings_editor(ui, &player, &mut sound, &mut actions) {
                                    actions.sound = Some(sound);
                                }
                            }
                            Some(place) => {
                                let edited = editor(ui, &chain, place);
                                if let Some(changed) = edited.chain {
                                    actions.chain = Some(changed);
                                }
                                actions.rebuild |= edited.rebuild;
                                if edited.load {
                                    actions.load = Some(place);
                                }
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        // The preset: which one this is, another one, and saving it.
                        preset_row(
                            ui,
                            kind,
                            &sound,
                            &chain,
                            &presets,
                            &mut window,
                            &mut actions,
                        );
                        ui.add_space(4.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button("Listen")
                                .on_hover_text("Play a short phrase through this chain")
                                .clicked()
                            {
                                actions.listen = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        actions.close = true;
                                    }
                                },
                            );
                        });
                    });
            });
        // A preset first: it carries both a string sound and a chain.
        if let Some(preset) = actions.preset {
            let target = preset.kind.unwrap_or(kind);
            self.playback.set_string_sound(target, preset.sound);
            self.playback.set_string_chain(target, preset.chain);
            if let Some(engine) = preset.engine {
                self.set_track_engine(window.track, engine);
            }
        }
        if let Some(engine) = actions.engine {
            self.set_track_engine(window.track, engine);
        }
        if let Some(sound) = actions.sound {
            self.playback.set_string_sound(kind, sound);
        }
        if let Some(chain) = actions.chain {
            self.playback.set_string_chain(kind, chain);
        }
        // A room's length changes its prepared response, so the chain is built again now
        // that the knob has been let go (after the new value is stored).
        if actions.rebuild {
            self.playback.rebuild_chain(kind);
        }
        if let Some(name) = actions.save_preset {
            let preset = ChainPreset {
                name,
                kind: kind_id(kind).to_owned(),
                sound: self.playback.string_sounds().get(kind),
                chain: self.playback.string_chains().get(kind).clone(),
                engine: engine_id(player.engine).to_owned(),
            };
            let presets = &mut self.settings.string_presets;
            presets.retain(|old| !(old.kind == preset.kind && old.name == preset.name));
            if presets.len() < MAX_STRING_PRESETS {
                presets.push(preset);
            }
            self.save_settings();
        }
        if let Some((preset_kind, name)) = actions.delete_preset {
            self.settings
                .string_presets
                .retain(|preset| !(preset.kind == preset_kind && preset.name == name));
            self.save_settings();
        }
        if let Some(place) = actions.load
            && let Err(error) = self.tasks.pick_impulse(kind, place, ctx)
        {
            self.notice = Some(error);
        }
        if actions.listen && modelled {
            match self.playback.play_string_demo(kind) {
                Ok(Heard::Skipped(why)) => self.notice = Some(format!("Not played: {why}")),
                Ok(_) => {}
                Err(error) => self.notice = Some(error),
            }
        }
        if actions.close {
            self.save_settings();
        } else {
            self.sound_chain = Some(window);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_block_dragged_to_another_place_slides_the_blocks_between() {
        let mut chain = ChainSetting::default();
        chain.blocks[0] = BlockSetting::of(BlockChoice::Drive);
        chain.blocks[1] = BlockSetting::of(BlockChoice::Amp);
        chain.blocks[2] = BlockSetting::of(BlockChoice::Reverb);
        // Toward the end of the chain.
        let mut later = chain.clone();
        assert!(move_block(&mut later.blocks, 0, 2));
        assert_eq!(later.blocks[0].choice, BlockChoice::Amp);
        assert_eq!(later.blocks[1].choice, BlockChoice::Reverb);
        assert_eq!(later.blocks[2].choice, BlockChoice::Drive);
        assert_eq!(later.blocks[3].choice, BlockChoice::Empty);
        // And toward the strings.
        let mut earlier = chain.clone();
        assert!(move_block(&mut earlier.blocks, 2, 0));
        assert_eq!(earlier.blocks[0].choice, BlockChoice::Reverb);
        assert_eq!(earlier.blocks[1].choice, BlockChoice::Drive);
        assert_eq!(earlier.blocks[2].choice, BlockChoice::Amp);
        // The knobs travel with the block.
        let mut knobs = chain.clone();
        knobs.blocks[1].drive = 0.7;
        assert!(move_block(&mut knobs.blocks, 1, BLOCKS.saturating_sub(1)));
        assert_eq!(
            knobs.blocks[BLOCKS.saturating_sub(1)].choice,
            BlockChoice::Amp
        );
        assert_eq!(knobs.blocks[BLOCKS.saturating_sub(1)].drive, 0.7);
        // Nothing moves for the same place or one off the board.
        let mut same = chain.clone();
        assert!(!move_block(&mut same.blocks, 1, 1));
        assert_eq!(same, chain);
        let mut off = chain.clone();
        assert!(!move_block(&mut off.blocks, 1, BLOCKS.saturating_add(3)));
        assert_eq!(off, chain);
    }
}
