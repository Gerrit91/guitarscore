//! A tab: one open song together with the view state that belongs to it alone — the shown
//! track, the measure selection, and the panel drafts. What is shared between tabs (theme,
//! zoom, which panels are visible) stays on the app.

use std::collections::HashMap;
use std::time::SystemTime;

use gscore_edit::Cursor;

use crate::document::Document;
use crate::state::MeasureSelection;

use super::edit_palette::EditPaletteState;
use super::grid::{MixerState, RenameTrack};
use super::new_track::NewTrackDialog;
use super::track_panel::TrackPanelState;

/// A tab's identity for as long as it is open. Unlike its index, it does not change when
/// another tab closes, so work that finishes later (a save, a question) finds the right tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabId(u64);

/// One open song and the view state that belongs to it.
#[derive(Debug)]
pub(super) struct Tab {
    pub(super) id: TabId,
    pub(super) document: Document,
    /// The shown track (0-based).
    pub(super) track: usize,
    /// The selected measure range in the song grid, if any.
    pub(super) selection: Option<MeasureSelection>,
    /// The right panel's state: the shown track, and the name being typed.
    pub(super) track_panel: TrackPanelState,
    /// The left panel's state: the chosen note length and header fields.
    pub(super) edit_palette: EditPaletteState,
    /// Where a drag on the page started, while selecting beats.
    pub(super) drag_anchor: Option<Cursor>,
    /// The cursor and selection the edit palette showed when playback started. While the
    /// song plays, the palette keeps showing those notes instead of following the playing
    /// cursor, whose constantly changing tiles are distracting.
    pub(super) palette_anchor: Option<(Cursor, Option<gscore_edit::Selection>)>,
    /// The cursor and revision the view last scrolled to, to scroll only on a change.
    pub(super) shown_cursor: Option<(Cursor, u64)>,
    /// In the page view: the cursor's page and the page count (1-based), for the status bar.
    pub(super) page_info: Option<(usize, usize)>,
    /// The track being renamed, while the rename window is open.
    pub(super) renaming: Option<RenameTrack>,
    /// State of the mixer view between frames (the decibel field being typed).
    pub(super) mixer: MixerState,
    /// Wall-clock time when each history entry was first seen, by the entry's stable id.
    pub(super) history_stamps: HashMap<u64, SystemTime>,
    /// The new-track popup, while it is open.
    pub(super) new_track: Option<NewTrackDialog>,
    /// Which tracks the score shows besides the cursor's.
    pub(super) score: super::score_view::ScoreTracks,
    /// Whether the last frame had a selection of more than one beat: a new one turns
    /// Loop on.
    pub(super) had_loop_selection: bool,
}

impl Tab {
    /// The measures a loop plays: the grid's measure selection, or the measures a
    /// selection made in the score spans.
    pub(super) fn loop_measures(&self) -> Option<MeasureSelection> {
        self.selection.or_else(|| {
            self.document
                .editor()
                .selection()
                .map(|selection| MeasureSelection {
                    anchor: selection.first_measure().get(),
                    focus: selection.last_measure().get(),
                })
        })
    }

    /// The tracks the score shows, top to bottom (the cursor's among them).
    pub(super) fn shown_tracks(&self) -> Vec<usize> {
        self.score
            .tracks(self.track, self.document.song().track_count())
    }

    /// A tab for `document` with the id number `id`, showing its first track and nothing
    /// selected.
    pub(super) fn new(id: u64, document: Document) -> Self {
        Tab {
            id: TabId(id),
            document,
            track: 0,
            selection: None,
            track_panel: TrackPanelState::default(),
            edit_palette: EditPaletteState::default(),
            drag_anchor: None,
            palette_anchor: None,
            shown_cursor: None,
            page_info: None,
            renaming: None,
            mixer: MixerState::default(),
            history_stamps: HashMap::new(),
            new_track: None,
            score: super::score_view::ScoreTracks::default(),
            had_loop_selection: false,
        }
    }

    /// Whether more than one beat is selected (beats in the score, or bars in the grid or
    /// the score): what Loop turns on for.
    pub(super) fn has_loop_selection(&self) -> bool {
        if self.selection.is_some() {
            return true;
        }
        match self.document.editor().selection() {
            Some(gscore_edit::Selection::Beats { anchor, focus, .. }) => anchor != focus,
            Some(gscore_edit::Selection::Measures { .. }) => true,
            None => false,
        }
    }
}
