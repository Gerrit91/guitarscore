//! Keys and actions: key presses go through the keymap to the document's editor; what the
//! editor hands back (play, save) is done here.

use crate::SystemTime;
use std::panic::{AssertUnwindSafe, catch_unwind};

use gscore_edit::{Action, ActionId, Command, Cursor, Outcome, Request};
use gscore_model::{BeatIndex, BeatPosition, MeasureIndex, TrackIndex, VoiceIndex};
use gscore_play::PlayState;

use crate::playback::Heard;

use super::GuitarScoreApp;

/// What an edit changes that the app must react to.
#[derive(Debug, Clone, Copy)]
enum EditEffect {
    /// A track's volume, pan or program: send it to the player without rebuilding.
    Sound(TrackIndex),
    /// Mute, solo, the drum kit or the tuning: the sequence must be rebuilt, in place.
    Rebuild,
    /// The name or color changed: playback goes on untouched.
    Display,
    /// The master volume or pan changed: every track's sound goes to the player.
    Master,
    /// The song's own sound state changed (what plays a track, its route, tone or send):
    /// playback takes the document's state.
    SoundState,
    /// Tracks were added, removed, duplicated or moved: per-track layouts are stale.
    Tracks,
    /// Anything else: measure stamps invalidate the layouts; playback stops.
    Other,
}

impl EditEffect {
    /// The effect of `command`.
    fn of(command: &Command) -> Self {
        if matches!(
            command,
            Command::SetTrackSound { .. } | Command::SetSongSound { .. }
        ) {
            EditEffect::SoundState
        } else if command.changes_master() {
            EditEffect::Master
        } else if let Some(track) = command.mix_track() {
            EditEffect::Sound(track)
        } else if command.rebuilds_playback() {
            EditEffect::Rebuild
        } else if command.changes_display_only() {
            EditEffect::Display
        } else if command.changes_tracks() {
            EditEffect::Tracks
        } else {
            EditEffect::Other
        }
    }
}

