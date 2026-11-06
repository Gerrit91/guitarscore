//! The Sound chain window: the modelled strings and the effects after them, laid out like
//! a pedal board. Six boxes in a fixed row, joined by cables: the strings first (they
//! cannot be moved or removed; a click opens the String sound dialog), then five places,
//! each empty or holding a block (a cabinet or body impulse response, a room impulse
//! response, the computed reverb). The selected place's choices or knobs show below the
//! board. One chain per kind of string; every change is heard at once and kept in the
//! settings.

use std::path::PathBuf;

use egui::{Align2, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use gscore_guitar::{BLOCKS, knob_ranges};
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use crate::app::string_sound::{kind_name, knob, knob_response};
use crate::app::style;
use crate::chain_settings::{BlockChoice, BlockSetting, ChainSetting};
use crate::playback::Heard;

/// Width of the window's content.
const WIDTH: f32 = 700.0;
/// A box on the board, the gap between boxes (where the cable runs), the board's height.
const BOX: egui::Vec2 = vec2(86.0, 60.0);
const GAP: f32 = 36.0;
const BOARD_HEIGHT: f32 = 96.0;
/// How far a cable sags between two boxes.
const SAG: f32 = 7.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct SoundChainWindow {
    /// The track it is for (its kind of string decides which chain shows).
    track: usize,
    /// The selected box: 0 the strings, 1 to 5 the places after them.
    selected: usize,
    pos: Option<Pos2>,
}

/// What the user did this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    listen: bool,
    open_strings: bool,
    load: Option<usize>,
    chain: Option<ChainSetting>,
    /// A room's length moved and its knob was let go: build the room again.
    rebuild: bool,
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
struct KnobChanges {
    /// A knob moved: the new values are sent.
    changed: bool,
    /// The room's length knob was let go: the room is built again.
    rebuild: bool,
}

/// The block a box holds, as its two lines of text.
fn box_text(block: &BlockSetting) -> (String, String) {
    let detail = match block.choice {
        BlockChoice::Empty => String::new(),
        BlockChoice::Reverb => "computed".to_owned(),
        BlockChoice::Cabinet | BlockChoice::Room => block
            .file
            .as_ref()
            .and_then(|file| file.file_stem())
            .map_or_else(
                || "no file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            ),
    };
    let title = if block.choice == BlockChoice::Empty {
        "+".to_owned()
    } else {
        block.choice.label().to_owned()
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

/// The board: the boxes and the cables between them. Returns the box clicked, if any.
fn board(
    ui: &mut egui::Ui,
    kind: StringKind,
    chain: &ChainSetting,
    selected: usize,
) -> Option<usize> {
    let tokens = style::tokens(ui);
    let (area, _) = ui.allocate_exact_size(vec2(WIDTH, BOARD_HEIGHT), Sense::hover());
    let count = BLOCKS.saturating_add(1);
    let row = BOX.x * count as f32 + GAP * BLOCKS as f32;
    let left = area.left() + (WIDTH - row) / 2.0;
    let top = area.center().y - BOX.y / 2.0;
    let rect_of =
        |index: usize| Rect::from_min_size(pos2(left + index as f32 * (BOX.x + GAP), top), BOX);
    let painter = ui.painter_at(area);
    // Cables first, under the boxes: a sagging line from one box's output jack to the next
    // box's input jack.
    let cable = Stroke::new(2.5, tokens.text_weak.gamma_multiply(0.8));
    for index in 0..BLOCKS {
        let from = rect_of(index).right_center();
        let to = rect_of(index.saturating_add(1)).left_center();
        let points: Vec<Pos2> = (0..=12)
            .map(|step| {
                let t = step as f32 / 12.0;
                let x = from.x + (to.x - from.x) * t;
                pos2(x, from.y + SAG * 4.0 * t * (1.0 - t))
            })
            .collect();
        painter.add(egui::Shape::line(points, cable));
        for end in [from, to] {
            painter.circle_filled(end, 3.5, tokens.text_weak);
        }
    }
    let mut clicked = None;
    for index in 0..count {
        let rect = rect_of(index);
        let response = ui
            .interact(
                rect,
                ui.scope_id().with(("chain_box", index)),
                Sense::click(),
            )
            .on_hover_text(if index == 0 {
                "The strings: click to dial in pick, sustain and tone"
            } else {
                "Click to choose or adjust this block"
            });
        let block = index
            .checked_sub(1)
            .and_then(|place| chain.blocks.get(place));
        let (title, detail) = match block {
            None => ("Strings".to_owned(), kind_name(kind).to_owned()),
            Some(block) => box_text(block),
        };
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label(if index == 0 {
                "Strings".to_owned()
            } else {
                format!("Block {index}: {title}")
            });
            node.set_role(egui::Role::Button);
        });
        let empty = block.is_some_and(|block| block.choice == BlockChoice::Empty);
        let bypassed = block.is_some_and(|block| block.bypass);
        let fill = if index == 0 {
            tokens.control_active
        } else if empty {
            tokens.panel
        } else if response.hovered() {
            tokens.control_hover
        } else {
            tokens.control
        };
        painter.rect_filled(rect, 6.0, fill);
        let outline = if index == selected {
            Stroke::new(2.0, tokens.accent)
        } else {
            Stroke::new(1.0, tokens.line)
        };
        painter.rect_stroke(rect, 6.0, outline, egui::StrokeKind::Inside);
        let text = if bypassed || empty {
            tokens.text_weak
        } else {
            tokens.text
        };
        let title_size = if empty { 22.0 } else { 13.0 };
        painter.text(
            rect.center() - vec2(0.0, if empty { 0.0 } else { 8.0 }),
            Align2::CENTER_CENTER,
            short(&title, 14),
            FontId::proportional(title_size),
            text,
        );
        if !detail.is_empty() {
            painter.text(
                rect.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                short(&detail, 15),
                FontId::proportional(11.0),
                tokens.text_weak,
            );
        }
        if bypassed {
            painter.text(
                rect.left_top() + vec2(6.0, 4.0),
                Align2::LEFT_TOP,
                "off",
                FontId::proportional(10.0),
                tokens.text_weak,
            );
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
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
        ui.label(RichText::new("Choose a block for this place").strong());
        ui.add_space(4.0);
        let choices = [
            (
                BlockChoice::Cabinet,
                "An impulse response from a WAV file: an acoustic guitar's body, or a \
                 cabinet after an amp",
            ),
            (
                BlockChoice::Room,
                "A room's impulse response from a WAV file (stereo if recorded so): a \
                 convolution reverb",
            ),
            (
                BlockChoice::Reverb,
                "A computed room reverb, no file needed",
            ),
        ];
        for (choice, explanation) in choices {
            ui.horizontal(|ui| {
                if ui.button(choice.label()).clicked() {
                    edited = BlockSetting::of(choice);
                    changed = true;
                    load = choice.uses_file();
                }
                ui.label(RichText::new(explanation).weak());
            });
        }
    } else {
        ui.horizontal(|ui| {
            ui.label(RichText::new(block.choice.label()).strong());
            ui.checkbox(&mut edited.bypass, "Bypass")
                .on_hover_text("Let the sound pass this block unchanged");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Remove").clicked() {
                    edited = BlockSetting::default();
                    changed = true;
                }
            });
        });
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
        let moved = block_knobs(ui, &mut edited);
        changed |= moved.changed;
        changed |= edited.bypass != block.bypass;
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

/// The knobs of the block in `block`.
fn block_knobs(ui: &mut egui::Ui, block: &mut BlockSetting) -> KnobChanges {
    let mut moved = KnobChanges::default();
    match block.choice {
        BlockChoice::Empty => {}
        BlockChoice::Cabinet | BlockChoice::Room => {
            // First the room itself: how long it lasts.
            if block.choice == BlockChoice::Room {
                // A hand-edited settings file may hold anything; show it in range.
                block.length = block.room_length();
                let (changed, released) = length_knob(ui, &mut block.length);
                moved.changed |= changed;
                moved.rebuild |= released;
            }
            moved.changed |= knob(
                ui,
                &mut block.mix,
                knob_ranges::MIX,
                "Mix",
                "",
                false,
                "How much of the convolved sound: 0 dry, 1 only the response.",
            );
            moved.changed |= knob(
                ui,
                &mut block.gain_db,
                knob_ranges::GAIN,
                "Gain",
                " dB",
                false,
                "The level of the convolved sound.",
            );
            moved.changed |= knob(
                ui,
                &mut block.low_cut,
                knob_ranges::LOW_CUT,
                "Low cut",
                " Hz",
                true,
                "Takes away the lows below this frequency (boom, rumble); on a room, only \
                 from the room's sound.",
            );
            moved.changed |= knob(
                ui,
                &mut block.high_cut,
                knob_ranges::HIGH_CUT,
                "High cut",
                " Hz",
                true,
                "Takes away the highs above this frequency (fizz, hiss); on a room, only \
                 from the room's sound.",
            );
            if block.choice == BlockChoice::Room {
                moved.changed |= knob(
                    ui,
                    &mut block.width,
                    knob_ranges::WIDTH,
                    "Width",
                    "",
                    false,
                    "Stereo width of the room: 0 mono, 1 as recorded.",
                );
            }
        }
        BlockChoice::Reverb => {
            moved.changed |= knob(
                ui,
                &mut block.size,
                knob_ranges::SIZE,
                "Size",
                "",
                false,
                "The room's size: small and short to large and long.",
            );
            moved.changed |= knob(
                ui,
                &mut block.damping,
                knob_ranges::DAMPING,
                "Damping",
                "",
                false,
                "How quickly the room's highs die: 0 bright, 1 dull.",
            );
            moved.changed |= knob(
                ui,
                &mut block.width,
                knob_ranges::WIDTH,
                "Width",
                "",
                false,
                "Stereo width: 0 mono, 1 wide.",
            );
            moved.changed |= knob(
                ui,
                &mut block.mix,
                knob_ranges::MIX,
                "Mix",
                "",
                false,
                "How much of the room: 0 dry, 1 only the room.",
            );
            moved.changed |= knob(
                ui,
                &mut block.low_cut,
                knob_ranges::LOW_CUT,
                "Low cut",
                " Hz",
                true,
                "Takes the lows out of the reverb (not the dry guitar): keeps it from \
                 booming and muddying the bass.",
            );
            moved.changed |= knob(
                ui,
                &mut block.high_cut,
                knob_ranges::HIGH_CUT,
                "High cut",
                " Hz",
                true,
                "Takes the highs out of the reverb (not the dry guitar): a darker, more \
                 distant room.",
            );
        }
    }
    moved
}

/// The room's length knob: the decay time against the recording's. Returns whether it
/// moved, and whether the room should be built again — at once for a click on the slider or
/// an arrow key, at the end of a drag (not on every step, which would rebuild the tail
/// dozens of times).
fn length_knob(ui: &mut egui::Ui, length: &mut f32) -> (bool, bool) {
    let response = knob_response(
        ui,
        length,
        gscore_guitar::LENGTH_RANGE,
        "Length",
        "×",
        true,
        "The room's decay time against the recording's: 0.5 half as long, 4 four times as \
         long (past the recording, a synthesized tail carries it on). Built again when you \
         let go of the knob.",
    );
    let changed = response.changed();
    let settled = changed && !response.dragged();
    (changed, settled || response.drag_stopped())
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
        let title = "Sound chain";
        let size = vec2(WIDTH + 32.0, 420.0);
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
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut window.pos) {
                    actions.close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.label(RichText::new(format!("{} strings", kind_name(kind))).strong());
                        if !modelled {
                            ui.label(
                                RichText::new(
                                    "The SoundFont plays this track: choose modelled strings \
                                     in the Strings box to hear this chain.",
                                )
                                .weak(),
                            );
                        }
                        let chain = self.playback.string_chains().get(kind).clone();
                        ui.add_space(4.0);
                        if let Some(index) = board(ui, kind, &chain, window.selected) {
                            window.selected = index;
                            if index == 0 {
                                actions.open_strings = true;
                            }
                        }
                        ui.separator();
                        match window.selected.checked_sub(1) {
                            None => {
                                ui.label(
                                    RichText::new(
                                        "The strings come first. Click them to dial in the \
                                         pick, sustain and tone; click a place after them to \
                                         add a block.",
                                    )
                                    .weak(),
                                );
                            }
                            Some(place) => {
                                let edited = editor(ui, &chain, place);
                                actions.chain = edited.chain;
                                actions.rebuild = edited.rebuild;
                                if edited.load {
                                    actions.load = Some(place);
                                }
                            }
                        }
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
                    });
            });
        if let Some(chain) = actions.chain {
            self.playback.set_string_chain(kind, chain);
        }
        // A room's length changes its prepared response, so the chain is built again now
        // that the knob has been let go (after the new value is stored).
        if actions.rebuild {
            self.playback.rebuild_chain(kind);
        }
        if actions.open_strings {
            self.open_string_sound(window.track);
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
