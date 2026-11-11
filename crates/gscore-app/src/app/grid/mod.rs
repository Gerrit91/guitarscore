//! The track and song grid (the bottom panel): one row per track with its color, instrument
//! and mixer controls, and a measure matrix that fills a square where a track has notes.
//!
//! Clicking a square moves the edit cursor there; dragging selects a range of measures (for
//! copy/paste and looping later); the measure being played is highlighted. Track commands
//! go through `gscore-edit`, so every change can be undone.

use egui::{Align, Color32, Layout, Margin, RichText, vec2};
use gscore_edit::Command;
use gscore_model::TrackIndex;

use super::GuitarScoreApp;
use super::shape_editor::Outcome;
use super::style;
use crate::chain_settings::{BlockSetting, DrumPart};
use crate::document::GridData;
use crate::state::MeasureSelection;

use rows::TrackRow;

mod bar;
mod matrix;
pub(super) mod mixer;
mod rows;
mod strips;

pub(crate) use strips::MixerState;

/// Width of the fixed track column on the left.
pub(super) const TRACK_COLUMN: f32 = 384.0;
/// Height of one track row.
pub(super) const ROW_HEIGHT: f32 = 24.0;
/// Height of the ruler above the matrix (measure numbers, sections, repeats).
pub(super) const RULER_HEIGHT: f32 = 26.0;
/// Most track rows the panel shows when it opens; more tracks scroll.
pub(crate) const OPEN_ROWS: usize = 8;
/// The panel's margins and the horizontal scroll bar, around the rows.
const PANEL_EXTRA: f32 = 22.0;

/// The panel's height for `tracks` rows plus the master row.
pub(crate) fn height_for(tracks: usize) -> f32 {
    RULER_HEIGHT + (tracks as f32 + 1.0) * ROW_HEIGHT + PANEL_EXTRA
}

/// Default height of the mixer view, and the range it may be dragged to.
pub(crate) const MIXER_HEIGHT: f32 = 390.0;
pub(crate) const MIXER_MIN_HEIGHT: f32 = 240.0;
pub(crate) const MIXER_MAX_HEIGHT: f32 = 620.0;

/// The height the panel opens at: all tracks (up to eight) and the master row.
pub(crate) fn opening_height(tracks: usize) -> f32 {
    height_for(tracks.min(OPEN_ROWS))
}

/// Width of one measure column: the same as a row, so every cell is a square.
pub(super) const MEASURE_WIDTH: f32 = ROW_HEIGHT;
/// Padding inside a matrix square: none, the squares touch.
pub(super) const CELL_PADDING: f32 = 0.0;

/// What the grid asks the app to do after drawing.
#[derive(Debug)]
pub(super) enum GridAction {
    /// Run a track command; `merge` continues the previous undo step (one slider drag).
    Command { command: Command, merge: bool },
    /// Open the new-track popup, inserting before this track.
    NewTrack { at: usize },
    /// Switch between the measure grid and the mixer.
    ToggleMixer,
    /// Run a command, then show `track` (a track that was added or moved).
    CommandShowing { command: Command, track: usize },
    /// A slider drag started or ended: the next command starts a new undo step.
    BreakMerge,
    /// Move the cursor to the first beat of a track and measure.
    MoveCursor { track: usize, measure: usize },
    /// Replace the selected measures.
    Select(MeasureSelection),
    /// Open the rename window for a track.
    Rename { track: usize },
    /// Show a track in the track panel.
    ShowTrack { track: usize },
    /// Set the metronome's click volume (0 to 1).
    SetMetronomeVolume(f32),
    /// Open the window of the mixer's reverb bus.
    OpenReverbBus,
    /// Set the mixer's reverb bus (an edit of the song, heard at once).
    ReverbBus { bus: crate::settings::ReverbBus },
    /// Open the window of the mixer's compressor bus.
    OpenCompressorBus,
    /// Set the mixer's compressor bus (an edit of the song, heard at once).
    CompressorBus { bus: crate::settings::CompressorBus },
    /// Open the window of the mixer's chorus bus.
    OpenChorusBus,
    /// Set the mixer's chorus bus (an edit of the song, heard at once).
    ChorusBus { bus: crate::settings::ChorusBus },
    /// Open the master's sound chain window.
    OpenMasterChain,
    /// Open `track`'s sound chain window.
    OpenSoundChain { track: usize },
    /// Open the sound chain of `part` of a drum track.
    OpenSoundChainPart { track: usize, part: DrumPart },
    /// A drum part's level, pan, mute or solo, kept with the song.
    DrumPart {
        track: usize,
        part: DrumPart,
        level: u8,
        pan: u8,
        muted: bool,
        soloed: bool,
        /// A fader drag continues the previous undo step.
        merge: bool,
    },
    /// A track's send into the compressor bus (0-127), kept with the song.
    CompSend { track: usize, value: u8 },
    /// A block dropped on a track's (or part's) mixer strip: into its chain's first free
    /// place.
    PasteBlock {
        track: usize,
        part: Option<DrumPart>,
        block: BlockSetting,
    },
}

