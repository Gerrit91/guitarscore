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
pub(super) mod strips;

pub(crate) use strips::MixerState;

/// How wide the handle on the column's right edge is.
const COLUMN_HANDLE: f32 = 6.0;
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
    /// A plain click on a bar: drop the selection (and the loop it turned on), put the
    /// cursor on the bar's first beat and, while playing, play on from there.
    Place { track: usize, measure: usize },
    /// Replace the selected measures.
    Select(MeasureSelection),
    /// Drop the selection (a section drag starts: only the dragged section is marked).
    ClearSelection,
    /// Swap the bars of the sections that start at `from` and `at` (dragging a section's
    /// name in the master row onto another section).
    SwapSections { from: usize, to: usize },
    /// Open the rename window for a track.
    Rename { track: usize },
    /// Show the track in the score's current view, or leave it out (the row's eye).
    ToggleShown { track: usize },
    /// Show a track in the track panel.
    ShowTrack { track: usize },
    /// Set the metronome's click volume (0 to 1).
    SetMetronomeVolume(f32),
    /// Set the measure columns' width factor (the zoom buttons).
    SetGridZoom(f32),
    /// Set the track column's width (dragged at its edge); `done` once the drag ends.
    SetTrackColumn { width: f32, done: bool },
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
    /// A drum part's level, pan, room send, mute or solo, kept with the song.
    DrumPart {
        track: usize,
        part: DrumPart,
        level: u8,
        pan: u8,
        room: u8,
        muted: bool,
        soloed: bool,
        /// A fader drag continues the previous undo step.
        merge: bool,
    },
    /// A track's send into the compressor bus (0-127), kept with the song.
    CompSend { track: usize, value: u8 },
    /// A drum kit's own send into its room (0-127), kept with the song.
    DrumRoomSend { track: usize, value: u8 },
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
    /// The short name typed so far (written before the track's staves).
    pub short_name: String,
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
        let shown = tab.shown_score(false).tracks;
        let document = &mut tab.document;
        let grid = document.grid();
        let cursor = document.editor().cursor();
        let rows = rows::track_rows(document, &shown);
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
        let room_sends: Vec<u8> = (0..rows.len())
            .map(|index| {
                sound
                    .tracks
                    .get(index)
                    .map_or(0, |track| track.drum_room_send)
            })
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
                            room: entry.map_or(0, |entry| entry.room.min(127)),
                            muted: entry.is_some_and(|entry| entry.muted),
                            soloed: entry.is_some_and(|entry| entry.soloed),
                            any_soloed,
                        }
                    })
                    .collect()
            })
            .collect();
        let meters = self.playback.meters();
        // A bus's meter without measured levels: the loudest send into it, through its own
        // (square-law) level.
        let estimate_bus = |sends: &dyn Fn(usize) -> u8, level: u8, muted: bool| {
            if muted {
                return 0.0;
            }
            let loudest = (0..rows.len())
                .filter(|index| !midi_out.get(*index).copied().unwrap_or(false))
                .map(|index| {
                    f32::from(sends(index).min(127)) / 127.0
                        * estimates.get(index).copied().unwrap_or(0.0)
                })
                .fold(0.0_f32, f32::max);
            let level = f32::from(level.min(127)) / 127.0;
            loudest * level * level
        };
        let bus_level = estimate_bus(
            &|index| rows.get(index).map_or(0, |row| row.reverb),
            reverb_bus.level,
            reverb_bus.muted,
        );
        let compressor_level = estimate_bus(
            &|index| comp_sends.get(index).copied().unwrap_or(0),
            compressor_bus.level,
            compressor_bus.muted,
        );
        let chorus_level = estimate_bus(
            &|index| rows.get(index).map_or(0, |row| row.chorus),
            chorus_bus.level,
            chorus_bus.muted,
        );
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
                compressor: strips::Reading::estimated(compressor_level, 64),
                chorus: strips::Reading::estimated(chorus_level, 64),
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
                zoom: self.view.grid_zoom,
                metronome: (
                    self.playback.metronome_volume(),
                    self.playback.metronome_level(),
                ),
                track_column: self.view.grid_track_column(),
                stripes: self.settings.grid_stripes,
            },
            &levels,
            &comp_sends,
            &room_sends,
            &parts,
            &mut tab.mixer,
            &mut actions,
        );
        for action in actions {
            self.apply_grid_action(action);
        }
    }

    /// Shows the mixer in the bottom panel instead of the measure grid, and back (the
    /// View menu, F3 and the grid's own button). The choice is remembered.
    pub(super) fn toggle_mixer(&mut self) {
        self.view.grid_mixer = !self.view.grid_mixer;
        self.save_settings();
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
            GridAction::ToggleMixer => self.toggle_mixer(),
            GridAction::BreakMerge => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.document.editor_mut().break_merge();
                }
            }
            GridAction::MoveCursor { track, measure } => {
                self.move_cursor_to(track, measure);
                self.grid_takes_keys();
            }
            GridAction::Place { track, measure } => {
                self.place_cursor_at(track, measure);
                self.grid_takes_keys();
            }
            GridAction::Select(selection) => {
                self.grid_takes_keys();
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
            GridAction::SwapSections { from, to } => self.swap_sections(from, to),
            GridAction::ClearSelection => {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                if tab.has_loop_selection() {
                    self.playback.set_loop_selection(false);
                }
                tab.selection = None;
                tab.had_loop_selection = false;
                tab.document.editor_mut().set_selection(None);
            }
            GridAction::ToggleShown { track } => self.toggle_track_shown(track),
            GridAction::Rename { track } => {
                let (name, short_name) = self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| tab.document.song().tracks().nth(track))
                    .map_or_else(Default::default, |row| {
                        (row.name().to_owned(), row.short_name().to_owned())
                    });
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.renaming = Some(RenameTrack {
                        track,
                        name,
                        short_name,
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
            GridAction::SetTrackColumn { width, done } => {
                self.view.set_grid_track_column(width);
                if done {
                    self.save_settings();
                }
            }
            GridAction::SetGridZoom(zoom) => {
                self.view.set_grid_zoom(zoom);
                self.save_settings();
            }
            GridAction::OpenReverbBus => self.open_reverb_bus_window(),
            GridAction::ReverbBus { bus } => self.set_song_reverb_bus(bus),
            GridAction::OpenCompressorBus => self.open_compressor_bus_window(),
            GridAction::CompSend { track, value } => self.set_comp_send(track, value),
            GridAction::DrumRoomSend { track, value } => {
                self.set_track_drum_room_send(track, value)
            }
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
                room,
                muted,
                soloed,
                merge,
            } => {
                let mut own = self.track_drum_part(track, part);
                own.level = level.min(127);
                own.pan = pan.min(127);
                own.room = room.min(127);
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

    /// The grid was clicked: Shift with the arrows selects bars there from now on.
    fn grid_takes_keys(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.grid_keys = true;
            // The pointer chose the rows: the keys start from the selection as it is.
            tab.grid_rows = None;
        }
    }

    /// After a click in the grid, Shift+Left/Right extends the grid's bar selection by a
    /// bar and Shift+Home/End to the song's first or last bar (from the cursor's bar, in
    /// its track, when nothing is selected), as a drag in the grid would. Shift+Up or
    /// Shift+Down takes in every track, as a drag leaving the row does; the other one then
    /// goes back to the track the selection started in. Pressing the same one again does
    /// nothing more. Returns whether the grid took `action`.
    pub(super) fn grid_select_key(&mut self, action: gscore_edit::ActionId) -> bool {
        use gscore_edit::ActionId;
        let Some(tab) = self.tabs.get(self.active) else {
            return false;
        };
        if !tab.grid_keys
            || !matches!(
                action,
                ActionId::SelectLeft
                    | ActionId::SelectRight
                    | ActionId::SelectHome
                    | ActionId::SelectEnd
                    | ActionId::SelectLineUp
                    | ActionId::SelectLineDown
            )
        {
            return false;
        }
        let Some(last) = tab.document.song().measure_count().checked_sub(1) else {
            return true;
        };
        let cursor = tab.document.editor().cursor();
        let current = tab
            .selection
            .unwrap_or_else(|| MeasureSelection::on(cursor.track.get(), cursor.measure.get()));
        // A selection of every track has nothing to take in: the key narrows it to the
        // cursor's track (the other direction then takes in every track again). Without
        // this, a section's selection (every track) would not answer the keys at all.
        if matches!(action, ActionId::SelectLineUp | ActionId::SelectLineDown)
            && current.track.is_none()
        {
            let narrowed = MeasureSelection {
                track: Some(cursor.track.get()),
                ..current
            };
            self.apply_grid_action(GridAction::Select(narrowed));
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.grid_rows = Some((cursor.track.get(), 0));
            }
            return true;
        }
        // The track the selection started in and which way the keys left it: -1 up, 1
        // down, 0 still on it (from a pointer's selection of every track: its cursor's
        // track, as if on it).
        let (track, rows) = tab
            .grid_rows
            .unwrap_or((current.track.unwrap_or(cursor.track.get()), 0));
        let rows = match action {
            ActionId::SelectLineUp => (rows - 1).max(-1),
            ActionId::SelectLineDown => (rows + 1).min(1),
            _ => rows,
        };
        let selection = match action {
            ActionId::SelectLeft => current.to(current.focus.saturating_sub(1)),
            ActionId::SelectRight => current.to(current.focus.saturating_add(1).min(last)),
            ActionId::SelectHome => current.to(0),
            ActionId::SelectEnd => current.to(last),
            _ => MeasureSelection {
                track: (rows == 0).then_some(track),
                ..current
            },
        };
        self.apply_grid_action(GridAction::Select(selection));
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.grid_rows = Some((track, rows));
        }
        true
    }

    /// A plain click on a bar: no selection, no loop from it, the cursor on the bar's first
    /// beat, and playback (while playing) going on from there.
    fn place_cursor_at(&mut self, track: usize, measure: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.has_loop_selection() {
            // Loop came on with that selection; without it, playback goes on normally.
            self.playback.set_loop_selection(false);
        }
        tab.selection = None;
        tab.had_loop_selection = false;
        let document = &mut tab.document;
        document.editor_mut().set_selection(None);
        let cursor = gscore_edit::Cursor {
            track: TrackIndex::new(track),
            measure: gscore_model::MeasureIndex::new(measure),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(0),
            ..document.editor().cursor()
        };
        document.editor_mut().set_cursor(cursor);
        tab.track = track;
        tab.turn_to_cursor = true;
        self.jump_playback_to(track, measure);
    }

    /// While playing, continues playback at the first beat of `track`, `measure` (after a
    /// new count-in when it is on).
    fn jump_playback_to(&mut self, track: usize, measure: usize) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        let at = gscore_model::BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(track),
            measure: gscore_model::MeasureIndex::new(measure),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(0),
        };
        if let Err(error) = self.playback.jump_to(document.song(), at) {
            self.notice = Some(error);
        }
        self.move_play_origin_to_cursor();
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
        let size = vec2(width + 32.0, 300.0);
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
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(
                                "The short name, written before the track's staves when \
                                 several tracks are shown (empty: the name).",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        let short = ui.add(
                            egui::TextEdit::singleline(&mut rename.short_name)
                                .hint_text("Short name, such as el.guit.")
                                .char_limit(gscore_model::MAX_SHORT_NAME_CHARS)
                                .desired_width(width),
                        );
                        ui.ctx().accesskit_node_builder(short.id, |node| {
                            node.set_label("Short name");
                        });
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
                let track = TrackIndex::new(rename.track);
                let (name, short_name) = (rename.name.trim(), rename.short_name.trim());
                let (old_name, old_short) = self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| tab.document.song().track(track))
                    .map_or_else(Default::default, |view| {
                        (view.name().to_owned(), view.short_name().to_owned())
                    });
                // What changed, as one undo step.
                let mut commands = Vec::new();
                if name != old_name {
                    commands.push(Command::RenameTrack {
                        track,
                        name: name.to_owned(),
                    });
                }
                if short_name != old_short {
                    commands.push(Command::SetTrackShortName {
                        track,
                        short_name: short_name.to_owned(),
                    });
                }
                self.apply_commands(commands);
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
    /// The width of one measure column, relative to a square cell (0.2 to 3).
    zoom: f32,
    /// The metronome's volume and how loud it sounds right now (both 0 to 1).
    metronome: (f32, f32),
    /// The track column's width.
    track_column: f32,
    /// Whether every other row has a lighter background (a setting).
    stripes: bool,
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
    room_sends: &[u8],
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
            room_sends,
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
    let scroll_id = ui.make_persistent_id("song_grid_scroll");
    // The matrix's scroll offset and the width it shows (without its vertical bar), as
    // last drawn: the ruler follows both.
    let last = ui
        .ctx()
        .data(|data| data.get_temp::<(egui::Vec2, f32)>(scroll_id));
    let offset = last.map(|(offset, _)| offset).unwrap_or_default();
    ui.horizontal_top(|ui| {
        rows::header(ui, rows, view, actions);
        let width = last.map(|(_, width)| width);
        matrix::ruler(ui, grid, view, offset.x, width, actions);
    });
    let scrolled =
        ui.horizontal_top(|ui| body(ui, rows, master, grid, view, levels, offset, actions));
    if Some(scrolled.inner) != last {
        // The ruler and the track column follow the matrix on the next frame.
        ui.ctx()
            .data_mut(|data| data.insert_temp(scroll_id, scrolled.inner));
        ui.ctx().request_repaint();
    }
    column_handle(ui, view, actions);
}

