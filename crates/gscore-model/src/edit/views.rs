//! Score views and Guitar Pro's per-track eye: which tracks a score shows.

use super::{Change, ChangeKind, track_mut};
use crate::error::ModelError;
use crate::position::TrackIndex;
use crate::score_views::{MAX_SCORE_VIEWS, MAX_VIEW_NAME_CHARS, ScoreView, StoredView, ViewTrack};
use crate::song::Song;

impl Song {
    /// GuitarScore's score views, in their order, with their tracks as they are now. A
    /// track deleted since it was added is left out (an undo brings it back).
    pub fn score_views(&self) -> Vec<ScoreView> {
        self.score_views
            .iter()
            .map(|view| ScoreView {
                name: view.name.clone(),
                tracks: view
                    .tracks
                    .iter()
                    .filter_map(|&(key, staves)| {
                        let track = self
                            .inner()
                            .tracks
                            .iter()
                            .position(|track| track.session_key == key)?;
                        Some(ViewTrack {
                            track: TrackIndex::new(track),
                            staves,
                        })
                    })
                    .collect(),
                hide_empty_staves: view.hide_empty_staves,
            })
            .collect()
    }

    /// Replaces the score views with `views`. Each needs a name of its own (at most
    /// [`MAX_VIEW_NAME_CHARS`] characters) and at least one track, each track once; a song
    /// keeps at most [`MAX_SCORE_VIEWS`]. The views have no field in the Guitar Pro
    /// formats; the app saves them in the song's notes.
    pub fn set_score_views(&mut self, views: &[ScoreView]) -> Result<Change, ModelError> {
        if views.len() > MAX_SCORE_VIEWS {
            return Err(ModelError::Invalid(format!(
                "a song keeps at most {MAX_SCORE_VIEWS} score views"
            )));
        }
        let count = self.track_count();
        let mut stored = Vec::with_capacity(views.len());
        for (index, view) in views.iter().enumerate() {
            let name = view.name.trim();
            if name.is_empty() {
                return Err(ModelError::Invalid("a score view needs a name".to_owned()));
            }
            if name.chars().count() > MAX_VIEW_NAME_CHARS {
                return Err(ModelError::Invalid(format!(
                    "a view name has at most {MAX_VIEW_NAME_CHARS} characters"
                )));
            }
            if views
                .get(..index)
                .unwrap_or_default()
                .iter()
                .any(|other| other.name.trim().eq_ignore_ascii_case(name))
            {
                return Err(ModelError::Invalid(format!(
                    "there is already a view called \"{name}\""
                )));
            }
            if view.tracks.is_empty() {
                return Err(ModelError::Invalid(format!(
                    "the view \"{name}\" shows no track"
                )));
            }
            let mut tracks = Vec::with_capacity(view.tracks.len());
            for (position, entry) in view.tracks.iter().enumerate() {
                let key = self
                    .inner()
                    .tracks
                    .get(entry.track.get())
                    .map(|track| track.session_key)
                    .ok_or_else(|| {
                        ModelError::Invalid(format!(
                            "{}: the song has {count} track(s)",
                            entry.track
                        ))
                    })?;
                if view
                    .tracks
                    .get(..position)
                    .unwrap_or_default()
                    .iter()
                    .any(|other| other.track == entry.track)
                {
                    return Err(ModelError::Invalid(format!(
                        "the view \"{name}\" shows {} twice",
                        entry.track
                    )));
                }
                tracks.push((key, entry.staves));
            }
            stored.push(StoredView {
                name: name.to_owned(),
                tracks,
                hide_empty_staves: view.hide_empty_staves,
            });
        }
        let previous = std::mem::replace(&mut self.score_views, stored);
        Ok(Change {
            kind: ChangeKind::ScoreViews { previous },
        })
    }

    /// Shows or hides `track` when all tracks are shown together (Guitar Pro's eye in its
    /// track list, saved in GP5 and GP7 files). One track always stays shown.
    pub fn set_track_visible(
        &mut self,
        at: TrackIndex,
        visible: bool,
    ) -> Result<Change, ModelError> {
        let others_shown = self
            .tracks()
            .any(|track| track.index() != at && track.is_visible());
        if !visible && !others_shown {
            return Err(ModelError::Invalid(
                "one track always stays shown".to_owned(),
            ));
        }
        let previous = self.edit_track_settings(|inner| {
            let track = track_mut(inner, at)?;
            Ok(std::mem::replace(&mut track.visible, visible))
        })?;
        Ok(Change {
            kind: ChangeKind::TrackVisible { at, previous },
        })
    }

    /// Puts back views replaced by [`Self::set_score_views`] (undo).
    pub(super) fn restore_score_views(&mut self, previous: &[StoredView]) {
        self.score_views = previous.to_vec();
    }
}
