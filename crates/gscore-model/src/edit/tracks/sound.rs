//! Track sound edits: the General MIDI program, the drum-kit flag, and GuitarScore's own
//! opaque sound payload (which the app saves in the song's notes, not in the file).
//!
//! All are per-track settings, so they use [`Song::edit_track_settings`] and keep every
//! measure's layout stamp. Playback reads the program and the drum flag through
//! [`crate::Song::track_sound`], and the app payload through
//! [`crate::TrackView::sound_state`].

use super::super::{Change, ChangeKind, RseSound, track_mut};
use crate::error::ModelError;
use crate::position::TrackIndex;
use crate::song::Song;

/// The GP7 instrument type of a drum kit, as the GPIF importer and exporter spell it.
const DRUM_KIT: &str = "drumKit";
/// The name the track panel shows for a drum kit that had no instrument type.
const DRUMS: &str = "Drums";

/// Longest sound-state payload accepted for one track. Ours is a few kilobytes; this
/// bounds a hostile file or a hand-edited notes block.
const MAX_TRACK_SOUND_STATE_BYTES: usize = 256 * 1024;

impl Song {
    /// Sets the track's General MIDI program (0-127).
    pub fn set_track_program(&mut self, at: TrackIndex, program: u8) -> Result<Change, ModelError> {
        if program > 127 {
            return Err(ModelError::Invalid(format!(
                "{program} is not a MIDI program (0 to 127)"
            )));
        }
        // The program it plays now (a GP3-5 track keeps it in its channel).
        let playing = self.track_sound(at).map(|sound| sound.program);
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.midi_program_gpif;
            track.midi_program_gpif = Some(i32::from(program));
            // Another instrument: the GP7 sound chosen for the old one no longer fits.
            let previous_rse = if playing == Some(program) {
                None
            } else {
                Some(RseSound::take(track))
            };
            Ok(Change {
                kind: ChangeKind::TrackProgram {
                    at,
                    previous,
                    previous_rse,
                },
            })
        })
    }

    /// Makes the track a drum kit (or an ordinary instrument again). A drum kit's notes are
    /// General MIDI drum sounds on the percussion channel. A track that becomes a kit starts
    /// on its percussion staff alone: its tablature switch (the drum tab) goes off.
    pub fn set_track_drums(&mut self, at: TrackIndex, drums: bool) -> Result<Change, ModelError> {
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.percussion_track;
            let previous_type = track.instrument_type.clone();
            let previous_name = track.instrument_name.clone();
            // A kit and an instrument have different GP7 sounds.
            let previous_rse = if previous == drums {
                None
            } else {
                Some(RseSound::take(track))
            };
            let previous_staves = (
                track.settings.notation,
                track.settings.tablature,
                track.settings.slash,
            );
            if drums && !previous {
                track.settings.notation = true;
                track.settings.tablature = false;
            }
            track.percussion_track = drums;
            if drums {
                // GP7 stores the kit in the instrument, not only in the flag; otherwise a
                // save would read back as the old instrument.
                track.instrument_type = Some(DRUM_KIT.to_owned());
                track.instrument_name = Some(DRUMS.to_owned());
            } else if track.instrument_type.as_deref() == Some(DRUM_KIT) {
                track.instrument_type = None;
                track.instrument_name = None;
            }
            Ok(Change {
                kind: ChangeKind::TrackDrums {
                    at,
                    previous,
                    previous_type,
                    previous_name,
                    previous_staves,
                    previous_rse,
                },
            })
        })
    }

    /// Sets GuitarScore's own sound state for the track: the opaque payload the app
    /// serializes (what plays it, its tone and chain, its sends, ...), or `None` for a
    /// track at its defaults. It has no field in the Guitar Pro format and is saved in the
    /// song's notes; keeping it on the track means every track edit and its undo carry it
    /// along like the name or the color.
    pub fn set_track_sound_state(
        &mut self,
        at: TrackIndex,
        state: Option<String>,
    ) -> Result<Change, ModelError> {
        if state
            .as_ref()
            .is_some_and(|state| state.len() > MAX_TRACK_SOUND_STATE_BYTES)
        {
            return Err(ModelError::Invalid(format!(
                "the track's sound state is longer than {MAX_TRACK_SOUND_STATE_BYTES} bytes"
            )));
        }
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = std::mem::replace(&mut track.sound_state, state);
            Ok(Change {
                kind: ChangeKind::TrackSoundState { at, previous },
            })
        })
    }
}