/// The track rows beside the measure matrix, scrolled together; returns the scroll offset
/// and the width of the measures shown.
///
/// The matrix scrolls both ways in one area, so both its scroll bars stay at the panel's
/// edges, in sight with any number of tracks, and they are always drawn (solid, not egui's
/// floating bars that appear only under the pointer). The track column has no bar of its
/// own: it follows the matrix up and down, and a wheel over it scrolls both.
#[allow(clippy::too_many_arguments)]
fn body(
    ui: &mut egui::Ui,
    rows: &[TrackRow],
    master: gscore_model::MasterMix,
    grid: &GridData,
    view: GridView,
    levels: &[f32],
    offset: egui::Vec2,
    actions: &mut Vec<GridAction>,
) -> (egui::Vec2, f32) {
    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
    let height = ui.available_height();
    let bar = ui.spacing().scroll.allocated_width();
    let matrix_width = ui.available_width() - view.track_column - ui.spacing().item_spacing.x;
    let wide = view.measure_count as f32 * MEASURE_WIDTH * view.zoom > matrix_width;
    let column = egui::ScrollArea::vertical()
        .id_salt("song_grid_rows")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .vertical_scroll_offset(offset.y)
        .max_height(height)
        .auto_shrink([true, false])
        .show(ui, |ui| {
            // The area takes the panel's sideways layout; the rows and the room below go down.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                rows::body(ui, rows, master, view, levels, actions);
                // Room for the matrix's horizontal bar, so both scroll down equally far.
                if wide {
                    ui.add_space(bar);
                }
            });
        });
    let scrolled = egui::ScrollArea::both()
        .id_salt("song_grid")
        .vertical_scroll_offset(column.state.clamped_offset().y)
        .max_width(ui.available_width())
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| matrix::matrix(ui, rows, grid, view, actions));
    (scrolled.state.clamped_offset(), scrolled.inner_rect.width())
}

/// The track column's right edge, dragged to make the column wider or narrower.
fn column_handle(ui: &mut egui::Ui, view: GridView, actions: &mut Vec<GridAction>) {
    let area = ui.min_rect();
    let x = area.left() + view.track_column;
    let rect = egui::Rect::from_min_max(
        egui::pos2(x - COLUMN_HANDLE / 2.0, area.top()),
        egui::pos2(x + COLUMN_HANDLE / 2.0, area.bottom()),
    );
    let response = ui.interact(
        rect,
        ui.scope_id().with("track_column_handle"),
        egui::Sense::drag(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, "Track column width")
    });
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        ui.painter().vline(
            x,
            area.y_range(),
            egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
        );
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        actions.push(GridAction::SetTrackColumn {
            width: pointer.x - area.left(),
            done: false,
        });
    }
    if response.drag_stopped() {
        actions.push(GridAction::SetTrackColumn {
            width: view.track_column,
            done: true,
        });
    }
}
