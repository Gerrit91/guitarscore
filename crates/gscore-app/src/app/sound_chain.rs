//! The Sound chain window: what plays the track and the effects after it, laid out like a
//! pedal board. Six boxes in a fixed row, joined by cables: what plays the track first
//! (MIDI synth or PM synthesis; their sound - the kind of string and its knobs - shows in
//! the same window), then five places, each empty or holding a block (a cabinet or body
//! impulse response, a room impulse response, the computed reverb, amp or drive pedal). A
//! block can be dragged to another place, the blocks between sliding one place over, so the
//! order the sound passes them in is the one the board shows. The selected place's choices
//! or knobs show below the board. What the window edits is the **tone of the track's
//! preset** (its General MIDI program): it is kept in the settings, so every track with
//! that preset, in every song, plays it, and Built-in forgets it (the kind's default
//! plays). Saved tones (the list at the bottom) can be given to any preset.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use gscore_guitar::{BLOCKS, knob_ranges};
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use super::grid::strips;
use crate::app::string_sound::{kind_name, round_knob, sound_knobs};
use crate::app::style::{self, Tokens};
use crate::app::track_engine::TrackPlayer;
use crate::chain_settings::{
    BlockChoice, BlockSetting, CabinetSetting, ChainSetting, DrumPart, Tone,
};
use crate::playback::{Heard, MidiOutRoute};
use crate::settings::{
    ChainPreset, MAX_STRING_PRESET_NAME, MAX_STRING_PRESETS, StringSound, engine_from_id,
    engine_id, kind_id,
};

/// Width of the window's content.
const WIDTH: f32 = 700.0;
/// The height of the slot above the board (the tone's two rows, or a drum part's level).
const HEADER_HEIGHT: f32 = 30.0;
/// The height of the chosen box's knobs: the tallest editor's but the EQ's, so the window
/// keeps one size whichever box is chosen; a chain with an EQ in it (its curve and
/// columns) takes the taller one...
const EDITOR_HEIGHT: f32 = 250.0;
const EQ_EDITOR_HEIGHT: f32 = 430.0;
/// ...unless the screen is too small for it: then it takes what the rest of the window
/// leaves (about this much), down to this height, and scrolls.
const EDITOR_ROOM: f32 = 420.0;
const EDITOR_MIN_HEIGHT: f32 = 200.0;
/// How far a track's input trim goes either way, in dB.
const TRIM_RANGE: (f32, f32) = (-24.0, 24.0);
/// A box on the board, the gap between boxes (where the cable runs), the board's height.
const BOX: egui::Vec2 = vec2(78.0, 70.0);
/// The widest gap between two boxes (the cable between them).
const GAP: f32 = 36.0;
/// The input trim's and the chain output's boxes: narrow, at the chain's two ends.
const END_WIDTH: f32 = 44.0;
const BOARD_HEIGHT: f32 = 114.0;
/// The smallest gap between two boxes: a board whose row needs more room than its width
/// leaves, even with these gaps, folds into two rows (a phone), the cable running down
/// from the first row's end to the second row's start.
const MIN_GAP: f32 = 8.0;
/// How far the board's second row starts below its first.
const ROW_STEP: f32 = BOARD_HEIGHT - 10.0;
/// The narrowest the window's content gets (a small phone, 360 points: a folded row of
/// four boxes still fits), and the room kept between the window and the screen's edges.
const MIN_WIDTH: f32 = 310.0;
const SCREEN_GUTTER: f32 = 8.0;
/// In the practice view the window is a sheet on a narrow screen, and on one less tall
/// than this (a phone on its side).
const SHEET_BELOW_HEIGHT: f32 = 600.0;
/// The window's frame around its content: the content's margins on either side.
const FRAME_WIDTH: f32 = 32.0;
/// The board's [`SoundChainWindow::selected`] value for the input trim's box.
pub(super) const SELECT_INPUT: usize = BLOCKS + 1;
/// The board's [`SoundChainWindow::selected`] value for the chain output's box.
pub(super) const SELECT_OUTPUT: usize = BLOCKS + 2;
/// The window's [`egui::Area`] id: stable even when the track named in its title changes.
const WINDOW_ID: &str = "gscore_sound_chain";

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
    /// The tone last chosen or saved for the track's preset (its program and name), so it
    /// can be saved over after its knobs were moved.
    preset: Option<(u8, String)>,
    /// Which part of a drum kit the board edits (`None`: the track's own chain, the drum
    /// bus).
    part: Option<DrumPart>,
    /// The string sound's tab shown in the strings' box.
    strings_tab: super::string_sound::StringsTab,
    /// The track's tone as the window found it: what A/B switches to.
    reference: Option<Tone>,
    /// Whether the track had a tone of its own then (else it played its built-in one).
    reference_own: bool,
    /// A/B is on: the track plays (and the window shows) the tone as the window found it,
    /// until A/B is clicked again.
    comparing: bool,
    /// The first box's list of instruments (General MIDI programs), for a synth.
    programs: super::tone_picker::TonePickerState,
    /// Whether the board has the keyboard: from a click on one of its boxes until a click
    /// outside the window. The arrows then move between the boxes, Delete empties the
    /// selected place and M mutes its block, instead of moving the score's cursor.
    board_keys: bool,
    /// The board's keys taken from this frame's input, before the score saw them.
    pending_keys: Vec<BoardKey>,
    /// Where the window was drawn last, for telling a click outside it.
    rect: Option<Rect>,
    /// A tone just imported from a file: the track switches to it on the next frame.
    imported: Option<ChainPreset>,
    /// A Plugin block's filter text (its list of plugins or its parameters).
    plugin_filter: String,
    /// A Plugin block that holds a plugin shows the list to choose another.
    plugin_choosing: bool,
}

/// A key the board handles while it has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardKey {
    Previous,
    Next,
    Remove,
    Mute,
}

/// The boxes in the order the arrows go through them: the source, the input trim, the
/// places, the output.
fn box_order() -> Vec<usize> {
    let mut order = vec![0, SELECT_INPUT];
    order.extend(1..=BLOCKS);
    order.push(SELECT_OUTPUT);
    order
}

/// The box after (or before) `selected` in [`box_order`], staying at the ends.
fn next_box(selected: usize, forward: bool) -> usize {
    let order = box_order();
    let Some(at) = order.iter().position(|&index| index == selected) else {
        return selected;
    };
    let to = if forward {
        at.saturating_add(1).min(order.len().saturating_sub(1))
    } else {
        at.saturating_sub(1)
    };
    order.get(to).copied().unwrap_or(selected)
}

/// What the user did this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    /// Look for installed plugins again.
    plugin_rescan: bool,
    listen: bool,
    load: Option<usize>,
    chain: Option<ChainSetting>,
    /// A room's length moved and its knob was let go: build the room again.
    rebuild: bool,
    /// What plays the track was chosen in the first box.
    engine: Option<TrackEngine>,
    /// The instrument (General MIDI program or drum kit) chosen in the first box.
    program: Option<u8>,
    /// A MIDI-out track's port or channel changed in the first box.
    midi_out: Option<MidiOutRoute>,
    /// The strings' sound changed (in the first box's editor).
    sound: Option<StringSound>,
    /// A saved tone was chosen for the track's preset: its string sound, its chain and
    /// what plays the strings.
    preset: Option<ChosenPreset>,
    /// Back to the program's default: the track's own tone is forgotten.
    reset_tone: bool,
    /// Back to the built-in tone of the drum part the window shows.
    reset_part: bool,
    /// A drum part's mixing level (0-127), while its slider moves.
    part_level: Option<u8>,
    /// A drum part's send into the kit's room (0-127), while its slider moves.
    part_room: Option<u8>,
    /// Open the kit's room window.
    open_room: bool,
    /// The track's input trim before its chain, in dB, while its knob turns.
    trim: Option<f32>,
    /// Save the sound and the chain under this name, or delete that preset (its kind of
    /// string and its name, since names are unique within a kind only).
    save_preset: Option<String>,
    delete_preset: Option<(String, String)>,
    /// The header's copy menu: the selected block, or the whole chain shown, goes on the
    /// clipboard.
    copy_block: bool,
    copy_chain: bool,
    /// The header's paste button: what the clipboard holds goes in (a block into the
    /// selected place or the first free one, a chain over the whole board).
    paste: bool,
    /// A/B was clicked: switch between the tone as the window found it and as it is now.
    toggle_compare: bool,
    /// Export the track's tone to a file, under this name.
    export_tone: Option<String>,
    /// Import a tone from a file.
    import_tone: bool,
    /// A block or chain copied from a board, for the app's clipboard.
    copy: Option<SoundClip>,
    /// A block dropped on a drum part's button (or the kit's), before that part's chain.
    pasted_part: Option<(Option<DrumPart>, BlockSetting)>,
}

/// A tone chosen in the list: the whole sound it carries.
struct ChosenPreset {
    /// The kind of string the sound belongs to.
    kind: StringKind,
    sound: StringSound,
    chain: ChainSetting,
    /// What plays the strings, when the tone names it (older ones do not).
    engine: Option<TrackEngine>,
    /// The row it was chosen from, marked while the track plays it.
    choice: super::tone_picker::PickerChoice,
}

impl ChosenPreset {
    /// A saved tone as a choice, for a track whose strings are `kind` now: its own kind
    /// and engine when it names a modelled one.
    fn of_saved(preset: &ChainPreset, kind: StringKind) -> Self {
        let target = match engine_from_id(&preset.kind) {
            Some(TrackEngine::Modelled(saved)) => saved,
            _ => kind,
        };
        ChosenPreset {
            kind: target,
            sound: preset.sound,
            chain: preset.chain.clone(),
            engine: Some(engine_from_id(&preset.engine).unwrap_or(TrackEngine::Modelled(target))),
            choice: super::tone_picker::PickerChoice::Saved {
                kind: preset.kind.clone(),
                name: preset.name.clone(),
            },
        }
    }
}

/// What editing the selected block asked for.
#[derive(Default)]
pub(super) struct Edited {
    /// The chain with the edit, when something moved.
    pub(super) chain: Option<ChainSetting>,
    /// Open the file dialog for the impulse response.
    pub(super) load: bool,
    /// The room's length changed: its response must be prepared again.
    pub(super) rebuild: bool,
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
        BlockChoice::Compressor => format!("CA-76 · {}", block.comp_ratio.label()),
        BlockChoice::Eq => "4 bands".to_owned(),
        BlockChoice::Chorus => "stereo".to_owned(),
        BlockChoice::Delay if block.delay_sync => block.delay_note.note().label().to_owned(),
        BlockChoice::Delay => format!("{:.0} ms", block.delay_ms),
        BlockChoice::Plugin => block
            .plugin
            .as_ref()
            .map_or_else(|| "none chosen".to_owned(), |plugin| plugin.vendor.clone()),
        BlockChoice::Cabinet | BlockChoice::Room => block
            .file
            .as_ref()
            .and_then(|file| file.file_stem())
            .map_or_else(
                // A cab without a file plays its built-in response.
                || {
                    if block.choice == BlockChoice::Cabinet {
                        block.cabinet.label().to_owned()
                    } else {
                        "built-in".to_owned()
                    }
                },
                |name| name.to_string_lossy().into_owned(),
            ),
    };
    let title = match block.choice {
        BlockChoice::Empty => "+".to_owned(),
        BlockChoice::Compressor => "Comp".to_owned(),
        BlockChoice::Plugin => block
            .plugin
            .as_ref()
            .map_or_else(|| "Plugin".to_owned(), |plugin| plugin.name.clone()),
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

/// The port and channel of a track played by a hardware synthesizer. Returns the new route
/// when either changed.
pub(super) fn midi_out_controls(
    ui: &mut egui::Ui,
    player: &TrackPlayer,
    ports: &[String],
) -> Option<MidiOutRoute> {
    let mut route = player.midi_out.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Port").color(style::tokens(ui).text_weak));
        let port_choice = egui::ComboBox::from_id_salt("midi_out_port")
            .selected_text(route.port.as_deref().unwrap_or("Automatic port"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut route.port, None, "Automatic port");
                for port in ports {
                    ui.selectable_value(&mut route.port, Some(port.clone()), port);
                }
            });
        ui.ctx()
            .accesskit_node_builder(port_choice.response.id, |node| {
                node.set_label("MIDI out port");
            });
        ui.label(RichText::new("Channel").color(style::tokens(ui).text_weak));
        let channel_choice = egui::ComboBox::from_id_salt("midi_out_channel")
            .selected_text(route.channel.map_or_else(
                || "Automatic channel".to_owned(),
                |c| c.saturating_add(1).to_string(),
            ))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut route.channel, None, "Automatic channel");
                for channel in 0..16_u8 {
                    ui.selectable_value(
                        &mut route.channel,
                        Some(channel),
                        channel.saturating_add(1).to_string(),
                    );
                }
            });
        ui.ctx()
            .accesskit_node_builder(channel_choice.response.id, |node| {
                node.set_label("MIDI out channel");
            });
    });
    ui.label(
        RichText::new(
            "Ports are listed in Settings › Audio I/O; Refresh ports there finds a device \
             plugged in since.",
        )
        .weak(),
    );
    ui.label(
        RichText::new(
            "The hardware synthesizer plays this track. Its chain and the mixer's effects do \
             not apply; the strip's meter shows its notes.",
        )
        .weak(),
    );
    if route != player.midi_out {
        Some(route)
    } else {
        None
    }
}

