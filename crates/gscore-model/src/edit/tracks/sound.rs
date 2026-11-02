//! Track sound edits: the General MIDI program and the drum-kit flag.
//!
//! Both are per-track settings, so they use [`Song::edit_track_settings`] and keep every
//! measure's layout stamp. Playback reads them through [`crate::Song::track_sound`].

use super::super::{Change, ChangeKind, track_mut};
use crate::error::ModelError;
use crate::position::TrackIndex;
use crate::song::Song;

/// The GP7 instrument type of a drum kit, as the GPIF importer and exporter spell it.
const DRUM_KIT: &str = "drumKit";
/// The name the track panel shows for a drum kit that had no instrument type.
const DRUMS: &str = "Drums";

impl Song {
    /// Sets the track's General MIDI program (0-127).
    pub fn set_track_program(&mut self, at: TrackIndex, program: u8) -> Result<Change, ModelError> {
        if program > 127 {
            return Err(ModelError::Invalid(format!(
                "{program} is not a MIDI program (0 to 127)"
            )));
        }
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.midi_program_gpif;
            track.midi_program_gpif = Some(i32::from(program));
            Ok(Change {
                kind: ChangeKind::TrackProgram { at, previous },
            })
        })
    }

    /// Makes the track a drum kit (or an ordinary instrument again). A drum kit's notes are
    /// General MIDI drum sounds on the percussion channel.
    pub fn set_track_drums(&mut self, at: TrackIndex, drums: bool) -> Result<Change, ModelError> {
        self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = track.percussion_track;
            let previous_type = track.instrument_type.clone();
            let previous_name = track.instrument_name.clone();
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
                },
            })
        })
    }
}
