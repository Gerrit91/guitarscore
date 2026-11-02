//! Keys and actions: key presses go through the keymap to the document's editor; what the
//! editor hands back (play, save) is done here.

use std::panic::{AssertUnwindSafe, catch_unwind};

use gscore_edit::{Action, ActionId, Command, Cursor, Outcome, Request};
use gscore_model::TrackIndex;
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
                self.after_edit(effect, layout);
                ctx.request_repaint();
            }
            Ok(Outcome::Moved) => ctx.request_repaint(),
            Ok(Outcome::Nothing) => {}
            Ok(Outcome::Request(request)) => self.request(request, ctx),
            Err(error) => self.notice = Some(error.to_string()),
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