/// The first box: what plays the track (the internal synth, physical modelling or a MIDI
/// output) and then, for a synth, the instrument it plays (its General MIDI program) or,
/// for modelled strings, the kind of string and their sound, all in this window.
#[allow(clippy::too_many_arguments)]
fn strings_editor(
    ui: &mut egui::Ui,
    player: &TrackPlayer,
    sound: &mut StringSound,
    ports: &[String],
    tab: &mut super::string_sound::StringsTab,
    programs: &mut super::tone_picker::TonePickerState,
    actions: &mut Actions,
) -> bool {
    let mut changed = false;
    // PM keeps the kind it had (or the one the track's preset stands for).
    let modelled_kind = match player.engine {
        TrackEngine::Modelled(kind) => Some(kind),
        TrackEngine::SoundFont | TrackEngine::MidiOut => player.preset_kind,
    };
    let pm = TrackEngine::Modelled(modelled_kind.unwrap_or(StringKind::Steel));
    let source = |ui: &mut egui::Ui, actions: &mut Actions| {
        ui.label(RichText::new("Played by").strong());
        if let Some(engine) =
            super::tone_picker::source_switch(ui, player.engine, pm, player.strings)
        {
            actions.engine = Some(engine);
        }
    };
    let Some(kind) = (match player.engine {
        TrackEngine::Modelled(kind) => Some(kind),
        TrackEngine::SoundFont | TrackEngine::MidiOut => None,
    }) else {
        // A synth (the SoundFont or a hardware one): what plays it on the left, the
        // instrument it plays on the right (below it on a narrow screen), the same list as
        // the track panel's.
        let narrow = ui.available_width() < WIDTH;
        side_by_side(ui, narrow, |ui| {
            ui.vertical(|ui| {
                if !narrow {
                    ui.set_width(300.0);
                }
                source(ui, actions);
                if player.engine == TrackEngine::MidiOut {
                    if let Some(route) = midi_out_controls(ui, player, ports) {
                        actions.midi_out = Some(route);
                    }
                } else {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(if player.strings {
                            "The blocks to the right shape the instrument's sound."
                        } else {
                            "The blocks to the right shape the instrument's sound. Only \
                             string instruments can be played by modelled strings."
                        })
                        .weak(),
                    );
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(if player.drums {
                        "Drum kit"
                    } else {
                        "Instrument"
                    })
                    .strong(),
                );
                let rows = super::tone_picker::general_midi_rows(player.program, player.drums);
                if let Some(
                    super::tone_picker::PickerChoice::Program(program)
                    | super::tone_picker::PickerChoice::DrumKit(program),
                ) = super::tone_picker::picker_rows(
                    ui,
                    "chain_program_picker",
                    "programs",
                    &rows,
                    programs,
                    6,
                    None,
                ) {
                    actions.program = Some(program);
                }
            });
        });
        return changed;
    };
    source(ui, actions);
    // The string sound on tabs, apart from what plays the track: which strings first (the
    // Kind tab), then their knobs.
    ui.separator();
    changed |= sound_knobs(ui, sound, tab);
    if *tab == super::string_sound::StringsTab::Kind
        && let Some(picked) = super::string_kinds::kind_tiles(ui, kind)
    {
        actions.engine = Some(TrackEngine::Modelled(picked));
    }
    changed
}

/// The track's preset (its General MIDI program, whose tone the window edits) and the kind
/// of string it shows.
#[derive(Debug, Clone)]
struct ShownPreset {
    program: u8,
    kind: StringKind,
    /// What the header's copy and paste buttons can do.
    clip: ClipState,
    /// Whether the tone differs from the one the window found (A/B has something to
    /// compare).
    compare: bool,
    /// A/B is on: the tone shown is the one the window found, not to be edited.
    comparing: bool,
    /// The built-in or saved tone the track's tone was dialed in from, if known.
    basis: Option<String>,
    /// The built-in or saved tone last chosen from a list for the track, if any.
    chosen: Option<super::tone_picker::PickerChoice>,
}

/// The tone's buttons beside its dropdown: save (active once the tone changed; it saves
/// over the saved tone being worked on, or asks for a name), save under a new name,
/// delete the saved tone, and go back to the built-in tone.
#[allow(clippy::too_many_arguments)]
fn tone_buttons(
    ui: &mut egui::Ui,
    program: u8,
    kind: StringKind,
    changed: bool,
    built_in: bool,
    working: Option<String>,
    window: &mut SoundChainWindow,
    actions: &mut Actions,
) {
    use super::icons::{self, Icon};
    if let Some(name) = &mut window.saving {
        // Naming a tone: the field takes the place of the buttons.
        let field = ui.add(
            egui::TextEdit::singleline(name)
                .char_limit(MAX_STRING_PRESET_NAME)
                .hint_text("Name of the tone")
                .desired_width(150.0),
        );
        ui.ctx()
            .accesskit_node_builder(field.id, |node| node.set_label("Tone name"));
        // The field takes the keyboard while it is empty, so the name can be typed at once.
        if !field.has_focus() && name.is_empty() {
            field.request_focus();
        }
        let name_ok = !name.trim().is_empty();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.add_enabled(name_ok, egui::Button::new("Save")).clicked() || (enter && name_ok) {
            let name = name.trim().to_owned();
            actions.save_preset = Some(name.clone());
            window.preset = Some((program, name));
            window.saving = None;
        } else if ui.button("Cancel").clicked() {
            window.saving = None;
        }
        return;
    }
    let save_tip = match &working {
        _ if !changed => "Nothing to save: the tone is as it was chosen".to_owned(),
        Some(name) => format!("Save over \"{name}\" (its knobs as they are now)"),
        None => "Save this tone under a name, for other tracks".to_owned(),
    };
    let save = ui
        .add_enabled_ui(changed, |ui| {
            icons::toolbar_button(ui, Icon::Save, "Save tone", &save_tip)
        })
        .inner;
    if save.clicked() {
        match &working {
            Some(name) => {
                actions.save_preset = Some(name.clone());
                window.preset = Some((program, name.clone()));
            }
            None => window.saving = Some(String::new()),
        }
    }
    let save_as = ui
        .button("Save as…")
        .on_hover_text("Save this tone under a new name, for other tracks");
    ui.ctx()
        .accesskit_node_builder(save_as.id, |node| node.set_label("Save tone as"));
    if save_as.clicked() {
        window.saving = Some(String::new());
    }
    if let Some(name) = &working
        && icons::toolbar_button(
            ui,
            Icon::Delete,
            "Delete tone",
            &format!("Delete \"{name}\""),
        )
        .clicked()
    {
        actions.delete_preset = Some((kind_id(kind).to_owned(), name.clone()));
        window.preset = None;
    }
    let revert = ui
        .add_enabled_ui(!built_in, |ui| {
            icons::toolbar_button(
                ui,
                Icon::Revert,
                "Built-in tone",
                "Back to the built-in tone (forgets this track's own tone)",
            )
        })
        .inner;
    if revert.clicked() {
        window.preset = None;
        actions.reset_tone = true;
    }
}

/// The chain's copy and paste buttons, at the right end of the header.
/// The buttons only a track's own tone has (not a drum part's chain).
struct ToneRowButtons<'a> {
    /// Whether A/B has something to compare.
    compare: bool,
    /// A/B is on.
    comparing: bool,
    /// The tone's name, which an exported file is named after.
    name: &'a str,
}

/// What the header's copy and paste buttons can do: what the clipboard holds, and whether
/// a block is selected to copy.
#[derive(Debug, Clone, Copy)]
struct ClipState {
    /// `Some(true)`: a chain; `Some(false)`: a block; `None`: nothing.
    clipboard: Option<bool>,
    block_selected: bool,
}

/// `tone`: the A/B, export and import buttons too, for a track's own tone.
fn clip_buttons(
    ui: &mut egui::Ui,
    clip: ClipState,
    tone: Option<ToneRowButtons<'_>>,
    actions: &mut Actions,
) {
    use super::icons::{self, Icon};
    // While A/B shows the tone as it was, only A/B itself acts: that tone is not edited.
    let comparing = tone.as_ref().is_some_and(|tone| tone.comparing);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Paste puts in whatever was copied last: a block, or a whole chain.
        let paste = ui
            .add_enabled_ui(clip.clipboard.is_some() && !comparing, |ui| {
                icons::toolbar_button(
                    ui,
                    Icon::Paste,
                    "Paste",
                    match clip.clipboard {
                        Some(true) => "Paste the copied chain: it replaces every block of this one",
                        Some(false) => {
                            "Paste the copied block into the selected place (or the first free \
                             one)"
                        }
                        None => "Copy a block or a chain first",
                    },
                )
            })
            .inner;
        if paste.clicked() {
            actions.paste = true;
        }
        // Copy offers the selected block or the whole chain.
        let copy = ui
            .add_enabled_ui(!comparing, |ui| {
                icons::toolbar_button(
                    ui,
                    Icon::Copy,
                    "Copy",
                    "Copy the selected block or the whole chain, to paste into another place, \
                     track, part or the master",
                )
            })
            .inner;
        egui::Popup::menu(&copy).show(|ui| {
            let block = ui
                .add_enabled_ui(clip.block_selected, |ui| {
                    icon_menu_item(
                        ui,
                        Icon::Copy,
                        "Copy block",
                        "Copy the block selected on the board",
                    )
                })
                .inner;
            if block.clicked() {
                actions.copy_block = true;
                ui.close();
            }
            if menu_row(
                ui,
                "Copy chain",
                "Copy every block of this chain",
                |_, _, _| {},
            )
            .clicked()
            {
                actions.copy_chain = true;
                ui.close();
            }
        });
        let Some(tone) = tone else {
            return;
        };
        ui.add_space(8.0);
        ui.add_enabled_ui(!comparing, |ui| {
            if icons::toolbar_button(
                ui,
                Icon::Import,
                "Import tone",
                "Import a tone from a file (.json) into your saved tones, and play it on this \
                 track",
            )
            .clicked()
            {
                actions.import_tone = true;
            }
            if icons::toolbar_button(
                ui,
                Icon::Export,
                "Export tone",
                "Export this tone to a file (.json) to share it: the strings' sound, the chain \
                 and the impulse responses it loads",
            )
            .clicked()
            {
                actions.export_tone = Some(tone.name.trim_end_matches(" (changed)").to_owned());
            }
        });
        ui.add_space(8.0);
        let button = ui
            .add_enabled(
                tone.compare || tone.comparing,
                egui::Button::new("A/B").selected(tone.comparing),
            )
            .on_hover_text(if tone.comparing {
                "You hear and see the tone as it was when this window opened: click to go back \
                 to the tone as it is now"
            } else {
                "Click to hear and see the tone as it was when this window opened; click again \
                 to go back to your changes"
            })
            .on_disabled_hover_text("The tone is as it was when this window opened");
        ui.ctx().accesskit_node_builder(button.id, |node| {
            node.set_label("Compare with the tone as it was");
            node.set_toggled(if tone.comparing {
                egui::accesskit::Toggled::True
            } else {
                egui::accesskit::Toggled::False
            });
        });
        if button.clicked() {
            actions.toggle_compare = true;
        }
    });
}