impl GuitarScoreApp {
    /// Runs the actions of this frame's key presses. Keys go to the score only when no
    /// text field, menu or dialog has the keyboard; a key that runs an action is taken out
    /// of the input, so no widget handles it as well.
    pub(super) fn handle_keys(&mut self, ctx: &egui::Context) {
        let tab = self.tabs.get(self.active);
        // Without a song only the actions that need none (files, windows, the view) run.
        let without_song = tab.is_none();
        if self.dialog.is_some()
            || self.settings_window.is_some()
            || self.shortcuts.is_some()
            || self.tuner.is_some()
            || self.transpose.is_some()
            || self.about.is_some()
            || self.history.is_some()
            || self.score_views.is_some()
            || self.new_song_dialog.is_some()
            || tab.is_some_and(|tab| tab.new_track.is_some())
            || tab.is_some_and(|tab| tab.renaming.is_some())
            || tab.is_some_and(|tab| tab.song_text.is_some())
            || self.paste_around.is_some()
            || tab.is_some_and(|tab| tab.track_panel.tuning_open())
            || tab.is_some_and(|tab| tab.edit_palette.staff.is_open())
            || tab.is_some_and(|tab| tab.edit_palette.takes_keys())
            || ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        // Some platforms turn Ctrl+Shift+V into a paste event without its key; the held
        // Shift then says the bars dialog was meant, as on the desktop's key event.
        let shift_paste = ctx.input(|input| input.modifiers.shift);
        let events = ctx.input_mut(|input| std::mem::take(&mut input.events));
        let mut kept = Vec::with_capacity(events.len());
        let mut actions = Vec::new();
        // While a fingering window is open its keys choose the finger (they win over the
        // keymap: `1` is a finger then, not a fret); everything else, the arrows among
        // them, works as usual, so the cursor moves on to the next note.
        let finger_hand = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.edit_palette.finger_hand());
        // In step entry its own keys come first (letters are notes then).
        let step_entry = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.document.editor().step_entry());
        let mut finger = None;
        for event in events {
            if let Some(hand) = finger_hand {
                let chosen = match &event {
                    egui::Event::Text(text) => super::edit_effects::finger_key(hand, text),
                    egui::Event::Key {
                        key: egui::Key::Backspace | egui::Key::Delete,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none() => Some(None),
                    _ => None,
                };
                if let Some(chosen) = chosen {
                    finger = Some(chosen);
                    continue;
                }
                // The key press that typed a finger's character is not a command either.
                if let egui::Event::Key { key, modifiers, .. } = &event
                    && !modifiers.command
                    && !modifiers.alt
                    && key_types_finger(hand, *key)
                {
                    continue;
                }
            }
            let combo = match &event {
                egui::Event::Paste(_) if shift_paste => Some(gscore_edit::KeyCombo::new(
                    gscore_edit::Key::Char('v'),
                    gscore_edit::Modifiers {
                        ctrl: true,
                        shift: true,
                        ..gscore_edit::Modifiers::default()
                    },
                )),
                _ => self.keys.combo(&event),
            };
            match combo.and_then(|c| self.keymap.action_in(c, step_entry)) {
                Some(action) if !without_song || needs_no_song(action) => actions.push(action),
                _ => kept.push(event),
            }
        }
        ctx.input_mut(|input| {
            // Keep anything added meanwhile after what was there.
            kept.append(&mut input.events);
            input.events = kept;
        });
        if let Some(finger) = finger
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.edit_palette.choose_finger(finger);
            ctx.request_repaint();
        }
        for action in actions {
            self.perform_action(action, ctx);
        }
    }

    /// Ctrl+Shift with +, - or 0: the whole interface (menus, panels, icons) larger,
    /// smaller or back to its normal size. Ctrl alone zooms only the score (keymap). Read
    /// from the key events directly: the keymap binds typed characters, and Shift changes
    /// the character (`+` is Shift+= on QWERTY, Shift++ is `*` on QWERTZ), so the key
    /// position decides too. Works with or without an open song.
    pub(super) fn interface_zoom_keys(&mut self, ctx: &egui::Context) {
        let view = &mut self.view;
        let mut changed = false;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                if !(modifiers.command && modifiers.shift) {
                    return true;
                }
                match interface_zoom_step(*key, *physical_key) {
                    Some(InterfaceZoom::Larger) => view.interface_larger(),
                    Some(InterfaceZoom::Smaller) => view.interface_smaller(),
                    Some(InterfaceZoom::Reset) => view.set_interface_zoom(1.0),
                    None => return true,
                }
                changed = true;
                false
            });
        });
        if changed {
            ctx.request_repaint();
        }
    }

    /// Plays a held-back fret (or step-entry beat) once it is complete: its digit window
    /// has ended, or a held chord was let go.
    pub(super) fn deferred_audition_tick(&mut self, ctx: &egui::Context) {
        if self.deferred_audition.is_none() {
            return;
        }
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        let complete_at = self
            .document()
            .and_then(|document| document.editor().fret_complete_at(now_ms));
        match complete_at {
            None => self.flush_deferred_audition(ctx),
            Some(u64::MAX) => {}
            Some(deadline) => ctx.request_repaint_after(std::time::Duration::from_millis(
                deadline.saturating_sub(now_ms).max(1),
            )),
        }
    }

    /// Plays the held-back fret or beat now, if there is one.
    fn flush_deferred_audition(&mut self, ctx: &egui::Context) {
        if let Some(to_hear) = self.deferred_audition.take() {
            self.hear(to_hear, ctx);
        }
    }

    /// Plays an entered note (briefly) or a step-entry beat (with its effects).
    fn hear(&mut self, to_hear: DeferredAudition, ctx: &egui::Context) {
        match to_hear {
            DeferredAudition::Note(at, string) => self.audition(at, string, ctx),
            DeferredAudition::Beat(at) => self.preview_beat(at),
        }
    }

    /// Step entry: moves the cursor on once a typed fret's digit window has ended, and
    /// asks for a frame then.
    pub(super) fn step_entry_tick(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let editor = tab.document.editor_mut();
        let Some(deadline) = editor.step_deadline_ms() else {
            return;
        };
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        if editor.tick(now_ms) == Outcome::Nothing {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                deadline.saturating_sub(now_ms).max(1),
            ));
        } else {
            ctx.request_repaint();
        }
    }

    /// Runs `action` at the cursor (from a key or a menu).
    pub(crate) fn perform_action(&mut self, action: Action, ctx: &egui::Context) {
        // A fret held back until it was complete is complete now: the next key came. An
        // undo or redo takes it back, so it is not heard then.
        match action {
            Action::Digit(_) | Action::ChordDigit(_) => {}
            Action::Plain(ActionId::Undo | ActionId::Redo) => self.deferred_audition = None,
            _ => self.flush_deferred_audition(ctx),
        }
        // Palette actions run like a click on their tile.
        if let Action::Plain(id) = action
            && id.is_palette()
        {
            self.run_palette_action(id);
            ctx.request_repaint();
            return;
        }
        // View actions do not touch the song, so they work without an open document.
        if let Action::Plain(id) = action {
            match id {
                ActionId::ZoomIn => {
                    self.view.zoom_in();
                    ctx.request_repaint();
                    return;
                }
                ActionId::ZoomOut => {
                    self.view.zoom_out();
                    ctx.request_repaint();
                    return;
                }
                ActionId::ZoomReset => {
                    self.view.set_zoom(1.0);
                    ctx.request_repaint();
                    return;
                }
                ActionId::ScrollPageUp | ActionId::ScrollPageDown => {
                    self.page_scroll = Some(if id == ActionId::ScrollPageUp {
                        -1.0
                    } else {
                        1.0
                    });
                    ctx.request_repaint();
                    return;
                }
                ActionId::ToggleFullscreen => {
                    let fullscreen =
                        ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                    return;
                }
                ActionId::ToggleMixer => {
                    self.toggle_mixer();
                    ctx.request_repaint();
                    return;
                }
                // Windows and files: they work without an open document too.
                ActionId::ShowHistory => {
                    self.open_history();
                    ctx.request_repaint();
                    return;
                }
                ActionId::NewSong => {
                    self.new_song(ctx);
                    return;
                }
                ActionId::OpenFile => {
                    self.ask_open(ctx);
                    return;
                }
                ActionId::SaveAs => {
                    if self.document().is_some() {
                        self.save_as_active(ctx);
                    }
                    return;
                }
                ActionId::OpenSettings => {
                    self.open_settings_window();
                    ctx.request_repaint();
                    return;
                }
                // Like the tab's close icon: unsaved changes are asked about first, and
                // closing the last tab leaves an empty workspace.
                ActionId::CloseTab => {
                    self.close_tab(self.active);
                    ctx.request_repaint();
                    return;
                }
                // Like the window's close button: unsaved changes are asked about first.
                // A browser tab is closed by the browser.
                ActionId::Quit => {
                    if cfg!(not(target_arch = "wasm32")) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    return;
                }
                _ => {}
            }
        }
        if let Action::Plain(id) = action
            && self.grid_select_key(id)
        {
            ctx.request_repaint();
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if action == Action::Plain(ActionId::ToggleMultitrack) {
            tab.score.toggle_all();
            ctx.request_repaint();
            return;
        }
        let document = &mut tab.document;
        // egui's input time, in milliseconds since the app started: enough to tell
        // whether a second fret digit follows quickly.
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        // A bug in a command must not take the app down (AGENTS.md "Containment"): the
        // model edits a copy, so a panic leaves the song as it was. Keep a snapshot anyway.
        let editor = document.editor_mut();
        let performed = catch_unwind(AssertUnwindSafe(|| editor.perform(action, now_ms)));
        let Ok(performed) = performed else {
            document.snapshot();
            self.notice = Some(
                "The edit failed unexpectedly and was not applied. A crash report and a \
                 recovery snapshot were written."
                    .to_owned(),
            );
            return;
        };
        // What was copied goes on the system clipboard as a line of text too: a desktop
        // sends Ctrl+V on as a paste only when the system clipboard holds text, so without
        // it the paste would never reach us (and another program sees what was copied).
        if performed.is_ok()
            && matches!(
                action,
                Action::Plain(ActionId::Copy | ActionId::CopyAllTracks | ActionId::Cut)
            )
            && let Some(text) = document.editor().clipboard_text()
        {
            ctx.copy_text(text);
        }
        match performed {
            Ok(Outcome::Edited) => {
                self.status = None;
                let layout = document
                    .editor()
                    .last_command()
                    .is_some_and(Command::changes_layout);
                let effect = document
                    .editor()
                    .last_command()
                    .map_or(EditEffect::Other, EditEffect::of);
                // A fret typed in is heard (not an undo, redo or paste that sets one), and
                // so is a beat whose effect or dynamic a shortcut changed.
                let entered = match (action, document.editor().last_command()) {
                    (Action::Digit(_), Some(Command::SetFret { at, string, .. })) => {
                        Some((*at, *string))
                    }
                    _ => None,
                };
                // A note or chord entered in step entry is heard as its whole beat.
                let stepped = (document.editor().step_entry()
                    && !matches!(
                        action,
                        Action::Plain(ActionId::Undo | ActionId::Redo | ActionId::Paste)
                    ))
                .then(|| document.editor().stepped_beat())
                .flatten();
                // A typed fret a second digit may still change is heard once complete
                // (Settings › Editor), not digit by digit.
                let typed = matches!(action, Action::Digit(_) | Action::ChordDigit(_));
                let incomplete = typed
                    && self.settings.wait_for_whole_frets
                    && document.editor().fret_complete_at(now_ms).is_some();
                // Only a toggle sets an effect this way; undo, redo and paste replay
                // commands and stay quiet.
                let heard = match action {
                    Action::Plain(id)
                        if !matches!(id, ActionId::Undo | ActionId::Redo | ActionId::Paste) =>
                    {
                        document.editor().last_command().and_then(preview_target)
                    }
                    _ => None,
                };
                // A transposition is not visible in the score (the notes stay as written);
                // say what happened in the status bar. Undo and redo replay the command, so
                // its step would say the opposite of what an undo did: they stay quiet.
                let transposed = match action {
                    Action::Plain(id) if !matches!(id, ActionId::Undo | ActionId::Redo) => document
                        .editor()
                        .last_command()
                        .and_then(|command| match command {
                            Command::TransposeSong { semitones } => Some(*semitones),
                            _ => None,
                        }),
                    _ => None,
                };
                self.sync_grid_selection();
                self.after_edit(effect, layout);
                if let Some(semitones) = transposed {
                    self.status = Some(transpose_status(semitones));
                }
                let to_hear = stepped
                    .map(DeferredAudition::Beat)
                    .or_else(|| entered.map(|(at, string)| DeferredAudition::Note(at, string)));
                if typed {
                    self.deferred_audition = None;
                }
                if let Some(to_hear) = to_hear {
                    if incomplete {
                        self.deferred_audition = Some(to_hear);
                        ctx.request_repaint();
                    } else {
                        self.hear(to_hear, ctx);
                    }
                } else if let Some(at) = heard {
                    self.preview_beat(at);
                }
                ctx.request_repaint();
            }
            Ok(Outcome::Moved) => {
                // Ctrl+A selects every bar of the song in the grid too, which shows the
                // score's bar selections over its squares.
                if action == Action::Plain(ActionId::SelectAll) {
                    tab.selection = Some(crate::state::MeasureSelection {
                        anchor: 0,
                        focus: tab.document.song().measure_count().saturating_sub(1),
                        track: None,
                    });
                }
                ctx.request_repaint();
            }
            Ok(Outcome::Nothing) => {
                // Up past the top string or down past the bottom one: on to the track
                // shown above or below, when several are shown.
                let step = match action {
                    Action::Plain(ActionId::CursorUpString) => Some(-1),
                    Action::Plain(ActionId::CursorDownString) => Some(1),
                    _ => None,
                };
                if let Some(step) = step {
                    self.cursor_to_neighbour_track(step);
                    ctx.request_repaint();
                }
            }
            Ok(Outcome::Request(request)) => self.request(request, ctx),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Enters one step at the cursor (a note tool's click, a MIDI note or chord): the
    /// beat is written at the entry length and the cursor moves on; with `chord` the notes
    /// join the beat entered last. With `play`, the beat is heard as entered (a MIDI
    /// keyboard that is monitored is heard already).
    pub(crate) fn enter_step(
        &mut self,
        input: gscore_edit::StepInput,
        chord: bool,
        play: bool,
        ctx: &egui::Context,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        let document = &mut tab.document;
        let editor = document.editor_mut();
        let entered = catch_unwind(AssertUnwindSafe(|| editor.enter_step(input, chord, now_ms)));
        let Ok(entered) = entered else {
            document.snapshot();
            self.notice = Some(
                "The edit failed unexpectedly and was not applied. A crash report and a \
                 recovery snapshot were written."
                    .to_owned(),
            );
            return;
        };
        match entered {
            Ok(Outcome::Edited) => {
                self.status = None;
                let editor = document.editor();
                let layout = editor.last_command().is_some_and(Command::changes_layout);
                let effect = editor
                    .last_command()
                    .map_or(EditEffect::Other, EditEffect::of);
                let at = editor.stepped_beat();
                self.sync_grid_selection();
                self.after_edit(effect, layout);
                if play && let Some(at) = at {
                    self.preview_beat(at);
                }
            }
            Ok(_) => {}
            Err(error) => self.notice = Some(error.to_string()),
        }
        ctx.request_repaint();
    }

    /// Runs `command` as one undo step from a window rather than a key press (a dialog's
    /// Apply button), refreshing the layout and playback as any edit does. Returns the
    /// reason when it could not run; the song is then unchanged.
    pub(super) fn run_command_from_window(&mut self, command: Command) -> Result<(), String> {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return Err("no song is open".to_owned());
        };
        let document = &mut tab.document;
        // A bug in a command must not take the app down (AGENTS.md "Containment"): the
        // model edits a copy, so a panic leaves the song as it was.
        let performed = catch_unwind(AssertUnwindSafe(|| {
            document.editor_mut().execute_command(command.clone())
        }));
        let Ok(performed) = performed else {
            document.snapshot();
            self.notice = Some(
                "The edit failed unexpectedly and was not applied. A crash report and a \
                 recovery snapshot were written."
                    .to_owned(),
            );
            return Err("the edit failed unexpectedly".to_owned());
        };
        let performed = performed.map_err(|error| error.to_string())?;
        let _ = performed;
        let layout = command.changes_layout();
        let effect = EditEffect::of(&command);
        self.after_edit(effect, layout);
        Ok(())
    }

    /// Moves the cursor to the track shown above (`step` -1, onto its bottom string) or
    /// below (`step` 1, onto its top string), at the same time in the measure.
    fn cursor_to_neighbour_track(&mut self, step: isize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(next) = tab.score.neighbour(tab.document.song(), tab.track, step) else {
            return;
        };
        let from = tab.document.editor().cursor();
        let to = super::score_view::cursor_in_track(tab.document.song(), from, next, step > 0);
        tab.track = next;
        tab.document.editor_mut().set_cursor(to);
    }

    /// Plays a draft effect without applying it: `commands` run on a copy of the song (no
    /// undo step, nothing journaled) and their beat is played from the copy. Asked for with
    /// a Listen button, so it plays even with "Play notes as they are entered" off.
    pub(super) fn listen_to(&mut self, commands: &[Command]) {
        let Some(at) = commands.iter().find_map(preview_target) else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        let mut draft = document.song().clone();
        for command in commands {
            if let Err(error) = command.apply(&mut draft) {
                self.notice = Some(error.to_string());
                return;
            }
        }
        // Asked for: say so when it cannot be heard.
        match self.playback.preview_beat(&draft, at) {
            Ok(Heard::Playing) => self.status = None,
            Ok(Heard::Waiting) => self.status = Some("Loading the SoundFont…".to_owned()),
            Ok(Heard::Skipped(why)) => self.status = Some(format!("Cannot listen: {why}.")),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Plays the beat at `at` with its effects, if hearing notes is on.
    /// The player runs on its own thread, so nothing waits for the sound.
    pub(super) fn preview_beat(&mut self, at: BeatPosition) {
        if !self.settings.audition_notes {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        if let Err(error) = self.playback.preview_beat(document.song(), at) {
            self.notice = Some(error);
        }
        // A beat heard on a click is a convenience: skipping it needs no message.
    }

    /// Sounds a note that was just entered, if the setting is on.
    pub(super) fn audition(
        &mut self,
        at: BeatPosition,
        string: gscore_model::StringNumber,
        ctx: &egui::Context,
    ) {
        if !self.settings.audition_notes {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        match self.playback.audition(document.song(), at, string) {
            // The note-off is sent from the frame loop.
            Ok(()) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Applies what an edit changed after it was executed: the cursor stays inside the
    /// song, stale per-track layouts go, and playback is updated for the kind of change.
    fn after_edit(&mut self, effect: EditEffect, layout_changed: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &mut tab.document;
        if layout_changed || matches!(effect, EditEffect::Tracks) {
            document.forget_layouts();
        }
        let tracks = document.song().track_count();
        if tab.track >= tracks {
            tab.track = tracks.saturating_sub(1);
        }
        if let Some(selection) = tab.selection
            && (selection.start() >= document.song().measure_count()
                || selection.track.is_some_and(|track| track >= tracks))
        {
            tab.selection = None;
        }
        let updated = match effect {
            // Volume, pan and program go straight to the player, so sounding notes are not
            // cut off.
            EditEffect::Sound(track) => self.playback.send_track_sound(document.song(), track),
            EditEffect::Master => self.playback.send_all_sounds(document.song()),
            // Mute, solo, the drum kit and the tuning change what plays: rebuild the
            // sequence and play on at the same position.
            EditEffect::Rebuild => self.playback.mix_changed(document.song()),
            // The name or color is not heard.
            EditEffect::Display => Ok(()),
            // The song's sound settings already live in the document; hand them to the
            // player below, after the document borrow ends.
            EditEffect::SoundState => Ok(()),
            // Anything else changes the score: stop and rebuild on the next play.
            EditEffect::Tracks | EditEffect::Other => {
                self.playback.song_changed();
                Ok(())
            }
        };
        if let Err(error) = updated {
            self.notice = Some(error);
        }
        // Undoing an edit can change the song's own sound too, so take it from the
        // document after every edit; the player ignores what did not change.
        self.sync_song_sound();
        self.sync_history_stamps();
    }

    /// Gives every history entry the time it was first seen (when its command was applied)
    /// and forgets entries that are no longer in the undo or redo list.
    pub(super) fn sync_history_stamps(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let now = SystemTime::now();
        let mut live = std::collections::HashSet::new();
        for id in tab
            .document
            .editor()
            .undo_ids()
            .chain(tab.document.editor().redo_ids())
        {
            live.insert(id);
            tab.history_stamps.entry(id).or_insert(now);
        }
        tab.history_stamps.retain(|id, _| live.contains(id));
    }

    /// Applies `command` (from a panel or menu). `merge` continues the previous undo entry
    /// (one slider drag). Mixer changes take effect in playback at once; other edits stop
    /// playback and invalidate the layouts.
    pub(crate) fn apply_command(&mut self, command: Command, merge: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        // With several tracks shown, line breaks are the multitrack layout's.
        let several = tab.shown_tracks().len() > 1;
        let document = &mut tab.document;
        let command = super::score_view::for_score(command, document.song(), several);
        let effect = EditEffect::of(&command);
        let layout_changed = command.changes_layout();
        let result = document.editor_mut().execute_command_merged(command, merge);
        match result {
            Ok(_) => self.after_edit(effect, layout_changed),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Moves the cursor to the beat being played, so the page shows it and scrolls along.
    /// The shown track is followed; repeats and track switches come from the sequence.
    pub(super) fn follow_playback(&mut self) {
        if self.playback.state() != PlayState::Playing {
            return;
        }
        // The sequence is an `Arc`: clone it so the document can be borrowed mutably.
        let Some(sequence) = self.playback.sequence().cloned() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &mut tab.document;
        let cursor = document.editor().cursor();
        let tick = self.playback.display_position();
        let Some(at) = sequence.beat_at(document.song(), cursor.position(), tick) else {
            return;
        };
        if at != cursor.position() {
            document.editor_mut().set_cursor(Cursor {
                measure: at.measure,
                voice: at.voice,
                beat: at.beat,
                ..cursor
            });
        }
    }

    /// Stops playback, leaving the cursor on the last played beat, or putting it back
    /// where playback started when the setting asks for it.
    pub fn stop_playback(&mut self) {
        self.follow_playback();
        if let Err(error) = self.playback.stop() {
            self.notice = Some(error);
        }
        self.return_to_play_origin(self.settings.return_on_stop);
    }

    /// Playing reached the song's end (without Loop, which plays it again): the cursor
    /// goes back where playback started, always (as in Guitar Pro; the setting is Stop's).
    pub(super) fn poll_song_end(&mut self) {
        let playing = self.playback.state() == PlayState::Playing;
        let ended = !self.playback.loop_selection() && self.playback.at_song_end();
        let Some(origin) = self
            .tabs
            .get_mut(self.active)
            .and_then(|tab| tab.play_origin.as_mut())
        else {
            return;
        };
        if playing {
            origin.played = true;
        } else if ended && origin.played {
            self.return_to_play_origin(true);
        }
    }

    /// Forgets where playback started, putting the cursor and selection back there when
    /// `back`.
    fn return_to_play_origin(&mut self, back: bool) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.palette_anchor = None;
            if let Some(origin) = tab.play_origin.take()
                && back
            {
                tab.document.editor_mut().set_cursor(origin.cursor);
                tab.document.editor_mut().set_selection(origin.selection);
                tab.selection = origin.measures;
                tab.had_loop_selection = tab.has_loop_selection();
            }
        }
    }

    /// After a jump while playing (a click in the score or the grid, a bar back or on):
    /// the place jumped to is where Stop goes back to, not where playback first started.
    pub(super) fn move_play_origin_to_cursor(&mut self) {
        let playing =
            self.playback.state() == PlayState::Playing || self.playback.count_in_pending();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if !playing || tab.play_origin.is_none() {
            return;
        }
        let played = tab.play_origin.is_some_and(|origin| origin.played);
        tab.play_origin = Some(super::tab::PlayOrigin {
            cursor: tab.document.editor().cursor(),
            selection: tab.document.editor().selection(),
            measures: tab.selection,
            played,
        });
    }

    /// Starts the song when its count-in bar has finished.
    pub(super) fn poll_count_in(&mut self) {
        if !self.playback.count_in_pending() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let document = &tab.document;
        if let Err(error) = self.playback.poll_count_in(document.song()) {
            self.notice = Some(error);
        }
    }

    /// A new selection of more than one beat turns Loop on (as Guitar Pro does); turned
    /// off, it stays off for that selection.
    pub(super) fn auto_loop(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let selected = tab.has_loop_selection();
        if selected && !tab.had_loop_selection {
            self.playback.set_loop_selection(true);
        }
        tab.had_loop_selection = selected;
    }

    /// Keeps the playing position inside the selected measures when looping is on.
    pub(super) fn poll_loop(&mut self) {
        if !self.playback.loop_selection() {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        // Nothing selected: looping plays the whole song again.
        let selection = tab.loop_measures().or_else(|| {
            let last = tab.document.song().measure_count().checked_sub(1)?;
            Some(crate::state::MeasureSelection::new(0).to(last))
        });
        let Some(selection) = selection else {
            // A song without measures has nothing to loop.
            return;
        };
        let timeline = tab.document.timeline();
        let range = timeline
            .as_deref()
            .and_then(|timeline| timeline.measure_range(selection.start(), selection.end()));
        // The loop's first beat, where a count-in leads each pass.
        let first = BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: tab.document.editor().cursor().track,
            measure: MeasureIndex::new(selection.start()),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        };
        if let Err(error) = self.playback.poll_loop(tab.document.song(), range, first) {
            self.notice = Some(error);
        }
    }

    /// Moves the cursor to the first beat of `measure` and, while playing or paused, moves
    /// the player there too. `at_end` seeks to the very end of the score instead of the
    /// measure's start.
    pub(super) fn goto_measure(&mut self, measure: usize, at_end: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let looped = tab.loop_measures();
        let document = &mut tab.document;
        let measure_count = document.song().measure_count();
        if measure_count == 0 {
            return;
        }
        let timeline = document.timeline();
        let loop_range = if self.playback.loop_selection() {
            looped.and_then(|selection| {
                timeline
                    .as_deref()
                    .and_then(|timeline| timeline.measure_range(selection.start(), selection.end()))
            })
        } else {
            None
        };
        let measure = measure.min(measure_count.saturating_sub(1));
        let cursor = document.editor().cursor();
        let at = Cursor {
            measure: MeasureIndex::new(measure),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
            ..cursor
        };
        document.editor_mut().set_cursor(at);
        let target = BeatPosition {
            staff: at.staff,
            track: at.track,
            measure: at.measure,
            voice: at.voice,
            beat: at.beat,
        };
        // A count-in is playing for the old place: count in again where the cursor went,
        // instead of stopping playback.
        if self.playback.count_in_pending() {
            if let Err(error) = self.playback.jump_to(document.song(), target) {
                self.notice = Some(error);
            }
            self.move_play_origin_to_cursor();
            return;
        }
        let Some(sequence) = self.playback.sequence().cloned() else {
            return;
        };
        let tick = if at_end {
            Some(sequence.end)
        } else {
            sequence.tick_of(document.song(), target)
        };
        if let (Some((start, end)), Some(tick)) = (loop_range, tick)
            && (tick < start || tick > end)
        {
            // An explicit jump outside the loop is a way to leave it.
            self.playback.set_loop_selection(false);
        }
        if let Some(tick) = tick
            && let Err(error) = self.playback.seek_tick(tick)
        {
            self.notice = Some(error);
        }
        self.move_play_origin_to_cursor();
    }

    /// Moves to the previous bar (and seeks there while playing or paused).
    pub fn previous_bar(&mut self) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let measure = document.editor().cursor().measure.get().saturating_sub(1);
        self.goto_measure(measure, false);
    }

    /// Moves to the next bar (and seeks there while playing or paused).
    pub fn next_bar(&mut self) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let measure = document.editor().cursor().measure.get().saturating_add(1);
        self.goto_measure(measure, false);
    }

    /// Moves to the top of the score (and seeks there while playing or paused).
    pub fn go_to_start(&mut self) {
        self.goto_measure(0, false);
    }

    /// Moves to the end of the score (and seeks there while playing or paused).
    pub fn go_to_end(&mut self) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let last = document.song().measure_count().saturating_sub(1);
        self.goto_measure(last, true);
    }

    /// Whether Stop puts the cursor and selection back where playback started.
    pub fn set_return_on_stop(&mut self, back: bool) {
        self.settings.return_on_stop = back;
    }

    /// Turns looping of the selected measures on or off.
    pub fn set_loop_selection(&mut self, enabled: bool) {
        self.playback.set_loop_selection(enabled);
    }

    /// Turns the metronome on or off (heard at once while playing).
    pub fn set_metronome(&mut self, enabled: bool) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        if let Err(error) = self.playback.set_metronome(document.song(), enabled) {
            self.notice = Some(error);
        }
    }

    /// Sets the metronome's volume (0 to 1), heard at once while playing.
    pub fn set_metronome_volume(&mut self, volume: f32) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            self.playback.store_metronome_volume(volume);
            return;
        };
        if let Err(error) = self.playback.set_metronome_volume(document.song(), volume) {
            self.notice = Some(error);
        }
    }

    /// Chooses the metronome's click sound (heard at once while playing).
    pub fn set_click_sound(&mut self, sound: gscore_play::ClickSound) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            self.playback.store_click_sound(sound);
            return;
        };
        if let Err(error) = self.playback.set_click_sound(document.song(), sound) {
            self.notice = Some(error);
        }
    }

    /// Plays a bar of the metronome alone (the open song's tempo, else 120 BPM).
    pub fn preview_click(&mut self) {
        let fallback;
        let song = match self.tabs.get(self.active) {
            Some(tab) => tab.document.song(),
            None => {
                fallback = gscore_model::Song::default();
                &fallback
            }
        };
        if let Err(error) = self.playback.preview_click(song) {
            self.notice = Some(error);
        }
    }

    /// Turns the one-bar count-in on or off.
    pub fn set_count_in(&mut self, enabled: bool) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        if let Err(error) = self.playback.set_count_in(document.song(), enabled) {
            self.notice = Some(error);
        }
    }

    /// Plays guitar and bass tracks with the modelled strings, or with the SoundFont.
    pub fn set_modelled_guitars(&mut self, modelled: bool) {
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            self.playback.store_modelled_guitars(modelled);
            return;
        };
        if let Err(error) = self
            .playback
            .set_modelled_guitars(document.song(), modelled)
        {
            self.notice = Some(error);
        }
        // A track's starting trim follows what plays it.
        self.sync_trims();
    }

    /// Slows down or speeds up playback (0.5 to 1.2 of the written tempo).
    pub fn set_playback_speed(&mut self, speed: f32) {
        if let Err(error) = self.playback.set_speed(speed) {
            self.notice = Some(error);
            return;
        }
        self.settings.playback_speed = self.playback.speed();
    }

    /// Sets the speed trainer's settings (stored with the app settings).
    pub fn set_speed_trainer(&mut self, trainer: crate::playback::SpeedTrainer) {
        self.playback.set_trainer(trainer);
        let trainer = self.playback.trainer();
        self.settings.speed_trainer_enabled = trainer.enabled;
        self.settings.speed_trainer_every = trainer.every;
        self.settings.speed_trainer_step = trainer.step;
        self.settings.speed_trainer_target = trainer.target;
    }

    /// Does what the editor asked the app for.
    pub(super) fn request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::PlayPause => self.play_pause(),
            Request::Stop => self.stop_playback(),
            Request::Save => self.save_song(ctx),
            Request::PasteAround => self.open_paste_around(),
            Request::SelectLine { down } => self.select_line(down, ctx),
        }
    }

    /// Swaps the bars of the two sections that start at `from` and `to` (a drag in the
    /// grid's master row), with the effects a structural edit has.
    pub(super) fn swap_sections(&mut self, from: usize, to: usize) {
        let swapped = self.document_mut().map(|document| {
            document
                .editor_mut()
                .swap_sections(MeasureIndex::new(from), MeasureIndex::new(to))
        });
        match swapped {
            Some(Ok(_)) => self.after_bars_edit(),
            Some(Err(error)) => self.notice = Some(error.to_string()),
            None => {}
        }
    }

    /// Moves the `count` bars starting at `from` so they start at `to`, in every track
    /// (dragging the grid's bar selection), with the effects a structural edit has.
    pub(super) fn move_bars(&mut self, from: usize, count: usize, to: usize) {
        let moved = self.document_mut().map(|document| {
            document
                .editor_mut()
                .move_bars(MeasureIndex::new(from), count, MeasureIndex::new(to))
        });
        match moved {
            Some(Ok(_)) => self.after_bars_edit(),
            Some(Err(error)) => self.notice = Some(error.to_string()),
            None => {}
        }
    }

    /// What an edit of whole bars made outside [`Self::perform_action`] (a section swap, a
    /// paste from the paste dialog) needs afterwards, as any edit does: the grid shows the
    /// bars it selected, stale layouts go, and playback stops and is built again.
    pub(super) fn after_bars_edit(&mut self) {
        self.status = None;
        self.sync_grid_selection();
        self.after_edit(EditEffect::Other, true);
    }

    /// The grid shows the editor's measure selection: an edit can select what it wrote (a
    /// paste selects the bars it pasted).
    pub(super) fn sync_grid_selection(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let measures = match tab.document.editor().selection() {
            Some(gscore_edit::Selection::Measures {
                anchor,
                focus,
                track,
            }) => Some(crate::state::MeasureSelection {
                anchor: anchor.get(),
                focus: focus.get(),
                track: track.map(|track| track.get()),
            }),
            _ => None,
        };
        if measures.is_some() {
            tab.selection = measures;
        }
    }

    /// Extends the beat selection to the line above or below, as a text editor does: to
    /// the beat at the same place on the neighbouring system of the page. Past the first
    /// or last line it goes to the track's start or end.
    fn select_line(&mut self, down: bool, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let cursor = tab.document.editor().cursor();
        let target = tab.document.shown_layout().and_then(|layout| {
            let places = layout.staff_places(cursor.track, cursor.staff)?;
            Some(super::page::line_spot(
                places,
                gscore_edit::BeatSpot::of(cursor),
                cursor.voice,
                down,
            ))
        });
        match target {
            // No layout yet (nothing drawn since the last edit): nothing to go by.
            None => {}
            Some(Some(spot)) => {
                tab.document.editor_mut().extend_selection_to_spot(spot);
                // A grid selection gives way to the score's, as with Shift+Left/Right.
                tab.selection = None;
                ctx.request_repaint();
            }
            Some(None) => {
                let to = if down {
                    ActionId::SelectEnd
                } else {
                    ActionId::SelectHome
                };
                self.perform_action(Action::Plain(to), ctx);
            }
        }
    }

    /// Where a loop starts: the first beat of its first bar, while looping is on. A loop
    /// plays whole bars, so its first pass starts there too, not at the first selected note.
    fn loop_start(&self) -> Option<BeatPosition> {
        if !self.playback.loop_selection() {
            return None;
        }
        let tab = self.tabs.get(self.active)?;
        let measures = tab.loop_measures()?;
        Some(BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: tab.document.editor().cursor().track,
            measure: MeasureIndex::new(measures.start()),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        })
    }

    /// Starts playback of the open song at the cursor, or pauses it. Pausing leaves the
    /// cursor on the last played beat.
    pub fn play_pause(&mut self) {
        let already_playing = self.playback.state() == PlayState::Playing;
        if already_playing && self.settings.return_on_stop {
            // Going back to the start is asked for: Space stops as well (it is the key
            // users stop with), instead of pausing where it is.
            self.stop_playback();
            return;
        }
        if already_playing {
            self.follow_playback();
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if already_playing {
                // Paused: the palette follows the cursor again.
                tab.palette_anchor = None;
            } else {
                // Starting (or continuing after a count-in): remember what the palette
                // showed so its tiles stay on the note chosen before playback.
                tab.palette_anchor = Some((
                    tab.document.editor().cursor(),
                    tab.document.editor().selection(),
                ));
            }
        }
        // Pausing then playing resumes at the cursor; a fresh start plays from the
        // selection's first note when there is a selection.
        let started = if self.playback.state() == PlayState::Paused {
            None
        } else {
            self.loop_start().or_else(|| self.selection_start())
        };
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let from = started.unwrap_or_else(|| document.editor().cursor().position());
        let fresh = self.playback.state() == PlayState::Stopped;
        if let Err(error) = self.playback.play_pause(document.song(), Some(from)) {
            self.notice = Some(error);
            return;
        }
        if fresh && let Some(tab) = self.tabs.get_mut(self.active) {
            let cursor = tab.document.editor().cursor();
            tab.play_origin = Some(super::tab::PlayOrigin {
                cursor: Cursor {
                    track: from.track,
                    staff: from.staff,
                    measure: from.measure,
                    voice: from.voice,
                    beat: from.beat,
                    ..cursor
                },
                selection: tab.document.editor().selection(),
                measures: tab.selection,
                played: false,
            });
        }
    }
}

