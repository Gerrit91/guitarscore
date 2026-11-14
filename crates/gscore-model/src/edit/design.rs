//! Guitar Pro's design mode: a bar's width scale and a beat column's offset.
//!
//! Both are layout data, not music: they change how a bar is drawn, never what it sounds
//! like. A bar's scale may be set for the whole song (Guitar Pro's master bar) or for one
//! staff (its bar), which overrides the song's; a beat's offset is stored on the beat that
//! starts its column.

use super::{Change, ChangeKind};
use crate::error::ModelError;
use crate::position::{BeatPosition, MeasureIndex, StaffIndex, TrackIndex};
use crate::song::Song;

/// The width scale a bar may have (Guitar Pro's design mode).
pub const DESIGN_SCALE_RANGE: (f32, f32) = gscore_gp::SCALE_RANGE;

/// How far a beat's column may be moved from its natural place, in staff spaces.
pub const DESIGN_SPACING_RANGE: (f32, f32) = gscore_gp::SPACING_RANGE;

/// Offsets are kept to the nearest thousandth of a staff space: finer than a printed point
/// and enough to make a number round-trip exactly.
const SPACING_STEPS: f32 = 1000.0;

/// Which bars a width scale is set for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleScope {
    /// Every track's bar at the measure (Guitar Pro's master bar).
    Song,
    /// One staff's own bar; it overrides the song's scale.
    Staff {
        /// The track.
        track: TrackIndex,
        /// Which staff of it.
        staff: StaffIndex,
    },
}

/// `scale` checked and moved into range, or an error naming what is wrong.
fn checked_scale(scale: Option<f32>) -> Result<Option<f32>, ModelError> {
    match scale {
        None => Ok(None),
        Some(scale) if !scale.is_finite() => Err(ModelError::Invalid(format!(
            "a bar width of {scale} is not a number"
        ))),
        Some(scale) if !(DESIGN_SCALE_RANGE.0..=DESIGN_SCALE_RANGE.1).contains(&scale) => {
            Err(ModelError::Invalid(format!(
                "a bar width of {scale} is outside {} to {}",
                DESIGN_SCALE_RANGE.0, DESIGN_SCALE_RANGE.1
            )))
        }
        Some(scale) => Ok(Some(scale)),
    }
}

/// `spacing` rounded to [`SPACING_STEPS`] and checked, or an error naming what is wrong.
/// A value that rounds to nothing is `None`.
fn checked_spacing(spacing: Option<f32>) -> Result<Option<f32>, ModelError> {
    match spacing {
        None => Ok(None),
        Some(spacing) if !spacing.is_finite() => Err(ModelError::Invalid(format!(
            "a beat offset of {spacing} is not a number"
        ))),
        Some(spacing) if !(DESIGN_SPACING_RANGE.0..=DESIGN_SPACING_RANGE.1).contains(&spacing) => {
            Err(ModelError::Invalid(format!(
                "a beat offset of {spacing} is outside {} to {} staff spaces",
                DESIGN_SPACING_RANGE.0, DESIGN_SPACING_RANGE.1
            )))
        }
        Some(spacing) => {
            let rounded = (spacing * SPACING_STEPS).round() / SPACING_STEPS;
            Ok((rounded.abs() >= 0.0005).then_some(rounded))
        }
    }
}

impl Song {
    /// The width scale in effect for `measure` of one staff of `track`: the staff's own bar
    /// when it has one, else the song's, else 1.
    pub fn measure_scale(
        &self,
        track: TrackIndex,
        staff: StaffIndex,
        measure: MeasureIndex,
    ) -> f32 {
        let own = self
            .staff(track, staff)
            .and_then(|view| view.measure(measure))
            .and_then(|measure| measure.display_scale());
        own.or_else(|| self.song_measure_scale(measure))
            .map_or(1.0, |scale| {
                scale.clamp(DESIGN_SCALE_RANGE.0, DESIGN_SCALE_RANGE.1)
            })
    }

    /// The song-wide width scale of `measure` (Guitar Pro's master bar), or `None` when the
    /// song has none.
    pub fn song_measure_scale(&self, measure: MeasureIndex) -> Option<f32> {
        self.inner()
            .measure_headers
            .get(measure.get())
            .and_then(|header| header.display_scale())
    }

    /// Sets the width scale of a bar (`None`: back to its natural width). A song-wide scale
    /// applies to every track at that measure; a staff's scale overrides it for that bar.
    pub fn set_measure_scale(
        &mut self,
        scope: ScaleScope,
        measure: MeasureIndex,
        scale: Option<f32>,
    ) -> Result<Change, ModelError> {
        let scale = checked_scale(scale)?;
        match scope {
            ScaleScope::Song => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                let previous = header.display_scale();
                header.set_display_scale(scale);
                Ok(Change {
                    kind: ChangeKind::MeasureScale {
                        scope,
                        measure,
                        previous,
                    },
                })
            }),
            ScaleScope::Staff { track, staff } => {
                let previous = self.edit_measure_of(track, staff, measure, |measure| {
                    let previous = measure.display_scale();
                    measure.set_display_scale(scale);
                    Ok(previous)
                })?;
                Ok(Change {
                    kind: ChangeKind::MeasureScale {
                        scope,
                        measure,
                        previous,
                    },
                })
            }
        }
    }

    /// Moves the beat's column by `spacing` staff spaces from its natural place (negative
    /// moves it left, `None` puts it back). The column is what every voice and staff that
    /// starts at the same time moves with; the layout reads the first moved beat of it.
    pub fn set_beat_spacing(
        &mut self,
        at: BeatPosition,
        spacing: Option<f32>,
    ) -> Result<Change, ModelError> {
        let spacing = checked_spacing(spacing)?;
        self.edit_measure_at(at, |measure| {
            let beat = super::beat_in(measure, at)?;
            let previous = std::mem::replace(&mut beat.spacing, spacing);
            Ok(Change {
                kind: ChangeKind::BeatSpacing { at, previous },
            })
        })
    }
}