/// The saved tones: the one this track plays, saving it (over it or under a new name),
/// choosing another from the library and deleting. A tone is the whole chain plus the
/// string sound it was saved with, and the kind of string it belongs to. (Called tones so
/// they are not mistaken for the track's preset, its General MIDI program; the settings
/// still call them presets.) The table is the same one the track panel uses.
fn preset_row(
    ui: &mut egui::Ui,
    shown: ShownPreset,
    default_tone: Option<&str>,
    tone: &Tone,
    presets: &[ChainPreset],
    window: &mut SoundChainWindow,
    actions: &mut Actions,
) {
    let (program, kind, clip) = (shown.program, shown.kind, shown.clip);
    let current =
        super::tone_picker::current_tone_choice(tone, shown.chosen.as_ref(), default_tone, presets);
    // The saved tone being worked on, if one was chosen or saved for this preset.
    let working = window
        .preset
        .as_ref()
        .filter(|(saved, _)| *saved == program)
        .map(|(_, name)| name.clone());
    // Changed: the tone is neither a built-in one nor a saved one as it was saved.
    let changed = current.is_none()
        || matches!(
            current,
            Some(
                super::tone_picker::PickerChoice::Program(_)
                    | super::tone_picker::PickerChoice::DrumKit(_)
            )
        );
    let shown_name = super::tone_picker::tone_display_name(
        current.as_ref(),
        working.as_deref().or(shown.basis.as_deref()),
    );
    let built_in = matches!(
        (&current, default_tone),
        (Some(super::tone_picker::PickerChoice::BuiltIn(name)), Some(default))
            if name == default
    );
    // Built-in tones by family, then the user's saved ones; compact here so the window
    // keeps its height (the track panel shows the full table).
    let mut chosen: Option<super::tone_picker::PickerChoice> = None;
    // On a narrow screen the row wraps, and the copy, paste and A/B buttons take a line of
    // their own.
    let narrow = ui.available_width() < WIDTH;
    let combo_width = if narrow {
        (ui.available_width() - 130.0).clamp(150.0, 230.0)
    } else {
        230.0
    };
    header_row(ui, narrow, |ui| {
        // While A/B shows the tone as it was, the dropdown names it but cannot change it.
        let combo = ui
            .add_enabled_ui(!shown.comparing, |ui| {
                egui::ComboBox::from_id_salt("sound_chain_tone")
                    .width(combo_width)
                    .selected_text(shown_name.clone())
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(false, "Built-in").clicked() {
                            window.preset = None;
                            actions.reset_tone = true;
                        }
                        // The user's saved tones first (the list is long), then the built-in ones.
                        if !presets.is_empty() {
                            ui.label(RichText::new("Saved").weak());
                            for preset in presets {
                                let selected = preset.name == shown_name;
                                if ui.selectable_label(selected, &preset.name).clicked() {
                                    chosen = Some(super::tone_picker::PickerChoice::Saved {
                                        kind: preset.kind.clone(),
                                        name: preset.name.clone(),
                                    });
                                }
                            }
                        }
                        for family in crate::tone_library::ToneFamily::ALL {
                            ui.label(RichText::new(family.label()).weak());
                            for tone in crate::tone_library::all()
                                .iter()
                                .filter(|tone| tone.family == family)
                            {
                                let selected = tone.name == shown_name;
                                let response = ui
                                    .selectable_label(selected, &tone.name)
                                    .on_hover_text(&tone.description);
                                if response.clicked() {
                                    chosen = Some(super::tone_picker::PickerChoice::BuiltIn(
                                        tone.name.clone(),
                                    ));
                                }
                            }
                        }
                    })
            })
            .inner;
        combo.response.clone().on_hover_text(format!(
            "The tone this track's preset ({}: {}) plays; {}",
            program,
            gscore_model::program_name(program),
            match default_tone {
                Some(name) => format!("its built-in one is {name}"),
                None => "it has no built-in one, so without a tone it plays its plain strings"
                    .to_owned(),
            }
        ));
        ui.ctx().accesskit_node_builder(combo.response.id, |node| {
            node.set_label("Tone");
            node.set_value(shown_name.clone());
        });
        ui.add_enabled_ui(!shown.comparing, |ui| {
            tone_buttons(
                ui, program, kind, changed, built_in, working, window, actions,
            );
        });
        if narrow {
            ui.end_row();
        }
        clip_buttons(
            ui,
            clip,
            Some(ToneRowButtons {
                compare: shown.compare,
                comparing: shown.comparing,
                name: &shown_name,
            }),
            actions,
        );
    });
    if let Some(choice) = chosen {
        use super::tone_picker::PickerChoice;
        match choice {
            PickerChoice::BuiltIn(name) => {
                if let Some(built) = crate::tone_library::find(&name) {
                    window.preset = None;
                    actions.preset = Some(ChosenPreset {
                        kind: built.tone.kind,
                        sound: built.tone.sound,
                        chain: built.tone.chain.clone(),
                        engine: Some(TrackEngine::Modelled(built.tone.kind)),
                        choice: PickerChoice::BuiltIn(name.clone()),
                    });
                }
            }
            PickerChoice::Saved {
                kind: saved_kind,
                name,
            } => {
                if let Some(preset) = presets
                    .iter()
                    .find(|preset| preset.kind == saved_kind && preset.name == name)
                {
                    window.preset = Some((program, preset.name.clone()));
                    actions.preset = Some(ChosenPreset::of_saved(preset, kind));
                }
            }
            PickerChoice::Program(_) | PickerChoice::DrumKit(_) => {}
        }
    }
}

/// One of the window's header rows: one line, or on a narrow screen wrapping onto the
/// next.
fn header_row<R>(ui: &mut egui::Ui, narrow: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if narrow {
        ui.horizontal_wrapped(add).inner
    } else {
        ui.horizontal(add).inner
    }
}

/// Two columns side by side, their tops lined up; on a narrow screen one under the other.
fn side_by_side<R>(ui: &mut egui::Ui, narrow: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if narrow {
        ui.vertical(add).inner
    } else {
        ui.horizontal_top(add).inner
    }
}

/// What a Copy in a chain's menu put on the app's clipboard: one block or the whole chain.
#[derive(Debug, Clone)]
pub(super) enum SoundClip {
    Block(BlockSetting),
    Chain(Box<ChainSetting>),
}

/// A block being dragged on a chain board. Its block travels with it, so it can be dropped
/// on another board (the master's, another track's strip) as a copy.
#[derive(Debug, Clone)]
pub(super) struct DraggedBlock {
    /// Which board the drag started on (a track/part number, or the master's). A drop on
    /// the same board moves the block; another board gets a copy.
    pub(super) origin: usize,
    /// The place it came from (0 to 4).
    pub(super) place: usize,
    /// The block itself.
    pub(super) block: BlockSetting,
}

/// The origin of the master chain's board (no track can have this number).
pub(super) const MASTER_BOARD: usize = usize::MAX;

/// The origin number of a track's or drum part's board (they never collide with
/// [`MASTER_BOARD`]).
pub(super) fn board_origin(track: usize, part: Option<DrumPart>) -> usize {
    track
        .saturating_mul(8)
        .saturating_add(part.map_or(7, DrumPart::index))
}

/// What a board has besides its blocks.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BoardExtras {
    /// The input trim's and the output's boxes (`None`: a board without them, the
    /// master's).
    pub(super) ends: Option<ChainEnds>,
    /// An empty place's menu offers a plugin (M24, a track's own chain).
    pub(super) plugins: bool,
}

/// What the board asked for.
#[derive(Default)]
pub(super) struct BoardActions {
    /// The box clicked: 0 the strings, 1 to 5 a place.
    pub(super) select: Option<usize>,
    /// An empty box's effect was chosen: the place gets it.
    pub(super) fill: Option<(usize, BlockChoice)>,
    /// A block's menu asked for it: its place is emptied.
    pub(super) remove: Option<usize>,
    /// The mute under a place was clicked: its bypass flips.
    pub(super) bypass: Option<usize>,
    /// A block was dropped on another place of this board: (from, to), both places.
    pub(super) moved: Option<(usize, usize)>,
    /// A block or chain to put on the clipboard.
    pub(super) copy: Option<SoundClip>,
    /// The clipboard's block goes into this place.
    pub(super) paste_block: Option<usize>,
    /// The clipboard's chain replaces this whole board.
    pub(super) paste_chain: bool,
    /// A block dragged here from another board: the place and the block.
    pub(super) dropped: Option<(usize, BlockSetting)>,
}

/// The first empty place of `chain`, if there is one.
pub(super) fn first_empty(chain: &ChainSetting) -> Option<usize> {
    chain
        .blocks
        .iter()
        .position(|block| block.choice == BlockChoice::Empty)
}

/// Moves the block at place `from` to place `to`, the places in between sliding one place
/// toward `from` - the order the sound passes them in follows the board. False when
/// nothing moves: the same place, or a place off the board.
pub(super) fn move_block(blocks: &mut [BlockSetting], from: usize, to: usize) -> bool {
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

/// The row of a drum kit's parts: the kit itself (the track's own chain, the drum bus) and
/// each part, which the board then edits; "Built-in" forgets a part's own tone.
fn parts_row(
    ui: &mut egui::Ui,
    part: &mut Option<DrumPart>,
    selected: &mut usize,
    dialed_in: &[DrumPart],
    actions: &mut Actions,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Part").color(style::tokens(ui).text_weak));
        let kit = part.is_none();
        let kit_button = ui
            .add(egui::Button::selectable(kit, "Kit"))
            .on_hover_text("The drum bus: the whole kit's chain, after the parts");
        if let Some(drag) = kit_button.dnd_release_payload::<DraggedBlock>() {
            *part = None;
            *selected = 1;
            actions.pasted_part = Some((None, drag.block.clone()));
        }
        if kit_button.clicked() && !kit {
            *part = None;
            *selected = 1;
        }
        for candidate in DrumPart::ALL {
            let chosen = *part == Some(candidate);
            let button = ui
                .add(egui::Button::selectable(chosen, candidate.label()))
                .on_hover_text(format!(
                    "The {}'s own chain, before the kit's: EQ, a compressor, maybe a room",
                    candidate.label()
                ));
            if let Some(drag) = button.dnd_release_payload::<DraggedBlock>() {
                *part = Some(candidate);
                *selected = 1;
                actions.pasted_part = Some((Some(candidate), drag.block.clone()));
            }
            if button.clicked() && !chosen {
                *part = Some(candidate);
                *selected = 1;
            }
        }
        // A part with a tone of its own can go back to the built-in one.
        if let Some(part) = *part
            && dialed_in.contains(&part)
        {
            ui.add_space(6.0);
            if super::icons::button(
                ui,
                super::icons::Icon::Revert,
                "Built-in tone",
                "Back to the built-in tone (forgets this part's own tone)",
            )
            .clicked()
            {
                actions.reset_part = true;
            }
        }
        // The one room the whole kit shares (the parts' and the kit's Room knobs send into
        // it), with its own knobs.
        ui.add_space(8.0);
        if ui
            .button("Kit room…")
            .on_hover_text(
                "The one short room behind the whole kit: its size, decay and tone. The \
                 parts' and the kit's Room knobs decide how much reaches it",
            )
            .clicked()
        {
            actions.open_room = true;
        }
    });
}

/// One row of a menu of blocks: the block's symbol (as on the board) and its name.
pub(super) fn block_menu_item(
    ui: &mut egui::Ui,
    choice: BlockChoice,
    label: &str,
    tip: &str,
) -> egui::Response {
    menu_row(ui, label, tip, |painter, rect, color| {
        super::block_icons::paint(painter, rect, choice, color);
    })
}

/// One row of a board's menu with one of the app's icons (copy, paste) beside its name, in
/// the same column as the blocks' symbols.
fn icon_menu_item(
    ui: &mut egui::Ui,
    icon: super::icons::Icon,
    label: &str,
    tip: &str,
) -> egui::Response {
    menu_row(ui, label, tip, |painter, rect, color| {
        super::icons::paint(painter, rect, color, icon);
    })
}