/// The beat to play after `command`: one whose sound it changed (an effect, a dynamic, a
/// dead note), or the first such beat of a batch.
pub(super) fn preview_target(command: &Command) -> Option<BeatPosition> {
    match command {
        Command::SetNoteEffect { at, .. }
        | Command::SetBeatEffect { at, .. }
        | Command::SetDynamic { at, .. }
        | Command::SetDeadNote { at, .. } => Some(*at),
        Command::Batch { commands } => commands.iter().find_map(preview_target),
        _ => None,
    }
}

/// Whether `key` types one of `hand`'s finger characters (its key event is then no command).
fn key_types_finger(hand: gscore_model::Hand, key: egui::Key) -> bool {
    let name = key.name().to_lowercase();
    super::edit_effects::finger_key(hand, &name).is_some()
        || matches!(key, egui::Key::Backspace | egui::Key::Delete)
}

/// What the status bar says after transposing by `semitones`.
fn transpose_status(semitones: i32) -> String {
    match semitones {
        12 => "Transposed up an octave".to_owned(),
        -12 => "Transposed down an octave".to_owned(),
        1 => "Transposed up a semitone".to_owned(),
        -1 => "Transposed down a semitone".to_owned(),
        step if step > 0 => format!("Transposed up {step} semitones"),
        step => format!("Transposed down {} semitones", step.saturating_neg()),
    }
}

