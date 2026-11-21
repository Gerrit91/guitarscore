//! Monitoring a MIDI input (M23 step 4): the notes played on a keyboard or a MIDI guitar
//! are heard at once through the cursor's track's own sound, as live notes beside the
//! sequence. On the modelled strings they sound on the track's string with its chain;
//! otherwise on a channel of their own with the track's SoundFont sound. The latency is
//! the output buffer's (and a frame of the app's).

use std::sync::Arc;

use gscore_model::{Song, TrackIndex};
use gscore_play::{Channel, LiveNote, PlayState, Sequence, TrackEngine, sound_events};

use super::{Output, Playback};

/// The channel monitored notes sound on when the strings do not play them (the entered
/// notes' audition uses the last one).
const MONITOR_CHANNEL: u8 = 14;

impl Playback {
    /// Starts (`velocity` above 0) or ends a note played live on `string` of `track`,
    /// heard through the track's own sound. Nothing happens while a count-in runs or the
    /// output is not ready: monitoring is a convenience, its failures stay quiet.
    pub fn monitor_note(
        &mut self,
        song: &Song,
        track: TrackIndex,
        string: u8,
        key: u8,
        velocity: u8,
    ) -> Result<(), String> {
        if self.count_in_pending()
            || self.connect().is_err()
            || self.is_loading()
            || self.error().is_some()
        {
            return Ok(());
        }
        let Some(view) = song.track(track) else {
            return Ok(());
        };
        let channel = if view.is_percussion() {
            Channel::PERCUSSION
        } else {
            Channel::new(MONITOR_CHANNEL).unwrap_or(Channel::PERCUSSION)
        };
        let index = if self.state() == PlayState::Playing {
            self.playing_strings(track)
        } else {
            self.monitor_strings(song, track)?
        };
        // On the strings, or (any index the strings lack) a note on the channel.
        let note = LiveNote {
            track: index.unwrap_or(usize::MAX),
            channel,
            string,
            key,
            velocity,
        };
        let player = self.player_mut()?;
        if index.is_none()
            && velocity > 0
            && let Some(sound) = song.played_sound(track)
        {
            // The channel may have played something else meanwhile: its sound first.
            for event in sound_events(channel, sound) {
                player.send_event(event).map_err(|e| e.to_string())?;
            }
        }
        player.live_note(note).map_err(|e| e.to_string())
    }

    /// While the song plays: the index of `track` in the playing sequence if the strings
    /// play it.
    fn playing_strings(&self, track: TrackIndex) -> Option<usize> {
        let sequence = self.primary_sequence.as_ref()?;
        sequence
            .performance
            .tracks
            .iter()
            .position(|entry| entry.track == track && is_modelled(entry.engine))
    }

    /// While stopped: makes sure the strings are set up for `track` when they play it (a
    /// note-less excerpt of the track loaded in place of the song), and gives its index.
    fn monitor_strings(&mut self, song: &Song, track: TrackIndex) -> Result<Option<usize>, String> {
        if !matches!(self.output, Output::Synth(_)) {
            return Ok(None);
        }
        if let Some((monitored, excerpt)) = &self.monitor
            && *monitored == track
            && self.preview.is_some()
            && self
                .preview_sequence
                .as_ref()
                .is_some_and(|loaded| Arc::ptr_eq(loaded, excerpt))
        {
            return Ok(Some(0));
        }
        let Some(excerpt) = Sequence::track_excerpt(song, track).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let excerpt = Arc::new(self.engines(excerpt));
        let modelled = excerpt
            .performance
            .tracks
            .first()
            .is_some_and(|entry| is_modelled(entry.engine));
        if !modelled {
            return Ok(None);
        }
        self.load_excerpt(Arc::clone(&excerpt), false)?;
        self.monitor = Some((track, excerpt));
        Ok(Some(0))
    }
}

fn is_modelled(engine: TrackEngine) -> bool {
    matches!(engine, TrackEngine::Modelled(_))
}