/// A menu row: a symbol drawn by `symbol` in an 18-point square on the left, then `label`.
fn menu_row(
    ui: &mut egui::Ui,
    label: &str,
    tip: &str,
    symbol: impl FnOnce(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let height = ui.spacing().interact_size.y.max(22.0);
    // A fixed width: a menu grows to whatever its rows ask for.
    let width = 150.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::Role::Button);
        node.set_label(label);
    });
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
    }
    let square = Rect::from_center_size(
        pos2(rect.left() + 4.0 + 9.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    symbol(ui.painter(), square, visuals.text_color());
    ui.painter().text(
        pos2(square.right() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        visuals.text_color(),
    );
    response.on_hover_text(tip)
}

/// What an empty place can hold: the block, the name its menu shows, and what it does.
pub(super) fn block_choices() -> [(BlockChoice, &'static str, &'static str); 9] {
    [
        (
            BlockChoice::Drive,
            "Drive",
            "An overdrive pedal: in front of an amp it tightens the lows and pushes the mids",
        ),
        (
            BlockChoice::Amp,
            "Amp",
            "A computed guitar amp: clean, crunch or lead, with a tone stack. It plays direct: put \
             a Cab block after it for its speaker",
        ),
        (
            BlockChoice::Cabinet,
            "Cab",
            "A cabinet or a guitar's body: built in Rock (a 4×12 after a Marshall 1960A), \
             Acoustic (a dreadnought's body) or Classical, or an impulse response from a WAV \
             file",
        ),
        (
            BlockChoice::Eq,
            "EQ",
            "A 4-band parametric EQ: a low cut, a low shelf, two bells and a high shelf, \
             then a high cut",
        ),
        (
            BlockChoice::Chorus,
            "Chorus",
            "A stereo chorus: two short delays swinging slowly, mixed under the dry sound",
        ),
        (
            BlockChoice::Delay,
            "Delay",
            "A digital delay: echoes at a note value synced to the song's tempo, or free",
        ),
        (
            BlockChoice::Compressor,
            "Compressor",
            "A FET compressor (CA-76): tames peaks, thickens a part, and its meter shows how \
             far it works",
        ),
        (
            BlockChoice::Room,
            "Room",
            "A convolution reverb: the built-in short room, or a room's impulse response from \
             a WAV file (stereo if recorded so)",
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
    /// The block's symbol, drawn above its name.
    icon: Option<BlockChoice>,
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
    if face.empty {
        inside.text(
            rect.center(),
            Align2::CENTER_CENTER,
            short(face.title, 13),
            FontId::proportional(22.0),
            text,
        );
        return;
    }
    // The symbol at the top, then the name, then what it is set to.
    let narrow = rect.width() < BOX.x;
    let (title_y, detail_y) = match face.icon {
        Some(choice) => {
            let icon =
                Rect::from_center_size(pos2(rect.center().x, rect.top() + 18.0), vec2(22.0, 22.0));
            super::block_icons::paint(&inside, icon, choice, text);
            (rect.top() + 41.0, rect.top() + 57.0)
        }
        None => (rect.center().y - 8.0, rect.center().y + 12.0),
    };
    inside.text(
        pos2(rect.center().x, title_y),
        Align2::CENTER_CENTER,
        short(face.title, if narrow { 5 } else { 12 }),
        FontId::proportional(if narrow { 12.0 } else { 13.0 }),
        text,
    );
    if !face.detail.is_empty() {
        inside.text(
            pos2(rect.center().x, detail_y),
            Align2::CENTER_CENTER,
            short(face.detail, if narrow { 7 } else { 12 }),
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

/// The levels at a track chain's two ends, shown as narrow boxes on its board: the input
/// trim before the first block and the chain's output after the last.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChainEnds {
    pub(super) input_db: f32,
    pub(super) output_db: f32,
}

/// What stands where on a board, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// A box: 0 the source, 1 to 5 a place.
    Box(usize),
    Input,
    Output,
}

/// A small button with an icon, in a rect of its own (the trash can under a block).
fn small_icon_button(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: super::icons::Icon,
    label: &str,
    tip: &str,
) -> egui::Response {
    let tokens = style::tokens(ui);
    let response = ui
        .interact(
            rect,
            ui.scope_id().with(("small_icon", label)),
            Sense::click(),
        )
        .on_hover_text(tip);
    let fill = if response.hovered() {
        tokens.control_hover
    } else {
        tokens.control
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    let side = rect.height() - 4.0;
    super::icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(side, side)),
        tokens.text_weak,
        icon,
    );
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_role(egui::Role::Button);
    });
    response
}

/// The board on a dark ground: the source, the input trim's narrow box, the five places
/// and the output's narrow box, joined by cables; under each block its mute and a trash
/// can. A block can be dragged to another place; the block follows the mouse and the
/// place it would land in is outlined, until it is let go there. `ends` is `None` for a
/// board without them (the master's). Where the row does not fit the width there is
/// (narrower than the window's usual width), the board folds into two rows.
pub(super) fn board(
    ui: &mut egui::Ui,
    source: (&str, &str),
    chain: &ChainSetting,
    selected: usize,
    origin: usize,
    clipboard: Option<&SoundClip>,
    extras: BoardExtras,
) -> BoardActions {
    let BoardExtras { ends, plugins } = extras;
    let tokens = style::tokens(ui);
    let count = BLOCKS.saturating_add(1);
    // The slots left to right, with their widths.
    let mut slots = vec![(Slot::Box(0), BOX.x)];
    if ends.is_some() {
        slots.push((Slot::Input, END_WIDTH));
    }
    slots.extend((1..count).map(|index| (Slot::Box(index), BOX.x)));
    if ends.is_some() {
        slots.push((Slot::Output, END_WIDTH));
    }
    let width = ui.available_width().min(WIDTH);
    let layout = board_layout(&slots, width);
    let height = if layout.wrap.is_some() {
        BOARD_HEIGHT + ROW_STEP
    } else {
        BOARD_HEIGHT
    };
    let (area, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let placed: Vec<(Slot, Rect)> = slots
        .iter()
        .zip(&layout.rects)
        .map(|(&(slot, _), rect)| (slot, rect.translate(area.min.to_vec2())))
        .collect();
    let rect_of = |index: usize| {
        placed
            .iter()
            .find(|(slot, _)| *slot == Slot::Box(index))
            .map_or(Rect::NOTHING, |(_, rect)| *rect)
    };
    let painter = ui.painter_at(area);
    // The dark ground the pedals stand on.
    painter.rect_filled(area, 8.0, tokens.chrome);
    // Cables first, under the boxes: a straight line from one box's output jack to the next
    // box's input jack.
    let cable = Stroke::new(2.5, tokens.text_weak.gamma_multiply(0.8));
    for (at, pair) in placed.windows(2).enumerate() {
        let [(_, a), (_, b)] = pair else {
            continue;
        };
        let (from, to) = (a.right_center(), b.left_center());
        if layout.wrap == Some(at.saturating_add(1)) {
            // Folded: out of the first row's end, down under it, back along to the second
            // row's start and into it.
            let out = (from.x + layout.gap / 2.0).min(area.right() - 4.0);
            let back = (to.x - layout.gap / 2.0).max(area.left() + 4.0);
            let under = (a.bottom() + 22.0 + b.top()) / 2.0;
            painter.add(egui::Shape::line(
                vec![
                    from,
                    pos2(out, from.y),
                    pos2(out, under),
                    pos2(back, under),
                    pos2(back, to.y),
                    to,
                ],
                cable,
            ));
        } else {
            painter.line_segment([from, to], cable);
        }
        for end in [from, to] {
            painter.circle_filled(end, 3.5, tokens.text_weak);
        }
    }
    let mut actions = BoardActions::default();
    // The input trim's and the output's boxes: a click shows their knob and meter.
    if let Some(ends) = ends {
        for (slot, rect) in &placed {
            let (select, title, db, tip, label) = match slot {
                Slot::Input => (
                    SELECT_INPUT,
                    "In",
                    ends.input_db,
                    "The input trim, before the first block: how hard the chain is fed",
                    "Block in: input trim",
                ),
                Slot::Output => (
                    SELECT_OUTPUT,
                    "Out",
                    ends.output_db,
                    "The chain's output level, after every block and before the fader",
                    "Block out: chain output",
                ),
                Slot::Box(_) => continue,
            };
            let response = ui
                .interact(
                    *rect,
                    ui.scope_id().with(("chain_end", select)),
                    Sense::click(),
                )
                .on_hover_text(tip);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(label);
                node.set_role(egui::Role::Button);
            });
            let fill = if response.hovered() {
                tokens.control_hover
            } else {
                tokens.control
            };
            let outline = if selected == select {
                Stroke::new(2.0, tokens.accent)
            } else {
                Stroke::new(1.0, tokens.line)
            };
            paint_box(
                &painter,
                *rect,
                &BoxFace {
                    empty: false,
                    icon: None,
                    bypassed: false,
                    title,
                    detail: &format!("{db:+.1}"),
                },
                fill,
                outline,
                tokens,
            );
            if response.clicked() {
                actions.select = Some(select);
            }
        }
    }
    // The block being dragged, while one is.
    let dragging = egui::DragAndDrop::payload::<DraggedBlock>(ui.ctx());
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
            if let (Some(place), Some(block)) = (index.checked_sub(1), block) {
                response.dnd_set_drag_payload(DraggedBlock {
                    origin,
                    place,
                    block: block.clone(),
                });
            }
            let held = dragging.as_ref().is_some_and(|drag| {
                drag.origin == origin && index.checked_sub(1) == Some(drag.place)
            });
            response = response.on_hover_cursor(if held {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        let (title, detail) = match block {
            None => (source.0.to_owned(), source.1.to_owned()),
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
            if let Some(drag) = response.dnd_hover_payload::<DraggedBlock>() {
                landing = drag.origin != origin || index.checked_sub(1) != Some(drag.place);
            }
            if let Some(drag) = response.dnd_release_payload::<DraggedBlock>() {
                let place = index.saturating_sub(1);
                if drag.origin == origin {
                    actions.moved = Some((drag.place, place));
                } else {
                    actions.dropped = Some((place, drag.block.clone()));
                }
            }
        }
        let fill = if index == 0 {
            tokens.control_active
        } else if empty {
            tokens.panel
        } else if dragging
            .as_ref()
            .is_some_and(|drag| drag.origin == origin && index.checked_sub(1) == Some(drag.place))
        {
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
                icon: block.map(|block| block.choice),
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
                    if let Some(SoundClip::Block(_)) = clipboard
                        && icon_menu_item(
                            ui,
                            super::icons::Icon::Paste,
                            "Paste block",
                            "Put the copied block in this place",
                        )
                        .clicked()
                    {
                        actions.paste_block = Some(place);
                        ui.close();
                    }
                    for (choice, label, tip) in block_choices() {
                        if block_menu_item(ui, choice, label, tip).clicked() {
                            actions.fill = Some((place, choice));
                            ui.close();
                        }
                    }
                    if plugins
                        && block_menu_item(
                            ui,
                            BlockChoice::Plugin,
                            "Plugin",
                            "A CLAP effect installed on this computer (experimental)",
                        )
                        .clicked()
                    {
                        actions.fill = Some((place, BlockChoice::Plugin));
                        ui.close();
                    }
                });
                // A right-click on an empty place pastes what the clipboard holds, as a
                // filled block's menu does.
                if clipboard.is_some() {
                    egui::Popup::context_menu(&response)
                        .id(response.id.with("empty_place_context"))
                        .show(|ui| {
                            if let Some(SoundClip::Block(_)) = clipboard
                                && icon_menu_item(
                                    ui,
                                    super::icons::Icon::Paste,
                                    "Paste block",
                                    "Put the copied block in this place",
                                )
                                .clicked()
                            {
                                actions.paste_block = Some(place);
                                ui.close();
                            }
                            if let Some(SoundClip::Chain(_)) = clipboard
                                && menu_row(
                                    ui,
                                    "Paste chain",
                                    "Replace every place of this chain with the copied one",
                                    |_, _, _| {},
                                )
                                .clicked()
                            {
                                actions.paste_chain = true;
                                ui.close();
                            }
                        });
                }
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
                    if menu_row(
                        ui,
                        if bypassed { "Hear it again" } else { "Bypass" },
                        tip,
                        |_, _, _| {},
                    )
                    .clicked()
                    {
                        actions.bypass = Some(place);
                        ui.close();
                    }
                    if icon_menu_item(
                        ui,
                        super::icons::Icon::Delete,
                        "Remove",
                        "Empty this place of the board",
                    )
                    .clicked()
                    {
                        actions.remove = Some(place);
                        ui.close();
                    }
                    ui.separator();
                    if let Some(block) = block
                        && icon_menu_item(
                            ui,
                            super::icons::Icon::Copy,
                            "Copy block",
                            "Keep this block to paste into another place, track or part",
                        )
                        .clicked()
                    {
                        actions.copy = Some(SoundClip::Block(block.clone()));
                        ui.close();
                    }
                    if menu_row(
                        ui,
                        "Copy chain",
                        "Keep every block of this chain to paste into another track or part",
                        |_, _, _| {},
                    )
                    .clicked()
                    {
                        actions.copy = Some(SoundClip::Chain(Box::new(chain.clone())));
                        ui.close();
                    }
                    if let Some(SoundClip::Block(_)) = clipboard
                        && icon_menu_item(
                            ui,
                            super::icons::Icon::Paste,
                            "Paste block",
                            "Replace this place with the copied block",
                        )
                        .clicked()
                    {
                        actions.paste_block = Some(place);
                        ui.close();
                    }
                    if let Some(SoundClip::Chain(_)) = clipboard
                        && menu_row(
                            ui,
                            "Paste chain",
                            "Replace every place of this chain with the copied one",
                            |_, _, _| {},
                        )
                        .clicked()
                    {
                        actions.paste_chain = true;
                        ui.close();
                    }
                });
            }
            // Under a block, its mute: the block passes the sound unchanged while it is on.
            if empty {
                continue;
            }
            let mute = Rect::from_center_size(
                pos2(rect.center().x - 13.0, rect.bottom() + 13.0),
                vec2(24.0, 16.0),
            );
            let trash = Rect::from_center_size(
                pos2(rect.center().x + 13.0, rect.bottom() + 13.0),
                vec2(24.0, 16.0),
            );
            if small_icon_button(
                ui,
                trash,
                super::icons::Icon::Delete,
                &format!("Remove block {}", place.saturating_add(1)),
                "Remove this block (empty its place)",
            )
            .clicked()
            {
                actions.remove = Some(place);
            }
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
    // The block being dragged, at the mouse, above the window (drawn by the board it came
    // from, once, so two open boards do not paint it twice).
    if let Some(drag) = &dragging
        && drag.origin == origin
        && let Some(pointer) = ui.ctx().pointer_interact_pos()
    {
        let layer = egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("chain_drag"));
        let painter = ui.ctx().layer_painter(layer);
        let (title, detail) = box_text(&drag.block);
        paint_box(
            &painter,
            Rect::from_center_size(pointer, BOX),
            &BoxFace {
                empty: false,
                icon: Some(drag.block.choice),
                bypassed: drag.block.bypass,
                title: &title,
                detail: &detail,
            },
            tokens.control_active,
            Stroke::new(2.0, tokens.accent),
            tokens,
        );
    }
    // A block's mute and trash buttons are put where they belong, which moves the cursor
    // to them: the window goes on under the whole board, whichever buttons it shows.
    ui.advance_cursor_after_rect(area);
    actions
}

