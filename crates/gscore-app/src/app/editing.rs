//! Keys and actions: key presses go through the keymap to the document's editor; what the
//! editor hands back (play, save) is done here.

use gscore_edit::{Action, Command, Outcome, Request};

use super::GuitarScoreApp;

impl GuitarScoreApp {
    /// Runs the actions of this frame's key presses. Keys go to the score only when no
    /// text field, menu or dialog has the keyboard; a key that runs an action is taken out
    /// of the input, so no widget handles it as well.
    pub(super) fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.document.is_none()
            || self.dialog.is_some()
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
        let Some(document) = &mut self.document else {
            return;
        };
        // egui's input time, in milliseconds since the app started: enough to tell
        // whether a second fret digit follows quickly.
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        match document.editor_mut().perform(action, now_ms) {
            Ok(Outcome::Edited) => {
                self.status = None;
                if let Some(document) = &mut self.document {
                    // Measure stamps tell the layout cache what changed; track settings
                    // that only affect the page header need no invalidation.
                    let tracks = document.song().track_count();
                    if self.view.track >= tracks {
                        self.view.track = tracks.saturating_sub(1);
                    }
                    if let Some(selection) = self.view.selection
                        && selection.start() >= document.song().measure_count()
                    {
                        self.view.selection = None;
                    }
                }
                // The song changed: playback rebuilds its sequence on the next play.
                self.playback.song_changed();
                ctx.request_repaint();
            }
            Ok(Outcome::Moved) => ctx.request_repaint(),
            Ok(Outcome::Nothing) => {}
            Ok(Outcome::Request(request)) => self.request(request, ctx),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Applies `command` (from a panel or menu) and updates what depends on the song. Mix
    /// commands (volume, pan, mute, solo) take effect in playback at once; any other edit
    /// stops playback and drops the cached layouts.
    pub(crate) fn apply_command(&mut self, command: Command) {
        let Some(document) = &mut self.document else {
            return;
        };
        let changes_mix = command.changes_mix();
        let changes_tracks = command.changes_tracks();
        let result = document.editor_mut().execute_command(command);
        match result {
            Ok(_) => {
                if changes_tracks {
                    document.forget_layouts();
                }
                if changes_mix {
                    if let Err(error) = self.playback.mix_changed(document.song()) {
                        self.notice = Some(error);
                    }
                } else {
                    self.playback.song_changed();
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
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    /// Does what the editor asked the app for.
    pub(super) fn request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::PlayPause => self.play_pause(),
            Request::Stop => {
                if let Err(error) = self.playback.stop() {
                    self.notice = Some(error);
                }
            }
            Request::Save => self.save_song(ctx),
        }
    }

    /// Starts playback of the open song at the cursor, or pauses it.
    pub fn play_pause(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let from = document.editor().cursor().position();
        if let Err(error) = self.playback.play_pause(document.song(), Some(from)) {
            self.notice = Some(error);
        }
    }
}
