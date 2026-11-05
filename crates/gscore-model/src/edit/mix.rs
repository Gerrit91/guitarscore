//! Display and mix edits that are not one track's sound: which staves a track shows, and
//! the master volume and pan.

use super::{Change, ChangeKind, track_mut};
use crate::error::ModelError;
use crate::playback::midi_to_unit;
use crate::position::TrackIndex;
use crate::song::Song;
use crate::style::TrackStaves;

impl Song {
    /// Chooses which staves the track's page shows. Every measure's layout changes.
    pub fn set_track_staves(
        &mut self,
        at: TrackIndex,
        staves: TrackStaves,
    ) -> Result<Change, ModelError> {
        let change = self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            let previous = (
                track.settings.notation,
                track.settings.tablature,
                track.settings.slash,
            );
            let (notation, tablature, slash) = staves.flags();
            track.settings.notation = notation;
            track.settings.tablature = tablature;
            track.settings.slash = slash;
            Ok(Change {
                kind: ChangeKind::TrackStaves { at, previous },
            })
        })?;
        self.touch_all_measures();
        Ok(change)
    }

    /// Sets the clef a measure's standard-notation staff uses.
    pub fn set_clef(
        &mut self,
        track: TrackIndex,
        measure: crate::position::MeasureIndex,
        clef: crate::style::Clef,
    ) -> Result<Change, ModelError> {
        self.edit_measure(track, measure, |m| {
            let previous = (clef_of(m), m.has_clef);
            m.clef = clef.to_gp();
            m.has_clef = true;
            Ok(Change {
                kind: ChangeKind::Clef {
                    track,
                    measure,
                    previous,
                },
            })
        })
    }

    /// Sets the master volume (0-127; [`crate::MASTER_UNITY`] leaves every track as loud as
    /// it is).
    pub fn set_master_volume(&mut self, volume: u8) -> Result<Change, ModelError> {
        if volume > 127 {
            return Err(ModelError::Invalid(format!(
                "master volume {volume} (supported: 0 to 127)"
            )));
        }
        let pan = midi_to_unit(self.master_mix().pan);
        self.set_master(midi_to_unit(volume), pan)
    }

    /// Sets the master pan (0 left, 64 leaves every track's pan as it is, 127 right).
    pub fn set_master_pan(&mut self, pan: u8) -> Result<Change, ModelError> {
        if pan > 127 {
            return Err(ModelError::Invalid(format!(
                "master pan {pan} (supported: 0 to 127)"
            )));
        }
        let volume = self
            .inner()
            .master_volume_pan
            .map_or(midi_to_unit(crate::MASTER_UNITY), |(volume, _)| volume);
        self.set_master(volume, midi_to_unit(pan))
    }

    fn set_master(&mut self, volume: f32, pan: f32) -> Result<Change, ModelError> {
        self.edit_metadata(|inner| {
            let previous = inner.master_volume_pan.replace((volume, pan));
            Ok(Change {
                kind: ChangeKind::MasterMix { previous },
            })
        })
    }
}

/// The clef of a measure, also when the file did not store one (Guitar Pro's default).
fn clef_of(measure: &gscore_gp::Measure) -> crate::style::Clef {
    crate::style::Clef::from_gp(&measure.clef)
}