/// Where a board's slots go, relative to the board's top left corner.
#[derive(Debug, Clone, PartialEq)]
struct BoardLayout {
    /// One rectangle per slot, in order.
    rects: Vec<Rect>,
    /// The first slot of the second row, when the board is folded.
    wrap: Option<usize>,
    /// The gap between two boxes in a row.
    gap: f32,
}

/// Lays `slots` (each with its width) out across `width`: in one row with gaps of up to
/// [`GAP`], or, where even the smallest gaps leave the row too wide, in two rows that start
/// at the same edge (the first one holding the larger half).
fn board_layout(slots: &[(Slot, f32)], width: f32) -> BoardLayout {
    let row_width = |row: &[(Slot, f32)], gap: f32| -> f32 {
        let boxes: f32 = row.iter().map(|(_, width)| width).sum();
        boxes + gap * row.len().saturating_sub(1) as f32
    };
    let fits = row_width(slots, MIN_GAP) + 24.0 <= width;
    let rows: Vec<&[(Slot, f32)]> = if fits {
        vec![slots]
    } else {
        let (first, second) = slots.split_at(slots.len().div_ceil(2));
        vec![first, second]
    };
    // One gap for every row, so the boxes of the two rows line up.
    let gap = rows
        .iter()
        .map(|row| {
            let boxes: f32 = row.iter().map(|(_, width)| width).sum();
            let gaps = row.len().saturating_sub(1).max(1) as f32;
            ((width - 24.0 - boxes) / gaps).clamp(MIN_GAP, GAP)
        })
        .fold(GAP, f32::min);
    let widest = rows
        .iter()
        .map(|row| row_width(row, gap))
        .fold(0.0, f32::max);
    let left = ((width - widest) / 2.0).max(0.0);
    let mut rects = Vec::with_capacity(slots.len());
    for (line, row) in rows.iter().enumerate() {
        let top = 10.0 + line as f32 * ROW_STEP;
        let mut x = left;
        for (_, slot_width) in *row {
            rects.push(Rect::from_min_size(pos2(x, top), vec2(*slot_width, BOX.y)));
            x += slot_width + gap;
        }
    }
    BoardLayout {
        rects,
        wrap: rows.get(1).map(|_| rows.first().map_or(0, |row| row.len())),
        gap,
    }
}

/// Where the window goes and how its content is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placement {
    /// Its top left corner.
    pos: Pos2,
    /// The content's width.
    width: f32,
    /// The margin on either side of the content.
    side: i8,
    /// The body's height, where it scrolls (a narrow screen); `None`: as tall as it is.
    body: Option<f32>,
    /// Whether the body fills that height however little it shows (a sheet).
    fill: bool,
    /// How much of the body's bottom the floating drawer hides.
    hidden: f32,
}

impl Placement {
    /// A sheet over `cover` (everything but the drawer): as wide and as tall, the content
    /// no wider than usual and centred, `hidden` of its bottom under the floating drawer.
    fn sheet(cover: Rect, hidden: f32) -> Placement {
        // The window's outline, one point on either side.
        let inside = cover.width() - 2.0;
        let side = ((inside - WIDTH) / 2.0).clamp(16.0, 120.0).floor();
        Placement {
            pos: cover.min,
            width: (inside - 2.0 * side).max(MIN_WIDTH),
            side: side as i8,
            body: Some(
                (cover.height() - super::title_bar::TITLE_BAR_HEIGHT - 24.0 - 2.0)
                    .max(EDITOR_MIN_HEIGHT),
            ),
            fill: true,
            hidden: hidden.max(0.0),
        }
    }
}

/// Whether the window is a sheet over `cover` (the practice view's room above the drawer):
/// on a narrow screen, and on one not tall enough for it (a phone on its side).
pub(super) fn is_sheet(cover: Rect) -> bool {
    content_width(cover.width()) < WIDTH || cover.height() < SHEET_BELOW_HEIGHT
}

/// The width of the window's content on a screen `screen` points wide: its usual width,
/// or on a narrower screen (a phone) what is left between the edges.
fn content_width(screen: f32) -> f32 {
    (screen - 2.0 * SCREEN_GUTTER - FRAME_WIDTH).clamp(MIN_WIDTH, WIDTH)
}

/// What was picked in a Cab or Room block's dropdown.
pub(super) enum ResponseChoice {
    /// One of the built-in cabinets.
    Cabinet(CabinetSetting),
    /// The built-in room.
    Room,
    /// A WAV file, from the file dialog.
    Load,
}

/// A Cab or Room block's dropdown: its built-in responses (the cabinets, or the one room),
/// then loading a file; the file's name while one is loaded.
pub(super) fn response_choice(ui: &mut egui::Ui, block: &BlockSetting) -> Option<ResponseChoice> {
    const BUILT_IN_ROOM: &str = "Built-in room";
    let room = block.choice == BlockChoice::Room;
    let mut picked = None;
    let shown = block.file.as_ref().map_or_else(
        || {
            if room {
                BUILT_IN_ROOM.to_owned()
            } else {
                block.cabinet.label().to_owned()
            }
        },
        |file| {
            file.file_name().map_or_else(
                || file.display().to_string(),
                |name| short(&name.to_string_lossy(), 40),
            )
        },
    );
    ui.horizontal(|ui| {
        let combo = egui::ComboBox::from_id_salt(if room {
            "room response"
        } else {
            "cab response"
        })
        .selected_text(shown)
        .width(260.0)
        .show_ui(ui, |ui| {
            if room {
                let chosen = block.file.is_none();
                if ui
                    .selectable_label(chosen, BUILT_IN_ROOM)
                    .on_hover_text(
                        "A short room, computed: the one a drum kit's room starts on. No \
                             file needed.",
                    )
                    .clicked()
                    && !chosen
                {
                    picked = Some(ResponseChoice::Room);
                }
            } else {
                for cabinet in CabinetSetting::ALL {
                    let chosen = block.file.is_none() && block.cabinet == cabinet;
                    if ui
                        .selectable_label(chosen, cabinet.label())
                        .on_hover_text(cabinet.tip())
                        .clicked()
                        && !chosen
                    {
                        picked = Some(ResponseChoice::Cabinet(cabinet));
                    }
                }
            }
            ui.separator();
            if ui
                .selectable_label(false, "Load file…")
                .on_hover_text(if room {
                    "A room recorded in a WAV file (an impulse response)"
                } else {
                    "An impulse response from a WAV file: a cabinet or a body"
                })
                .clicked()
            {
                picked = Some(ResponseChoice::Load);
            }
        });
        ui.ctx().accesskit_node_builder(combo.response.id, |node| {
            node.set_label(if room {
                "Room response"
            } else {
                "Cab response"
            });
        });
        if let Some(file) = &block.file {
            combo.response.on_hover_text(file.display().to_string());
        }
    });
    picked
}

