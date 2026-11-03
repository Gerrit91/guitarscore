//! Keys and actions: key presses go through the keymap to the document's editor; what the
//! editor hands back (play, save) is done here.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::SystemTime;

use gscore_edit::{Action, ActionId, Command, Cursor, Outcome, Request};
use gscore_model::{BeatIndex, BeatPosition, MeasureIndex, TrackIndex, VoiceIndex};
use gscore_play::PlayState;

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
    /// Tracks were added, removed, duplicated or moved: per-track layouts are stale.
    Tracks,
    /// Anything else: measure stamps invalidate the layouts; playback stops.
    Other,
}

impl EditEffect {
    /// The effect of `command`.
    fn of(command: &Command) -> Self {
        if command.changes_master() {
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
        if self.document.is_none()
            || self.dialog.is_some()
            || self.settings_window.is_some()
            || self.shortcuts.is_some()
            || self.new_track.is_some()
            || self.edit_palette.staff.is_open()
            || ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        let events = ctx.input_mut(|input| std::mem::take(&mut input.events));
        let mut kept = Vec::with_capacity(events.len());
        let mut actions = Vec::new();
        for event in events {
            match self.keys.combo(&event).and_then(|c| self.keymap.action(c)) {
                Some(action) => actions.push(action),
                None => kept.push(event),
            }
        }
        ctx.input_mut(|input| {
            // Keep anything added meanwhile after what was there.
            kept.append(&mut input.events);
            input.events = kept;
        });
        for action in actions {
            self.perform_action(action, ctx);
        }
    }

    /// Runs `action` at the cursor (from a key or a menu).
    pub(crate) fn perform_action(&mut self, action: Action, ctx: &egui::Context) {
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
                ActionId::ToggleFullscreen => {
                    let fullscreen =
                        ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                    return;
                }
                _ => {}
            }
        }
        let Some(document) = &mut self.document else {
            return;
        };
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
                self.after_edit(effect, layout);
                if let Some((at, string)) = entered {
                    self.audition(at, string, ctx);
                } else if let Some(at) = heard {
                    self.preview_beat(at);
                }
                ctx.request_repaint();
            }
            Ok(Outcome::Moved) => ctx.request_repaint(),
            Ok(Outcome::Nothing) => {}
            Ok(Outcome::Request(request)) => self.request(request, ctx),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Plays the beat at `at` with its effects, if hearing notes is on.
    /// The player runs on its own thread, so nothing waits for the sound.
    pub(super) fn preview_beat(&mut self, at: BeatPosition) {
        if !self.settings.audition_notes {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        if let Err(error) = self.playback.preview_beat(document.song(), at) {
            self.notice = Some(error);
        }
    }

    /// Sounds a note that was just entered, if the setting is on.
    fn audition(
        &mut self,
        at: BeatPosition,
        string: gscore_model::StringNumber,
        ctx: &egui::Context,
    ) {
        if !self.settings.audition_notes {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        match self.playback.audition(document.song(), at, string) {
            // The note-off is sent from the frame loop.
            Ok(()) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Applies what an edit changed after it was executed: the cursor stays inside the
    /// song, stale per-track layouts go, and playback is updated for the kind of change.
    fn after_edit(&mut self, effect: EditEffect, layout_changed: bool) {
        let Some(document) = &mut self.document else {
            return;
        };
        if layout_changed || matches!(effect, EditEffect::Tracks) {
            document.forget_layouts();
        }
        let tracks = document.song().track_count();
        if self.view.track >= tracks {
            self.view.track = tracks.saturating_sub(1);
        }
        if let Some(selection) = self.view.selection
            && selection.start() >= document.song().measure_count()
        {
            self.view.selection = None;
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
            // Anything else changes the score: stop and rebuild on the next play.
            EditEffect::Tracks | EditEffect::Other => {
                self.playback.song_changed();
                Ok(())
            }
        };
        if let Err(error) = updated {
            self.notice = Some(error);
        }
        self.sync_history_stamps();
    }

    /// Gives every history entry the time it was first seen (when its command was applied)
    /// and forgets entries that are no longer in the undo or redo list.
    pub(super) fn sync_history_stamps(&mut self) {
        let Some(document) = &self.document else {
            self.history_stamps.clear();
            return;
        };
        let now = SystemTime::now();
        let mut live = std::collections::HashSet::new();
        for id in document
            .editor()
            .undo_ids()
            .chain(document.editor().redo_ids())
        {
            live.insert(id);
            self.history_stamps.entry(id).or_insert(now);
        }
        self.history_stamps.retain(|id, _| live.contains(id));
    }

    /// Applies `command` (from a panel or menu). `merge` continues the previous undo entry
    /// (one slider drag). Mixer changes take effect in playback at once; other edits stop
    /// playback and invalidate the layouts.
    pub(crate) fn apply_command(&mut self, command: Command, merge: bool) {
        let Some(document) = &mut self.document else {
            return;
        };
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
        let Some(document) = &mut self.document else {
            return;
        };
        let cursor = document.editor().cursor();
        let tick = self.playback.position();
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

    /// Stops playback, leaving the cursor on the last played beat.
    pub(super) fn stop_playback(&mut self) {
        self.follow_playback();
        if let Err(error) = self.playback.stop() {
            self.notice = Some(error);
        }
    }

    /// Starts the song when its count-in bar has finished.
    pub(super) fn poll_count_in(&mut self) {
        if !self.playback.count_in_pending() {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        if let Err(error) = self.playback.poll_count_in(document.song()) {
            self.notice = Some(error);
        }
    }

    /// Keeps the playing position inside the selected measures when looping is on.
    pub(super) fn poll_loop(&mut self) {
        if !self.playback.loop_selection() {
            return;
        }
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(selection) = self.view.selection else {
            // Nothing is selected any more: looping has no range.
            self.playback.set_loop_selection(false);
            return;
        };
        let timeline = document.timeline();
        let range = timeline
            .as_deref()
            .and_then(|timeline| timeline.measure_range(selection.start(), selection.end()));
        if let Err(error) = self.playback.poll_loop(range) {
            self.notice = Some(error);
        }
    }

    /// Moves the cursor to the first beat of `measure` and, while playing or paused, moves
    /// the player there too. `at_end` seeks to the very end of the score instead of the
    /// measure's start.
    fn goto_measure(&mut self, measure: usize, at_end: bool) {
        if self.playback.count_in_pending() {
            // A count-in for the old place makes no sense now.
            if let Err(error) = self.playback.stop() {
                self.notice = Some(error);
            }
        }
        let Some(document) = &mut self.document else {
            return;
        };
        let measure_count = document.song().measure_count();
        if measure_count == 0 {
            return;
        }
        let timeline = document.timeline();
        let loop_range = if self.playback.loop_selection() {
            self.view.selection.and_then(|selection| {
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
            track: at.track,
            measure: at.measure,
            voice: at.voice,
            beat: at.beat,
        };
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
    }

    /// Moves to the previous bar (and seeks there while playing or paused).
    pub fn previous_bar(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let measure = document.editor().cursor().measure.get().saturating_sub(1);
        self.goto_measure(measure, false);
    }

    /// Moves to the next bar (and seeks there while playing or paused).
    pub fn next_bar(&mut self) {
        let Some(document) = &self.document else {
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
        let Some(document) = &self.document else {
            return;
        };
        let last = document.song().measure_count().saturating_sub(1);
        self.goto_measure(last, true);
    }

    /// Turns looping of the selected measures on or off.
    pub fn set_loop_selection(&mut self, enabled: bool) {
        self.playback.set_loop_selection(enabled);
    }

    /// Turns the metronome on or off (heard at once while playing).
    pub fn set_metronome(&mut self, enabled: bool) {
        let Some(document) = &self.document else {
            return;
        };
        if let Err(error) = self.playback.set_metronome(document.song(), enabled) {
            self.notice = Some(error);
        }
    }

    /// Turns the one-bar count-in on or off.
    pub fn set_count_in(&mut self, enabled: bool) {
        let Some(document) = &self.document else {
            return;
        };
        if let Err(error) = self.playback.set_count_in(document.song(), enabled) {
            self.notice = Some(error);
        }
    }

    /// Does what the editor asked the app for.
    pub(super) fn request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::PlayPause => self.play_pause(),
            Request::Stop => self.stop_playback(),
            Request::Save => self.save_song(ctx),
        }
    }

    /// Starts playback of the open song at the cursor, or pauses it. Pausing leaves the
    /// cursor on the last played beat.
    pub fn play_pause(&mut self) {
        if self.playback.state() == PlayState::Playing {
            self.follow_playback();
        }
        // Pausing then playing resumes at the cursor; a fresh start plays from the
        // selection's first note when there is a selection.
        let started = if self.playback.state() == PlayState::Paused {
            None
        } else {
            self.selection_start()
        };
        let Some(document) = &self.document else {
            return;
        };
        let from = started.unwrap_or_else(|| document.editor().cursor().position());
        if let Err(error) = self.playback.play_pause(document.song(), Some(from)) {
            self.notice = Some(error);
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