/// Whether `action` works with no song open: files, windows and the view.
fn needs_no_song(action: Action) -> bool {
    matches!(
        action,
        Action::Plain(
            ActionId::NewSong
                | ActionId::OpenFile
                | ActionId::OpenSettings
                | ActionId::CloseTab
                | ActionId::ShowHistory
                | ActionId::Quit
                | ActionId::ZoomIn
                | ActionId::ZoomOut
                | ActionId::ZoomReset
                | ActionId::ToggleFullscreen
                | ActionId::ToggleMixer
        )
    )
}

/// A change of the interface size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterfaceZoom {
    Larger,
    Smaller,
    Reset,
}

/// What Ctrl+Shift with `key` (the character, or the key position when the character has
/// no name) at `physical` (the key position, named as on a US keyboard) does to the
/// interface size. The 0 key resets (Shift+0 is `=` on QWERTZ, so the position counts
/// first); the minus key, or QWERTZ's minus at the US `/` position, shrinks; plus or
/// equals, or QWERTZ's plus at the US `]` position, enlarges.
fn interface_zoom_step(key: egui::Key, physical: Option<egui::Key>) -> Option<InterfaceZoom> {
    use egui::Key as K;
    let at = |position: K| physical == Some(position);
    if at(K::Num0) || key == K::Num0 {
        Some(InterfaceZoom::Reset)
    } else if at(K::Minus) || at(K::Slash) || key == K::Minus {
        Some(InterfaceZoom::Smaller)
    } else if matches!(key, K::Plus | K::Equals) || at(K::Equals) || at(K::CloseBracket) {
        Some(InterfaceZoom::Larger)
    } else {
        None
    }
}