/// The selected place's choices (when empty) or knobs.
pub(super) fn editor(
    ui: &mut egui::Ui,
    chain: &ChainSetting,
    place: usize,
    reduction: f32,
) -> Edited {
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
            match response_choice(ui, block) {
                Some(ResponseChoice::Cabinet(cabinet)) => {
                    edited.cabinet = cabinet;
                    edited.file = None;
                    changed = true;
                }
                Some(ResponseChoice::Room) => {
                    edited.file = None;
                    changed = true;
                }
                Some(ResponseChoice::Load) => load = true,
                None => {}
            }
        }
        ui.add_space(4.0);
        let moved = block_knobs(ui, &mut edited, false, reduction);
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
    reduction: f32,
) -> KnobChanges {
    let mut moved = KnobChanges::default();
    if all_wet {
        block.mix = 1.0;
    }
    // The EQ is a curve with a column of knobs under it, not a row of knobs.
    if block.choice == BlockChoice::Eq {
        moved.changed |= super::eq_editor::eq_knobs(ui, block);
        return moved;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        match block.choice {
            BlockChoice::Empty => {}
            BlockChoice::Amp => moved.changed |= super::amp_knobs::amp_knobs(ui, block),
            BlockChoice::Drive => moved.changed |= super::amp_knobs::drive_knobs(ui, block),
            BlockChoice::Compressor => {
                moved.changed |= super::amp_knobs::compressor_knobs(ui, block, reduction);
            }
            BlockChoice::Eq => {}
            BlockChoice::Chorus => {
                moved.changed |= super::amp_knobs::chorus_knobs(ui, block, all_wet);
            }
            BlockChoice::Delay => moved.changed |= super::delay_knobs::delay_knobs(ui, block),
            // A plugin's parameters have a view of their own (`plugin_view`).
            BlockChoice::Plugin => {}
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
            Some(window) => {
                if window.track != track {
                    window.part = None;
                    window.reference = None;
                    window.comparing = false;
                }
                window.track = track;
            }
            None => {
                self.sound_chain = Some(SoundChainWindow {
                    track,
                    selected: 0,
                    pos: None,
                    saving: None,
                    preset: None,
                    part: None,
                    strings_tab: super::string_sound::StringsTab::default(),
                    reference: None,
                    reference_own: false,
                    comparing: false,
                    programs: super::tone_picker::TonePickerState::new(),
                    board_keys: false,
                    pending_keys: Vec::new(),
                    rect: None,
                    imported: None,
                    plugin_filter: String::new(),
                    plugin_choosing: false,
                });
            }
        }
    }

    /// Before the score's keys: while the Sound chain board has the keyboard, takes the
    /// keys it handles (the arrows, Delete or Backspace, M) out of this frame's input for
    /// the window, so they do not move the score's cursor or delete a note. A click
    /// outside the window gives the keyboard back to the score.
    pub(super) fn take_sound_chain_keys(&mut self, ctx: &egui::Context) {
        let Some(window) = self.sound_chain.as_mut() else {
            return;
        };
        let pressed_outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| window.rect.is_none_or(|rect| !rect.contains(pos)))
        });
        if pressed_outside {
            window.board_keys = false;
        }
        if !window.board_keys || ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
            return;
        }
        let mut taken = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let key = match event {
                    egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } if modifiers.is_none() || *modifiers == egui::Modifiers::SHIFT => {
                        let key = match key {
                            egui::Key::ArrowLeft => Some(BoardKey::Previous),
                            egui::Key::ArrowRight => Some(BoardKey::Next),
                            egui::Key::Delete | egui::Key::Backspace => Some(BoardKey::Remove),
                            egui::Key::M => Some(BoardKey::Mute),
                            _ => None,
                        };
                        match key {
                            // Only the press acts; its release is taken too.
                            Some(key) if *pressed => Some(Some(key)),
                            Some(_) => Some(None),
                            None => None,
                        }
                    }
                    // The character M types is the key's, not the score's.
                    egui::Event::Text(text) if text.eq_ignore_ascii_case("m") => Some(None),
                    _ => None,
                };
                match key {
                    Some(Some(key)) => {
                        taken.push(key);
                        false
                    }
                    Some(None) => false,
                    None => true,
                }
            });
        });
        if !taken.is_empty() {
            window.pending_keys.extend(taken);
            ctx.request_repaint();
        }
    }

    /// Writes `track`'s tone (as `kind`'s strings, with what plays them) into a tone file
    /// named `name`, chosen in a save dialog.
    fn export_track_tone(
        &mut self,
        track: usize,
        kind: StringKind,
        name: String,
        ctx: &egui::Context,
    ) {
        let tone = self.track_tone(track, kind);
        let engine = self
            .track_player(track)
            .map_or(TrackEngine::Modelled(kind), |player| player.engine);
        let name = if name.trim().is_empty() || name == "Unsaved tone" {
            "My tone".to_owned()
        } else {
            name
        };
        let preset = ChainPreset {
            name: name.clone(),
            kind: kind_id(tone.kind).to_owned(),
            sound: tone.sound,
            chain: tone.chain,
            engine: engine_id(engine).to_owned(),
        };
        match crate::tone_file::write(&preset) {
            Ok((bytes, unreadable)) => {
                if !unreadable.is_empty() {
                    self.notice = Some(format!(
                        "The tone is exported without {}: the file could not be read, so the \
                         block needs it on the other machine too.",
                        unreadable.join(", ")
                    ));
                }
                if let Err(error) = self.tasks.export_tone(name, bytes, ctx) {
                    self.notice = Some(error);
                }
            }
            Err(error) => self.notice = Some(format!("Not exported: {error}")),
        }
    }

    /// What the Import tone dialog does with the file it read (a tone file's bytes (JSON)):
    /// its tone joins the saved tones (under a name of its own if another saved tone has
    /// its name already) and, while the Sound chain window is open, plays on its track.
    /// Its impulse responses are kept in an `impulses` folder beside the settings file.
    pub fn import_tone_file(&mut self, bytes: &[u8]) {
        let dir = self
            .settings_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(|dir| dir.join("impulses"));
        let imported = match crate::tone_file::read(bytes, dir.as_deref()) {
            Ok(imported) => imported,
            Err(error) => {
                self.notice = Some(format!("Not imported: {error}"));
                return;
            }
        };
        let mut preset = imported.preset;
        // A sound written before pickups existed gets its kind's, as the track will play
        // it, so the saved tone and the one playing are the same.
        if let Some(kind) = crate::settings::kind_from_id(&preset.kind) {
            preset.sound = preset.sound.for_kind(kind);
        }
        let presets = &mut self.settings.string_presets;
        // The same tone imported again is the one already saved; another tone of that name
        // gets a number.
        let same = presets.iter().any(|old| {
            old.kind == preset.kind
                && old.name == preset.name
                && old.sound == preset.sound
                && old.chain == preset.chain
        });
        if !same {
            let base = preset.name.clone();
            let mut number = 2_u32;
            while presets
                .iter()
                .any(|old| old.kind == preset.kind && old.name == preset.name)
            {
                preset.name = format!("{base} ({number})");
                number = number.saturating_add(1);
            }
            if presets.len() >= MAX_STRING_PRESETS {
                self.notice = Some(format!(
                    "Not imported: there are {MAX_STRING_PRESETS} saved tones already; delete one \
                     first"
                ));
                return;
            }
            presets.push(preset.clone());
            self.playback
                .store_string_presets(self.settings.string_presets.clone());
            self.save_settings();
        }
        self.status = Some(format!("Imported the tone “{}”", preset.name));
        if !imported.missing.is_empty() {
            self.notice = Some(format!(
                "The tone “{}” was imported without {}: its blocks need those impulse \
                 responses, which the file did not bring.",
                preset.name,
                imported.missing.join(", ")
            ));
        }
        if let Some(window) = &mut self.sound_chain {
            window.imported = Some(preset);
        }
    }

    /// Lets go of A/B: the compared track plays its own tone again.
    fn stop_comparing(&mut self) {
        if self.compare_tone.take().is_some() {
            self.sync_track_tones();
        }
    }

    /// Opens the Sound chain window for `track`, showing `place` of its chain (0 the first
    /// place after the strings) selected, so its knobs are in front of the user.
    pub(super) fn open_sound_chain_at(&mut self, track: usize, place: usize) {
        self.open_sound_chain(track);
        if let Some(window) = &mut self.sound_chain {
            window.part = None;
            window.selected = place.saturating_add(1).min(BLOCKS);
        }
    }

    /// Opens the Sound chain window for `track`, showing `part` of a drum kit (its own
    /// board and level); from the mixer's part strip.
    pub(super) fn open_sound_chain_part(&mut self, track: usize, part: DrumPart) {
        self.open_sound_chain(track);
        if let Some(window) = &mut self.sound_chain {
            window.part = Some(part);
            window.selected = 1;
        }
    }

    /// The impulse response chosen for place `place` of `track`'s chain (as `kind`): read
    /// (an error is shown) and heard at once.
    pub(super) fn use_impulse(
        &mut self,
        track: usize,
        kind: StringKind,
        place: usize,
        part: Option<usize>,
        path: PathBuf,
    ) {
        if let Err(error) = self.playback.impulse(&path) {
            self.notice = Some(error);
            return;
        }
        if let Some(part) = part.and_then(DrumPart::at) {
            let mut own = self.track_drum_part(track, part);
            if let Some(block) = own.chain.blocks.get_mut(place) {
                block.file = Some(path);
            }
            self.set_track_drum_part(track, part, Some(own), false);
            return;
        }
        let mut tone = self.track_tone(track, kind);
        if let Some(block) = tone.chain.blocks.get_mut(place) {
            block.file = Some(path);
        }
        self.set_track_tone(track, tone);
    }

    /// The Sound chain window, while it is open.
    pub(super) fn show_sound_chain(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.sound_chain.take() else {
            self.stop_comparing();
            return;
        };
        // A change applied while the window is drawn (a knob, a block, a source) must be
        // seen on the next frame: the board and the title are already drawn with the old
        // state. Ask for that frame; without input after a single click egui would wait.
        let revision = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.editor().revision());
        // On a narrow screen (a phone) the window is as wide as the screen leaves, starts
        // near the top and stays on the screen; what does not fit below scrolls.
        let screen = ctx.content_rect();
        // In the practice view on a phone (or a phone on its side) the window is a sheet:
        // it covers everything but the drawer, which stays on top of it.
        let sheet = self.practice_sheet();
        let place = match sheet {
            Some((cover, hidden)) => Placement::sheet(cover, hidden),
            None => {
                // On a narrow screen (a phone) the window is as wide as the screen leaves,
                // starts near the top and stays on the screen; what does not fit below
                // scrolls.
                let width = content_width(screen.width());
                let narrow = width < WIDTH;
                let size = vec2(width + FRAME_WIDTH, 600.0);
                let pos = *window.pos.get_or_insert_with(|| {
                    if narrow {
                        pos2(screen.left() + SCREEN_GUTTER, screen.top() + SCREEN_GUTTER)
                    } else {
                        style::centered_window_pos(ctx, size)
                    }
                });
                let pos = if narrow {
                    let kept = pos2(
                        pos.x
                            .clamp(screen.left(), (screen.right() - size.x).max(screen.left())),
                        pos.y
                            .clamp(screen.top(), (screen.bottom() - 120.0).max(screen.top())),
                    );
                    window.pos = Some(kept);
                    kept
                } else {
                    pos
                };
                // The body's height on a narrow screen: down to the screen's bottom, below
                // the title bar and the margins.
                let body = (screen.bottom()
                    - pos.y
                    - super::title_bar::TITLE_BAR_HEIGHT
                    - 24.0
                    - SCREEN_GUTTER)
                    .max(EDITOR_MIN_HEIGHT);
                Placement {
                    pos,
                    width,
                    side: 16,
                    body: narrow.then_some(body),
                    fill: false,
                    hidden: 0.0,
                }
            }
        };
        let (pos, width) = (place.pos, place.width);
        let narrow = width < WIDTH;
        let mut actions = Actions::default();
        let Some(player) = self.track_player(window.track) else {
            return;
        };
        // A tone imported from a file plays on this track at once, as a saved tone chosen
        // in the list would.
        if let Some(preset) = window.imported.take() {
            let kind = match player.engine {
                TrackEngine::Modelled(kind) => kind,
                TrackEngine::SoundFont | TrackEngine::MidiOut => {
                    player.preset_kind.unwrap_or(StringKind::Steel)
                }
            };
            window.preset = Some((player.program, preset.name.clone()));
            window.part = None;
            actions.preset = Some(ChosenPreset::of_saved(&preset, kind));
        }
        // The chain of the kind that plays the track; for the SoundFont, the kind its preset
        // stands for (the mixer runs a SoundFont track's chain too).
        let (kind, modelled) = match player.engine {
            TrackEngine::Modelled(kind) => (kind, true),
            TrackEngine::SoundFont | TrackEngine::MidiOut => {
                (player.preset_kind.unwrap_or(StringKind::Steel), false)
            }
        };
        let ports = self.playback.ports().to_vec();
        // Every preset, whatever kind of string it was saved for (choosing one also
        // switches the track to that kind when it names a modelled engine).
        let mut presets: Vec<ChainPreset> = self.settings.string_presets.clone();
        presets.sort_by_cached_key(|preset| preset.name.to_lowercase());
        // The title names the track (the MIDI box shows its preset, the General MIDI
        // program); the saved tones are the list at the bottom.
        let title = format!("Sound chain — {}", self.track_name(window.track));
        // This track's tone: its song's, or its program's default.
        // On the SoundFont only a tone the song carries brings a chain (the defaults are
        // the modelled strings'); a drum track's own chain is its bus.
        let tone = self.track_chain_tone(window.track, &player, kind);
        // What A/B compares with: the tone as the window found it.
        if window.reference.is_none() {
            window.reference = Some(tone.clone());
            window.reference_own = self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.track_sound(window.track).tone.is_some());
        }
        // A/B has something to compare when the tone changed since (not for a drum part's
        // own chain); while it is on, the window shows the tone as it was.
        let can_compare = window.part.is_none()
            && window
                .reference
                .as_ref()
                .is_some_and(|reference| *reference != tone);
        if !can_compare {
            window.comparing = false;
        }
        let comparing = window.comparing;
        let tone = match (&window.reference, comparing) {
            (Some(reference), true) => reference.clone(),
            _ => tone,
        };
        if comparing {
            // The board's keys would edit the tone shown, which is not the one to change.
            window.pending_keys.clear();
        }
        // How far the track's compressors work, for the block's meter (and a repaint while
        // the needle moves).
        let meters = self.playback.meters();
        let reduction = meters
            .as_ref()
            .map_or(0.0, |meters| meters.reduction(window.track));
        if meters.as_ref().is_some_and(|meters| meters.moving()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        // A drum kit: the board shows the part chosen in the row (or the track's own chain,
        // the drum bus), and the parts the song gives a tone of their own.
        let part_chain = window
            .part
            .map(|part| self.track_drum_part(window.track, part));
        let part_level = part_chain.as_ref().map(|own| own.level);
        let part_room = part_chain.as_ref().map(|own| own.room);
        let dialed_in: Vec<DrumPart> = self
            .tabs
            .get(self.active)
            .map(|tab| tab.track_sound(window.track))
            .map(|sound| {
                sound
                    .drum_parts
                    .iter()
                    .filter_map(super::song_state::DrumPartSound::part)
                    .collect()
            })
            .unwrap_or_default();
        let clipboard = self.sound_clipboard.clone();
        // What the header's copy and paste can do: the board shows the drum part's chain or
        // the tone's, and a selected place may hold a block to copy.
        let clip = ClipState {
            clipboard: clipboard
                .as_ref()
                .map(|clip| matches!(clip, SoundClip::Chain(_))),
            block_selected: window
                .selected
                .checked_sub(1)
                .filter(|place| *place < BLOCKS)
                .and_then(|place| {
                    part_chain
                        .as_ref()
                        .map_or(&tone.chain, |own| &own.chain)
                        .blocks
                        .get(place)
                })
                .is_some_and(|block| block.choice != BlockChoice::Empty),
        };
        // The track's input trim and the level before its chain, for setting how hard an
        // amp or a compressor is driven.
        let trim_db = self
            .tabs
            .get(self.active)
            .map_or(0.0, |tab| self.track_trim_db_in(tab, window.track));
        let default_trim = self.default_trim_db(window.track);
        let input = meters
            .as_ref()
            .map_or_else(Default::default, |meters| meters.input(window.track));
        let output = meters
            .as_ref()
            .map_or_else(Default::default, |meters| meters.chain_output(window.track));
        // Plugins are offered in a track's own chain (not a drum part's yet), and what
        // the plugin thread knows of a selected Plugin block is read now: drawing never
        // waits for it.
        let plugins_allowed = self.playback.plugins_enabled() && window.part.is_none();
        let plugin_panel = match (window.part, window.selected.checked_sub(1)) {
            (None, Some(place)) => {
                self.plugin_panel(window.track, &tone.chain, place, window.plugin_choosing)
            }
            _ => None,
        };
        if plugin_panel.is_none() {
            window.plugin_choosing = false;
        }
        if plugin_panel
            .as_ref()
            .is_some_and(super::plugin_view::PluginPanel::waiting)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let shown = egui::Window::new(&title)
            .id(egui::Id::unique(WINDOW_ID))
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
                    .inner_margin(Margin::symmetric(place.side, 12))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let mut body = |ui: &mut egui::Ui| {
                            if player.drums {
                                parts_row(
                                    ui,
                                    &mut window.part,
                                    &mut window.selected,
                                    &dialed_in,
                                    &mut actions,
                                );
                                ui.separator();
                            }
                            // A drum part's level, or the tone: which one this preset plays,
                            // another one, and saving it. Above the board so it stays in reach,
                            // in a slot of its own height so the board does not jump when a
                            // part is chosen.
                            ui.vertical(|ui| {
                                ui.set_min_height(HEADER_HEIGHT);
                                if let Some(part) = window.part {
                                    header_row(ui, narrow, |ui| {
                                        ui.label(
                                            RichText::new(format!("{} level", part.label()))
                                                .color(style::tokens(ui).text_weak),
                                        );
                                        let mut level = part_level.unwrap_or(127);
                                        if ui
                                            .add(egui::Slider::new(&mut level, 0..=127))
                                            .on_hover_text(
                                                "The part's level inside the drum track, after its \
                                             chain (127 is unity)",
                                            )
                                            .changed()
                                        {
                                            actions.part_level = Some(level);
                                        }
                                        ui.add_space(16.0);
                                        ui.label(
                                            RichText::new("Room")
                                                .color(style::tokens(ui).text_weak),
                                        );
                                        let mut room = part_room.unwrap_or(0);
                                        if ui
                                        .add(egui::Slider::new(&mut room, 0..=127))
                                        .on_hover_text(
                                            "How much of the part goes into the kit's one short \
                                             shared room, which glues a kit together (0 is dry)",
                                        )
                                        .changed()
                                    {
                                        actions.part_room = Some(room);
                                    }
                                        if narrow {
                                            ui.end_row();
                                        }
                                        clip_buttons(ui, clip, None, &mut actions);
                                    });
                                } else {
                                    let default_tone = self.playback.track_default_tone_name(
                                        window.track,
                                        player.program,
                                        kind,
                                    );
                                    preset_row(
                                        ui,
                                        ShownPreset {
                                            program: player.program,
                                            kind,
                                            clip,
                                            compare: can_compare,
                                            comparing,
                                            basis: self
                                                .tabs
                                                .get(self.active)
                                                .and_then(|tab| tab.tone_basis.get(&window.track))
                                                .cloned(),
                                            chosen: self
                                                .tabs
                                                .get(self.active)
                                                .and_then(|tab| tab.tone_choice.get(&window.track))
                                                .cloned(),
                                        },
                                        default_tone.as_deref(),
                                        &tone,
                                        &presets,
                                        &mut window,
                                        &mut actions,
                                    );
                                }
                            });
                            ui.separator();
                            // While A/B shows the tone as it was, its board and knobs are
                            // shown but not edited.
                            ui.add_enabled_ui(!comparing, |ui| {
                                let chain = part_chain
                                    .as_ref()
                                    .map(|own| own.chain.clone())
                                    .unwrap_or_else(|| tone.chain.clone());
                                let sound = tone.sound;
                                // The first box names what plays the track: the strings, the MIDI
                                // synth or a hardware port.
                                let source = match player.engine {
                                    TrackEngine::Modelled(kind) => {
                                        ("Strings".to_owned(), kind_name(kind).to_owned())
                                    }
                                    TrackEngine::SoundFont => (
                                        "MIDI".to_owned(),
                                        format!(
                                            "{}: {}",
                                            player.program,
                                            gscore_model::program_name(player.program)
                                        ),
                                    ),
                                    TrackEngine::MidiOut => {
                                        ("MIDI out".to_owned(), player.midi_out.label())
                                    }
                                };
                                let board = board(
                                    ui,
                                    (source.0.as_str(), source.1.as_str()),
                                    &chain,
                                    window.selected,
                                    board_origin(window.track, window.part),
                                    clipboard.as_ref(),
                                    BoardExtras {
                                        ends: Some(ChainEnds {
                                            input_db: trim_db,
                                            output_db: chain.output_db,
                                        }),
                                        plugins: plugins_allowed,
                                    },
                                );
                                let mut board = board;
                                // The board's keys, as the clicks they stand for.
                                for key in std::mem::take(&mut window.pending_keys) {
                                    let place = window
                                        .selected
                                        .checked_sub(1)
                                        .filter(|place| *place < BLOCKS)
                                        .filter(|place| {
                                            chain.blocks.get(*place).is_some_and(|block| {
                                                block.choice != BlockChoice::Empty
                                            })
                                        });
                                    match key {
                                        BoardKey::Previous | BoardKey::Next => {
                                            board.select = Some(next_box(
                                                board.select.unwrap_or(window.selected),
                                                key == BoardKey::Next,
                                            ));
                                        }
                                        BoardKey::Remove => board.remove = board.remove.or(place),
                                        BoardKey::Mute => board.bypass = board.bypass.or(place),
                                    }
                                }
                                if board.select.is_some() {
                                    window.board_keys = true;
                                }
                                if let Some(index) = board.select {
                                    window.selected = index;
                                    // The first box is the track's source: the kit's, not a part's.
                                    if index == 0 && window.part.is_some() {
                                        window.part = None;
                                    }
                                }
                                if let Some(clip) = board.copy {
                                    actions.copy = Some(clip);
                                }
                                // The header's copy menu: the selected block or the whole chain.
                                let selected_place = window
                                    .selected
                                    .checked_sub(1)
                                    .filter(|place| *place < BLOCKS);
                                if actions.copy_block
                                    && let Some(block) = selected_place
                                        .and_then(|place| chain.blocks.get(place))
                                        .filter(|block| block.choice != BlockChoice::Empty)
                                {
                                    actions.copy = Some(SoundClip::Block(block.clone()));
                                }
                                if actions.copy_chain {
                                    actions.copy = Some(SoundClip::Chain(Box::new(chain.clone())));
                                }
                                // Its paste: whatever was copied last.
                                if actions.paste {
                                    match &clipboard {
                                        Some(SoundClip::Chain(copied)) => {
                                            actions.chain = Some(copied.as_ref().clone());
                                        }
                                        Some(SoundClip::Block(block)) => {
                                            if let Some(place) =
                                                selected_place.or_else(|| first_empty(&chain))
                                            {
                                                let mut changed = chain.clone();
                                                if let Some(slot) = changed.blocks.get_mut(place) {
                                                    *slot = block.clone();
                                                }
                                                actions.chain = Some(changed);
                                                window.selected = place.saturating_add(1);
                                            }
                                        }
                                        None => {}
                                    }
                                }
                                if let Some(place) = board.paste_block
                                    && let Some(SoundClip::Block(block)) = &clipboard
                                {
                                    let mut changed = chain.clone();
                                    if let Some(slot) = changed.blocks.get_mut(place) {
                                        *slot = block.clone();
                                    }
                                    actions.chain = Some(changed);
                                    window.selected = place.saturating_add(1);
                                }
                                if board.paste_chain
                                    && let Some(SoundClip::Chain(copied)) = &clipboard
                                {
                                    actions.chain = Some(copied.as_ref().clone());
                                }
                                if let Some((place, block)) = board.dropped {
                                    let mut changed = chain.clone();
                                    if let Some(slot) = changed.blocks.get_mut(place) {
                                        *slot = block.clone();
                                    }
                                    actions.chain = Some(changed);
                                    window.selected = place.saturating_add(1);
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
                                // The chosen box's knobs, in an area of one height for every box of
                                // the chain (taller only for a chain with an EQ, if the screen has
                                // room), so the window keeps its size when another box is chosen.
                                let tallest = if chain
                                    .blocks
                                    .iter()
                                    .any(|block| block.choice == BlockChoice::Eq)
                                {
                                    EQ_EDITOR_HEIGHT
                                } else {
                                    EDITOR_HEIGHT
                                };
                                // Where the whole window scrolls (a phone), the knobs get
                                // their full height.
                                let editor_height = if place.body.is_some() {
                                    tallest
                                } else {
                                    (ctx.content_rect().height() - EDITOR_ROOM)
                                        .clamp(EDITOR_MIN_HEIGHT, tallest)
                                };
                                egui::ScrollArea::vertical()
                                    .id_salt("sound chain editor")
                                    .auto_shrink([false, false])
                                    .max_height(editor_height)
                                    .show(ui, |ui| {
                                        ui.set_min_height(editor_height);
                                        if window.selected == SELECT_INPUT {
                                            ui.label(RichText::new("Input trim").strong());
                                            ui.horizontal(|ui| {
                                                let mut trim = trim_db;
                                                if round_knob(
                                                ui,
                                                &mut trim,
                                                TRIM_RANGE,
                                                "Input trim",
                                                " dB",
                                                default_trim,
                                                false,
                                                "The track's input trim, before its chain's first \
                                             block: turn it up to drive an amp or a compressor \
                                             harder, or down to clean them up. It does not \
                                             change the track's level.",
                                            )
                                            .changed
                                            {
                                                actions.trim = Some(trim);
                                            }
                                                let (rect, response) = ui.allocate_exact_size(
                                                    vec2(10.0, 30.0),
                                                    Sense::hover(),
                                                );
                                                strips::paint_meter(
                                                    ui,
                                                    rect,
                                                    strips::Reading::measured(input),
                                                );
                                                response.on_hover_text(
                                            "The level before the chain (after the input trim), \
                                             left and right",
                                        );
                                            });
                                            return;
                                        }
                                        if window.selected == SELECT_OUTPUT {
                                            ui.label(RichText::new("Chain output").strong());
                                            ui.horizontal(|ui| {
                                                let mut output_db = chain.output_db;
                                                if round_knob(
                                                ui,
                                                &mut output_db,
                                                knob_ranges::OUTPUT,
                                                "Chain output",
                                                " dB",
                                                0.0,
                                                false,
                                                "The chain's output level, after every block and \
                                             before the mixer's fader. It is the tone's own \
                                             loudness: turning it does not change how hard an \
                                             amp or a drive is fed.",
                                            )
                                            .changed
                                            {
                                                let mut changed = chain.clone();
                                                changed.output_db = output_db;
                                                actions.chain = Some(changed);
                                            }
                                                let (rect, response) = ui.allocate_exact_size(
                                                    vec2(10.0, 30.0),
                                                    Sense::hover(),
                                                );
                                                strips::paint_meter(
                                                    ui,
                                                    rect,
                                                    strips::Reading::measured(output),
                                                );
                                                response.on_hover_text(
                                            "The level after the chain (with its output level), \
                                             left and right, before the fader",
                                        );
                                            });
                                            return;
                                        }
                                        match window.selected.checked_sub(1) {
                                            None => {
                                                let mut sound = sound;
                                                if strings_editor(
                                                    ui,
                                                    &player,
                                                    &mut sound,
                                                    &ports,
                                                    &mut window.strings_tab,
                                                    &mut window.programs,
                                                    &mut actions,
                                                ) {
                                                    actions.sound = Some(sound);
                                                }
                                            }
                                            Some(place)
                                                if plugin_panel
                                                    .as_ref()
                                                    .is_some_and(|panel| panel.place == place) =>
                                            {
                                                if let Some(panel) = &plugin_panel {
                                                    let edit = super::plugin_view::show(
                                                        ui,
                                                        &chain,
                                                        panel,
                                                        &mut window.plugin_filter,
                                                    );
                                                    if let Some(changed) = edit.chain {
                                                        actions.chain = Some(changed);
                                                    }
                                                    actions.plugin_rescan |= edit.rescan;
                                                    if let Some(choosing) = edit.choosing {
                                                        window.plugin_choosing = choosing;
                                                    }
                                                }
                                            }
                                            Some(place) => {
                                                let edited = editor(ui, &chain, place, reduction);
                                                if let Some(changed) = edited.chain {
                                                    actions.chain = Some(changed);
                                                }
                                                actions.rebuild |= edited.rebuild;
                                                if edited.load {
                                                    actions.load = Some(place);
                                                }
                                            }
                                        }
                                    });
                            });
                            ui.add_space(6.0);
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
                        };
                        // On a narrow screen the body scrolls under the title bar; at its
                        // usual width it is drawn as it is (a scroll area would clip it to
                        // the screen's height).
                        if let Some(height) = place.body {
                            egui::ScrollArea::vertical()
                                .id_salt("sound chain body")
                                .max_height(height)
                                // An auto-sized window offers its contents only last
                                // frame's height: ask for the room down to the screen's
                                // bottom, or it never grows.
                                .min_scrolled_height(height)
                                // A sheet fills what it covers, however little it shows.
                                .auto_shrink([false, !place.fill])
                                .show(ui, |ui| {
                                    body(ui);
                                    // What the floating drawer hides can be scrolled out
                                    // from under it.
                                    ui.add_space(place.hidden);
                                });
                        } else {
                            body(ui);
                        }
                    });
            });
        window.rect = shown.map(|shown| shown.response.rect);
        if sheet.is_some() {
            // A sheet is not moved; out of the practice view the window starts afresh.
            window.pos = None;
        }
        // Escape closes the window the user is looking at (a popup or a text field gets it
        // first).
        if style::escape_closes(ctx, egui::Id::unique(WINDOW_ID)) {
            actions.close = true;
        }
        // A tone belongs to the track's preset (its General MIDI program): every track with
        // that preset, in every song, plays it. Built-in forgets it.
        let track = window.track;
        if let Some(clip) = actions.copy.take() {
            self.sound_clipboard = Some(clip);
        }
        // A block dropped on a part's (or the kit's) button: into that board's first free
        // place, heard at once and undoable.
        if let Some((part, block)) = actions.pasted_part.take() {
            match part {
                Some(part) => self.paste_block_into_part(track, part, block),
                None => self.paste_block_into_track(track, block),
            }
        }
        // A drum part's chain and level belong to the song's copy of the track; Built-in
        // forgets the song's own and follows the kit again.
        if let Some(part) = window.part {
            let mut own = self.track_drum_part(track, part);
            let mut changed = false;
            if actions.reset_part {
                self.set_track_drum_part(track, part, None, false);
            } else {
                if let Some(edited) = actions.chain.take() {
                    own.chain = edited;
                    changed = true;
                }
                if let Some(edited) = actions.part_level.take() {
                    own.level = edited;
                    changed = true;
                }
                if let Some(edited) = actions.part_room.take() {
                    own.room = edited;
                    changed = true;
                }
                if changed {
                    self.set_track_drum_part(track, part, Some(own), true);
                }
            }
            // The strings' sound and the chosen tone belong to the track, not the part.
            actions.sound = None;
            actions.preset = None;
            actions.reset_tone = false;
        }
        let tone_chosen = actions.reset_tone || actions.preset.is_some();
        if actions.reset_tone {
            self.reset_track_tone(track);
        }
        // What plays the track first: the sound and chain edits below belong to the kind
        // of string chosen here (switching to steel while adding a block must store steel).
        if let Some(engine) = actions.engine {
            self.set_track_engine(track, engine);
        }
        // Another instrument: the track's program, an edit like the track panel's choice.
        if let Some(program) = actions.program {
            self.apply_command(
                gscore_edit::Command::SetTrackProgram {
                    track: gscore_model::TrackIndex::new(track),
                    program,
                },
                false,
            );
        }
        // A saved tone: it carries both a string sound and a chain (and what plays them).
        if let Some(preset) = actions.preset {
            self.remember_tone_choice(track, preset.choice);
            self.set_track_tone(
                track,
                Tone {
                    kind: preset.kind,
                    // A tone saved before pickups existed gets its kind's.
                    sound: preset.sound.for_kind(preset.kind),
                    chain: preset.chain,
                },
            );
            if let Some(engine) = preset.engine {
                self.set_track_engine(track, engine);
            }
        }
        if let Some(route) = actions.midi_out {
            self.set_midi_out(track, route);
        }
        // The kind that plays the track now, after any source or tone change above.
        let active_kind = self
            .track_player(track)
            .map_or(kind, |player| match player.engine {
                TrackEngine::Modelled(kind) => kind,
                TrackEngine::SoundFont | TrackEngine::MidiOut => player.preset_kind.unwrap_or(kind),
            });
        if actions.sound.is_some() || actions.chain.is_some() {
            let mut tone = self.track_tone(track, active_kind);
            if let Some(sound) = actions.sound {
                tone.sound = sound;
            }
            if let Some(chain) = actions.chain {
                tone.chain = chain;
            }
            self.set_track_tone(track, tone);
        }
        // The track's input trim, before whichever chain its sound goes through.
        if let Some(db) = actions.trim.take() {
            self.set_track_trim(track, db);
        }
        // The kit's room window (its own setting, unlike the parts' chains).
        if actions.open_room {
            self.open_kit_room(track);
        }
        // A room's length changes its prepared response, so the chain is built again now
        // that the knob has been let go (after the new value is stored).
        if actions.rebuild {
            self.playback.rebuild_chain(track);
        }
        if let Some(name) = actions.save_preset {
            let tone = self.track_tone(track, active_kind);
            let engine = self
                .track_player(track)
                .map_or(TrackEngine::Modelled(active_kind), |player| player.engine);
            let preset = ChainPreset {
                name,
                kind: kind_id(tone.kind).to_owned(),
                sound: tone.sound,
                chain: tone.chain,
                engine: engine_id(engine).to_owned(),
            };
            let presets = &mut self.settings.string_presets;
            presets.retain(|old| !(old.kind == preset.kind && old.name == preset.name));
            if presets.len() < MAX_STRING_PRESETS {
                presets.push(preset);
            }
            self.playback
                .store_string_presets(self.settings.string_presets.clone());
            self.save_settings();
        }
        if let Some(name) = actions.export_tone {
            self.export_track_tone(track, active_kind, name, ctx);
        }
        if actions.import_tone
            && let Err(error) = self.tasks.import_tone(ctx)
        {
            self.notice = Some(error);
        }
        if let Some((preset_kind, name)) = actions.delete_preset {
            self.settings
                .string_presets
                .retain(|preset| !(preset.kind == preset_kind && preset.name == name));
            self.playback
                .store_string_presets(self.settings.string_presets.clone());
            self.save_settings();
            // A tone dialed in from the deleted one no longer names it.
            for tab in &mut self.tabs {
                tab.tone_basis.retain(|_, basis| *basis != name);
            }
        }
        if let Some(place) = actions.load
            && let Err(error) = self.tasks.pick_impulse(
                track,
                active_kind,
                place,
                window.part.map(DrumPart::index),
                ctx,
            )
        {
            self.notice = Some(error);
        }
        if actions.listen && modelled {
            // The chain as it is now (not as it was when the window opened).
            self.sync_track_tones();
            match self.playback.play_string_demo(track, active_kind) {
                Ok(Heard::Skipped(why)) => self.notice = Some(format!("Not played: {why}")),
                Ok(_) => {}
                Err(error) => self.notice = Some(error),
            }
        }
        // A/B: while it is on the track plays the tone the window found; off, its own tone
        // again. Only what is heard changes, never the song.
        if actions.toggle_compare {
            window.comparing = !window.comparing && can_compare;
        }
        let comparing = window.comparing && !actions.close;
        if comparing && self.compare_tone.is_none() {
            if let Some(reference) = window.reference.clone() {
                self.compare_tone = Some((track, window.reference_own.then_some(reference)));
                self.sync_track_tones();
            }
        } else if !comparing {
            self.stop_comparing();
        }
        if tone_chosen {
            // What the window remembers of the choice is this machine's: keep it at once.
            self.save_settings();
        }
        if self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.editor().revision())
            != revision
        {
            ctx.request_repaint();
        }
        if actions.plugin_rescan {
            self.playback.plugins().rescan();
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

    /// A track's board: the source, the input trim, the five places, the output.
    fn track_slots() -> Vec<(Slot, f32)> {
        let mut slots = vec![(Slot::Box(0), BOX.x), (Slot::Input, END_WIDTH)];
        slots.extend((1..=BLOCKS).map(|index| (Slot::Box(index), BOX.x)));
        slots.push((Slot::Output, END_WIDTH));
        slots
    }

    #[test]
    fn the_window_keeps_its_width_on_a_desktop_and_fits_a_phone() {
        assert_eq!(content_width(1920.0), WIDTH);
        let phone = content_width(390.0);
        assert!(phone < WIDTH);
        assert!(phone + FRAME_WIDTH + 2.0 * SCREEN_GUTTER <= 390.0);
        assert_eq!(content_width(200.0), MIN_WIDTH);
    }

    #[test]
    fn the_board_is_one_row_at_its_width_and_folds_in_two_on_a_phone() {
        let slots = track_slots();
        let wide = board_layout(&slots, WIDTH);
        assert_eq!(wide.wrap, None);
        assert_eq!(wide.rects.len(), slots.len());
        let top = wide.rects[0].top();
        assert!(wide.rects.iter().all(|rect| rect.top() == top));
        for width in [MIN_WIDTH, content_width(360.0), content_width(390.0)] {
            let folded = board_layout(&slots, width);
            let wrap = folded.wrap.unwrap();
            assert_eq!(wrap, slots.len().div_ceil(2));
            let (first, second) = folded.rects.split_at(wrap);
            // Both rows start at the same edge, one under the other, inside the width, and
            // no two boxes overlap.
            assert_eq!(first[0].left(), second[0].left());
            assert!(second[0].top() >= first[0].bottom());
            for row in [first, second] {
                assert!(row.first().unwrap().left() >= 0.0);
                assert!(row.last().unwrap().right() <= width, "{width}: {row:?}");
                for pair in row.windows(2) {
                    assert!(pair[1].left() - pair[0].right() >= MIN_GAP - 1e-3);
                }
            }
        }
    }

    #[test]
    fn a_sheet_covers_its_rect_with_its_content_centred_and_no_wider_than_usual() {
        let phone = Rect::from_min_max(pos2(0.0, 0.0), pos2(390.0, 700.0));
        let sheet = Placement::sheet(phone, 0.0);
        assert_eq!(sheet.pos, phone.min);
        assert!(sheet.fill);
        assert!(sheet.width < WIDTH);
        assert_eq!(
            sheet.width + 2.0 * f32::from(sheet.side) + 2.0,
            phone.width()
        );
        assert!(sheet.body.unwrap() < phone.height());
        let side = Rect::from_min_max(pos2(0.0, 0.0), pos2(844.0, 390.0));
        let sheet = Placement::sheet(side, 120.0);
        assert_eq!(sheet.width, WIDTH);
        assert_eq!(
            sheet.width + 2.0 * f32::from(sheet.side) + 2.0,
            side.width()
        );
        assert_eq!(sheet.hidden, 120.0);
        assert!(sheet.body.unwrap() >= EDITOR_MIN_HEIGHT);
    }

    #[test]
    fn the_master_board_without_ends_folds_too() {
        let slots: Vec<(Slot, f32)> = (0..=BLOCKS)
            .map(|index| (Slot::Box(index), BOX.x))
            .collect();
        assert_eq!(board_layout(&slots, WIDTH).wrap, None);
        assert_eq!(board_layout(&slots, MIN_WIDTH).wrap, Some(3));
    }

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
