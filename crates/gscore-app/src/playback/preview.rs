//! Hearing a beat with its effects: after a palette click (an effect, a dynamic), the beat
//! is played alone through the sequencer, so a vibrato, a ghost note or a forte is heard
//! as the song will play it. The excerpt borrows the player while the song is stopped;
//! pressing Play (or a change of the mix) ends it and loads the song again.

use std::sync::Arc;

use gscore_model::{BeatPosition, Song};
use gscore_play::{PlayState, Sequence};

use super::Playback;

impl Playback {
    /// Plays the beat at `at` alone, with its effects. Nothing happens while the song
    /// plays, is paused or counts in, for a beat with no sounding note, or while the
    /// output is unavailable or still loading (like auditioning an entered note).
    pub fn preview_beat(&mut self, song: &Song, at: BeatPosition) -> Result<(), String> {
        let previewing = self.preview.is_some();
        if (!previewing && self.state() != PlayState::Stopped) || self.count_in_pending() {
            return Ok(());
        }
        let Some(excerpt) = Sequence::beat_excerpt(song, at).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        if self.connect().is_err() || self.error().is_some() {
            return Ok(());
        }
        if self.is_loading() {
            // The first click starts the synthesizer: play the beat once it is ready.
            self.pending_preview = Some(Arc::new(excerpt));
            return Ok(());
        }
        self.pending_preview = None;
        self.play_excerpt(Arc::new(excerpt))
    }

    /// Loads and plays `excerpt` in place of the song.
    fn play_excerpt(&mut self, excerpt: Arc<Sequence>) -> Result<(), String> {
        self.end_audition();
        let rest = self.preview.unwrap_or_else(|| self.output_position());
        let player = self.player_mut()?;
        player.stop().map_err(|e| e.to_string())?;
        player
            .load(Arc::clone(&excerpt))
            .map_err(|e| e.to_string())?;
        player.play().map_err(|e| e.to_string())?;
        self.preview_sequence = Some(excerpt);
        // The song's sequence is no longer loaded: the next play loads it again.
        self.sequence = None;
        self.preview = Some(rest);
        Ok(())
    }

    /// Plays a beat chosen while the SoundFont was loading, once it has loaded. Returns
    /// whether one is still waiting (so the caller keeps checking).
    pub fn poll_preview(&mut self) -> Result<bool, String> {
        if self.pending_preview.is_none() {
            return Ok(false);
        }
        if self.is_loading() {
            return Ok(true);
        }
        let ready = self.state() == PlayState::Stopped && !self.count_in_pending();
        if let Some(excerpt) = self.pending_preview.take()
            && ready
            && self.error().is_none()
        {
            self.play_excerpt(excerpt)?;
        }
        Ok(false)
    }

    /// The beat excerpt last played, if any (for tests and diagnostics).
    pub fn preview_sequence(&self) -> Option<&Arc<Sequence>> {
        self.preview_sequence.as_ref()
    }

    /// Whether a beat excerpt is loaded (playing or finished).
    pub fn previewing(&self) -> bool {
        self.preview.is_some()
    }

    /// Stops a beat excerpt, if one is loaded. Returns whether there was one.
    pub(super) fn end_preview(&mut self) -> bool {
        self.pending_preview = None;
        if self.preview.take().is_none() {
            return false;
        }
        if let Some(active) = &mut self.player {
            let _ = active.player_mut().stop();
        }
        self.sequence = None;
        true
    }
}