/// The rename window's state.
#[derive(Debug)]
pub(super) struct RenameTrack {
    /// The track being renamed.
    pub track: usize,
    /// The text typed so far.
    pub name: String,
    /// Whether the text field has been focused yet.
    pub focused: bool,
    /// Where the window sits (remembered while it is open).
    pub pos: Option<egui::Pos2>,
}

impl GuitarScoreApp {
    /// The bottom panel: track headers and the measure matrix.
    pub(super) fn track_grid(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let sound = tab.sound();
        let document = &mut tab.document;
        let grid = document.grid();
        let cursor = document.editor().cursor();
        let rows = rows::track_rows(document);
        let master = document.song().master_mix();
        let measure_count = document.song().measure_count();
        let playing = self.playback.current_measure().map(|m| m.saturating_sub(1));
        let estimates: Vec<f32> = (0..rows.len())
            .map(|index| self.playback.track_level(TrackIndex::new(index)))
            .collect();
        let midi_out: Vec<bool> = (0..rows.len())
            .map(|index| self.playback.track_is_midi_out(index))
            .collect();
        let comp_sends: Vec<u8> = (0..rows.len())
            .map(|index| sound.tracks.get(index).map_or(0, |track| track.comp_send))
            .collect();
        let reverb_bus = sound
            .buses
            .reverb
            .clone()
            .unwrap_or_else(|| self.settings.reverb_bus.clone());
        let compressor_bus = sound
            .buses
            .compressor
            .clone()
            .unwrap_or_else(|| self.settings.compressor_bus.clone());
        let chorus_bus = sound
            .buses
            .chorus
            .clone()
            .unwrap_or_else(|| self.settings.chorus_bus.clone());
        // A drum track's parts, only for the tracks whose strips can open them.
        let parts: Vec<Vec<strips::PartRow>> = (0..rows.len())
            .map(|index| {
                if !rows.get(index).is_some_and(|row| row.drums) {
                    return Vec::new();
                }
                let own = sound.tracks.get(index);
                let any_soloed = own.is_some_and(|track| {
                    track
                        .drum_parts
                        .iter()
                        .any(|part| part.soloed && part.part().is_some())
                });
                DrumPart::ALL
                    .into_iter()
                    .map(|part| {
                        let entry = own.and_then(|track| {
                            track.drum_parts.iter().find(|own| own.part() == Some(part))
                        });
                        strips::PartRow {
                            part,
                            level: entry.map_or(127, |entry| entry.level.min(127)),
                            pan: entry.map_or(64, |entry| entry.pan.min(127)),
                            muted: entry.is_some_and(|entry| entry.muted),
                            soloed: entry.is_some_and(|entry| entry.soloed),
                            any_soloed,
                        }
                    })
                    .collect()
            })
            .collect();
        let meters = self.playback.meters();
        // The bus's meter: the loudest send into it, through its own (square-law) level.
        let bus_level = if reverb_bus.muted {
            0.0
        } else {
            let loudest = rows
                .iter()
                .enumerate()
                .filter(|(index, _)| !midi_out.get(*index).copied().unwrap_or(false))
                .map(|(index, row)| {
                    f32::from(row.reverb.min(127)) / 127.0
                        * estimates.get(index).copied().unwrap_or(0.0)
                })
                .fold(0.0_f32, f32::max);
            let level = f32::from(reverb_bus.level.min(127)) / 127.0;
            loudest * level * level
        };
        // What the meters show: measured by the built-in synthesizer, or estimated from the
        // notes (MIDI output, the browser).
        let readings = match &meters {
            Some(meters) => strips::Readings {
                tracks: rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| {
                        // A hardware synthesizer's audio never reaches the mixer: its strip
                        // shows the notes it plays instead of a measured level.
                        if midi_out.get(index).copied().unwrap_or(false) {
                            strips::Reading::estimated(
                                estimates.get(index).copied().unwrap_or(0.0),
                                row.pan,
                            )
                        } else {
                            strips::Reading::measured(meters.track(index))
                        }
                    })
                    .collect(),
                parts: rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| {
                        if !row.drums {
                            return Vec::new();
                        }
                        DrumPart::ALL
                            .into_iter()
                            .map(|part| strips::Reading::measured(meters.part(index, part.index())))
                            .collect()
                    })
                    .collect(),
                bus: strips::Reading::measured(meters.reverb()),
                compressor: strips::Reading::measured(meters.compressor()),
                chorus: strips::Reading::measured(meters.chorus()),
                master: strips::Reading::measured(meters.master()),
            },
            None => strips::Readings {
                tracks: rows
                    .iter()
                    .zip(&estimates)
                    .map(|(row, &level)| strips::Reading::estimated(level, row.pan))
                    .collect(),
                // Without meters the parts have nothing measured to show.
                parts: parts
                    .iter()
                    .map(|parts| vec![strips::Reading::estimated(0.0, 64); parts.len()])
                    .collect(),
                bus: strips::Reading::estimated(bus_level, 64),
                // No Comp knobs yet (step 5): nothing is sent into it, so it is silent.
                compressor: strips::Reading::estimated(0.0, 64),
                chorus: strips::Reading::estimated(0.0, 64),
                master: strips::Reading::estimated(
                    estimates
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| !midi_out.get(*index).copied().unwrap_or(false))
                        .map(|(_, &level)| level)
                        .fold(0.0_f32, f32::max),
                    master.pan,
                ),
            },
        };
        // The song grid's activity bars follow the meters.
        let levels: Vec<f32> = readings.tracks.iter().map(strips::Reading::bar).collect();
        if meters.as_ref().is_some_and(|meters| meters.moving()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(30));
        }
        let mut actions = Vec::new();
        draw(
            ui,
            &rows,
            master,
            &grid,
            strips::Buses {
                reverb: reverb_bus,
                compressor: compressor_bus,
                chorus: chorus_bus,
            },
            &readings,
            GridView {
                measure_count,
                cursor_measure: cursor.measure.get(),
                cursor_track: cursor.track.get(),
                playing,
                // The grid shows the score's selection too (the bars a beat selection
                // spans), in that track.
                selection: tab.loop_measures(),
                selection_color: crate::canvas::color(self.view.theme.palette().selection),
                shown_track: tab.track,
                mixer: self.view.grid_mixer,
                metronome: (
                    self.playback.metronome_volume(),
                    self.playback.metronome_level(),
                ),
            },
            &levels,
            &comp_sends,
            &parts,
            &mut tab.mixer,
            &mut actions,
        );
        for action in actions {
            self.apply_grid_action(action);
        }
    }

    /// Runs one action from the grid (or the score's right-click menu).
    pub(super) fn apply_grid_action(&mut self, action: GridAction) {
        match action {
            GridAction::Command { command, merge } => self.apply_command(command, merge),
            GridAction::CommandShowing { command, track } => {
                let revision = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.editor().revision());
                self.apply_command(command, false);
                if self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.editor().revision())
                    != revision
                    && let Some(tab) = self.tabs.get_mut(self.active)
                {
                    tab.track = track;
                }
            }
            GridAction::NewTrack { at } => self.begin_new_track(at),
            GridAction::ToggleMixer => {
                self.view.grid_mixer = !self.view.grid_mixer;
                self.save_settings();
            }
            GridAction::BreakMerge => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.document.editor_mut().break_merge();
                }
            }
            GridAction::MoveCursor { track, measure } => self.move_cursor_to(track, measure),
            GridAction::Select(selection) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.selection = Some(selection);
                    tab.document.editor_mut().set_selection(Some(
                        gscore_edit::Selection::Measures {
                            anchor: gscore_model::MeasureIndex::new(selection.start()),
                            focus: gscore_model::MeasureIndex::new(selection.end()),
                            track: selection.track.map(TrackIndex::new),
                        },
                    ));
                    if let Some(track) = selection.track {
                        tab.track = track;
                    }
                }
                // While playing, playback goes to the selection's first measure, unless
                // that measure is already playing (the selection just follows it).
                let playing_measure = if self.playback.state() == gscore_play::PlayState::Playing {
                    self.playback
                        .current_measure()
                        .map(|measure| measure.saturating_sub(1))
                } else {
                    None
                };
                if playing_measure != Some(selection.start()) {
                    let track = self.tabs.get(self.active).map_or(0, |tab| tab.track);
                    self.jump_playback_to(track, selection.start());
                }
            }
            GridAction::Rename { track } => {
                let name = self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| tab.document.song().tracks().nth(track))
                    .map_or_else(String::new, |row| row.name().to_owned());
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.renaming = Some(RenameTrack {
                        track,
                        name,
                        focused: false,
                        pos: None,
                    });
                }
            }
            GridAction::ShowTrack { track } => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.track = track;
                }
                self.view.show_track_panel = true;
            }
            GridAction::SetMetronomeVolume(volume) => self.set_metronome_volume(volume),
            GridAction::OpenReverbBus => self.open_reverb_bus_window(),
            GridAction::ReverbBus { bus } => self.set_song_reverb_bus(bus),
            GridAction::OpenCompressorBus => self.open_compressor_bus_window(),
            GridAction::CompSend { track, value } => self.set_comp_send(track, value),
            GridAction::CompressorBus { bus } => self.set_song_compressor_bus(bus),
            GridAction::OpenChorusBus => self.open_chorus_bus_window(),
            GridAction::ChorusBus { bus } => self.set_song_chorus_bus(bus),
            GridAction::OpenMasterChain => self.open_master_chain_window(),
            GridAction::OpenSoundChain { track } => self.open_sound_chain(track),
            GridAction::OpenSoundChainPart { track, part } => {
                self.open_sound_chain_part(track, part);
            }
            GridAction::PasteBlock { track, part, block } => match part {
                Some(part) => self.paste_block_into_part(track, part, block),
                None => self.paste_block_into_track(track, block),
            },
            GridAction::DrumPart {
                track,
                part,
                level,
                pan,
                muted,
                soloed,
                merge,
            } => {
                let mut own = self.track_drum_part(track, part);
                own.level = level.min(127);
                own.pan = pan.min(127);
                own.muted = muted;
                own.soloed = soloed;
                self.set_track_drum_part(track, part, Some(own), merge);
            }
        }
    }

    /// Moves the cursor to the first beat of `track`, `measure`. While playing, choosing a
    /// track (or the measure that is already playing) keeps playback going where it is; only
    /// a different measure seeks.
    fn move_cursor_to(&mut self, track: usize, measure: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &mut tab.document;
        let playing_measure = if self.playback.state() == gscore_play::PlayState::Playing {
            self.playback
                .current_measure()
                .map(|measure| measure.saturating_sub(1))
        } else {
            None
        };
        let same_measure = playing_measure == Some(measure);
        let mut cursor = document.editor().cursor();
        cursor.track = TrackIndex::new(track);
        cursor.measure = gscore_model::MeasureIndex::new(measure);
        if !same_measure {
            cursor.voice = gscore_model::VoiceIndex::new(0);
            cursor.beat = gscore_model::BeatIndex::new(0);
        }
        document.editor_mut().set_cursor(cursor);
        tab.track = track;
        if !same_measure {
            self.jump_playback_to(track, measure);
        }
    }

    /// While playing, continues playback at the first beat of `track`, `measure` (after a
    /// new count-in when it is on).
    fn jump_playback_to(&mut self, track: usize, measure: usize) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        let at = gscore_model::BeatPosition {
            track: TrackIndex::new(track),
            measure: gscore_model::MeasureIndex::new(measure),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(0),
        };
        if let Err(error) = self.playback.jump_to(document.song(), at) {
            self.notice = Some(error);
        }
    }

    /// The rename window, in the same layout as the other dialogs: a title bar, a hint, the
    /// name field (focused at once) and Cancel/Rename.
    pub(super) fn show_rename_window(&mut self, ctx: &egui::Context) {
        let Some(mut rename) = self
            .tabs
            .get_mut(self.active)
            .and_then(|tab| tab.renaming.take())
        else {
            return;
        };
        let title = "Rename track";
        let width = 420.0;
        let size = vec2(width + 32.0, 230.0);
        let pos = *rename
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut rename.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(
                            RichText::new(
                                "The name shown in the track panel, the song grid and the score.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut rename.name)
                                .hint_text("Track name")
                                .desired_width(width),
                        );
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Track name");
                        });
                        if !rename.focused {
                            response.request_focus();
                            rename.focused = true;
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                let button =
                                    |text: &str| egui::Button::new(text).min_size(vec2(84.0, 28.0));
                                if ui.add(button("Cancel")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                let tokens = style::tokens(ui);
                                let ok = egui::Button::new(
                                    RichText::new("Rename").color(tokens.on_accent),
                                )
                                .fill(tokens.accent)
                                .min_size(vec2(84.0, 28.0));
                                if ui.add_enabled(!rename.name.trim().is_empty(), ok).clicked() {
                                    outcome = Outcome::Apply;
                                }
                            },
                        );
                        if outcome == Outcome::Open {
                            ui.input(|input| {
                                if input.key_pressed(egui::Key::Escape) {
                                    outcome = Outcome::Cancel;
                                } else if input.key_pressed(egui::Key::Enter)
                                    && !rename.name.trim().is_empty()
                                {
                                    outcome = Outcome::Apply;
                                }
                            });
                        }
                    });
            });
        match outcome {
            Outcome::Apply => {
                let name = rename.name.trim().to_owned();
                self.apply_command(
                    Command::RenameTrack {
                        track: TrackIndex::new(rename.track),
                        name,
                    },
                    false,
                );
            }
            Outcome::Cancel => {}
            Outcome::Open | Outcome::Remove | Outcome::Listen => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.renaming = Some(rename);
                }
            }
        }
    }
}