#[cfg(test)]
mod interface_zoom_tests {
    use super::{InterfaceZoom, interface_zoom_step};
    use egui::Key as K;

    #[test]
    fn plus_minus_and_zero_size_the_interface_on_qwerty_and_qwertz() {
        // QWERTY: Shift+= types `+`, Shift+- types `_` (no key name: the position).
        assert_eq!(
            interface_zoom_step(K::Plus, Some(K::Equals)),
            Some(InterfaceZoom::Larger)
        );
        assert_eq!(
            interface_zoom_step(K::Minus, Some(K::Minus)),
            Some(InterfaceZoom::Smaller)
        );
        assert_eq!(
            interface_zoom_step(K::Num0, Some(K::Num0)),
            Some(InterfaceZoom::Reset)
        );
        // QWERTZ: + sits at the US `]`, - at the US `/`, and Shift+0 types `=`.
        assert_eq!(
            interface_zoom_step(K::CloseBracket, Some(K::CloseBracket)),
            Some(InterfaceZoom::Larger)
        );
        assert_eq!(
            interface_zoom_step(K::Slash, Some(K::Slash)),
            Some(InterfaceZoom::Smaller)
        );
        assert_eq!(
            interface_zoom_step(K::Equals, Some(K::Num0)),
            Some(InterfaceZoom::Reset)
        );
        assert_eq!(interface_zoom_step(K::A, Some(K::A)), None);
    }
}

/// An entered note waiting to be heard until no second digit can change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredAudition {
    /// One note, typed outside step entry.
    Note(BeatPosition, gscore_model::StringNumber),
    /// A beat entered in step entry (its chord).
    Beat(BeatPosition),
}