/// What the grid needs to know about the app while drawing.
#[derive(Clone, Copy)]
pub(super) struct GridView {
    measure_count: usize,
    cursor_measure: usize,
    cursor_track: usize,
    playing: Option<usize>,
    selection: Option<MeasureSelection>,
    /// The score's selection color, so a selection looks the same in both views.
    selection_color: Color32,
    shown_track: usize,
    mixer: bool,
    /// The metronome's volume and how loud it sounds right now (both 0 to 1).
    metronome: (f32, f32),
}

#[allow(clippy::too_many_arguments)]
fn draw(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: gscore_model::MasterMix,
    grid: &GridData,
    buses: strips::Buses,
    readings: &strips::Readings,
    view: GridView,
    levels: &[f32],
    comp_sends: &[u8],
    parts: &[Vec<strips::PartRow>],
    mixer_state: &mut MixerState,
    actions: &mut Vec<GridAction>,
) {
    if view.mixer {
        bar::header(ui, rows, view, actions);
        strips::strips(
            ui,
            rows,
            master,
            buses,
            readings,
            comp_sends,
            parts,
            view,
            mixer_state,
            actions,
        );
        return;
    }
    // The top line - the track actions and the mixer switch, beside the measure ruler -
    // stays put; only the tracks below scroll, however many there are.
    ui.spacing_mut().item_spacing.y = 0.0;
    let scroll_id = ui.make_persistent_id("song_grid_x");
    let scroll_x = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(scroll_id))
        .unwrap_or(0.0);
    ui.horizontal_top(|ui| {
        rows::header(ui, rows, view, actions);
        matrix::ruler(ui, grid, view, scroll_x, actions);
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal_top(|ui| {
                rows::body(ui, rows, master, view, levels, actions);
                let scrolled = egui::ScrollArea::horizontal()
                    .id_salt("song_grid")
                    .show(ui, |ui| matrix::matrix(ui, rows, grid, view, actions));
                ui.ctx().data_mut(|data| {
                    data.insert_temp(scroll_id, scrolled.state.clamped_offset().x);
                });
            });
        });
}
