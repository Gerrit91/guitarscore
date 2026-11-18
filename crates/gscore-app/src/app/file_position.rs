//! Remembering where the cursor and selection were in each file, so opening it again returns
//! there. A song has no field for this (Guitar Pro has none either), so it is the app's own
//! memory, kept in the settings.

use std::path::Path;

use gscore_edit::{BeatSpot, Cursor, Selection};
use gscore_model::{BeatIndex, MeasureIndex, Song, StaffIndex, TrackIndex, VoiceIndex};

use super::GuitarScoreApp;
use super::tab::Tab;
use crate::settings::{FilePosition, FileSelection};
use crate::state::MeasureSelection;

impl FilePosition {
    /// `tab`'s cursor and selection now, under `path`.
    pub(super) fn of_tab(path: &Path, tab: &Tab) -> Self {
        let editor = tab.document.editor();
        let cursor = editor.cursor();
        FilePosition {
            path: path.to_path_buf(),
            track: cursor.track.get(),
            staff: cursor.staff.get(),
            measure: cursor.measure.get(),
            voice: cursor.voice.get(),
            beat: cursor.beat.get(),
            string: cursor.string,
            shown_track: tab.track,
            selection: editor.selection().map(file_selection),
        }
    }
}

impl GuitarScoreApp {
    /// Puts the active tab's cursor and selection back where `path` was left last time (if
    /// the settings remember it). Parts of the stored place that no longer exist in the song
    /// are clamped or dropped, as an edit or another version of the file may have removed
    /// them.
    pub(super) fn restore_position(&mut self, path: &Path) {
        let Some(stored) = self.settings.position_for(path).cloned() else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut cursor = Cursor {
            track: TrackIndex::new(stored.track),
            staff: StaffIndex::new(stored.staff),
            measure: MeasureIndex::new(stored.measure),
            voice: VoiceIndex::new(stored.voice),
            beat: BeatIndex::new(stored.beat),
            string: stored.string,
        };
        let (selection, measures) = {
            let song = tab.document.song();
            cursor.clamp(song);
            match stored
                .selection
                .and_then(|stored| selection_in(song, stored))
            {
                Some(Selection::Measures {
                    anchor,
                    focus,
                    track,
                }) => (
                    Some(Selection::Measures {
                        anchor,
                        focus,
                        track,
                    }),
                    Some(MeasureSelection {
                        anchor: anchor.get(),
                        focus: focus.get(),
                        track: track.map(TrackIndex::get),
                    }),
                ),
                Some(selection) => (Some(selection), None),
                None => (None, None),
            }
        };
        let tracks = tab.document.song().track_count();
        tab.track = stored.shown_track.min(tracks.saturating_sub(1));
        tab.selection = measures;
        tab.document.editor_mut().set_cursor(cursor);
        tab.document.editor_mut().set_selection(selection);
        // Restoring a selection is not making one: it does not turn Loop on by itself.
        tab.had_loop_selection = tab.has_loop_selection();
    }
}

/// `selection` as the settings keep it.
fn file_selection(selection: Selection) -> FileSelection {
    match selection {
        Selection::Beats {
            track,
            staff,
            voice,
            anchor,
            focus,
        } => FileSelection::Beats {
            track: track.get(),
            staff: staff.get(),
            voice: voice.get(),
            anchor_measure: anchor.measure.get(),
            anchor_beat: anchor.beat.get(),
            focus_measure: focus.measure.get(),
            focus_beat: focus.beat.get(),
        },
        Selection::Measures {
            anchor,
            focus,
            track,
        } => FileSelection::Measures {
            anchor: anchor.get(),
            focus: focus.get(),
            track: track.map(TrackIndex::get),
        },
    }
}

/// The selection `stored` describes, if every part of it still exists in `song`.
fn selection_in(song: &Song, stored: FileSelection) -> Option<Selection> {
    match stored {
        FileSelection::Measures {
            anchor,
            focus,
            track,
        } => {
            let measures = song.measure_count();
            if anchor >= measures || focus >= measures {
                return None;
            }
            let track = match track {
                Some(track) if track < song.track_count() => Some(TrackIndex::new(track)),
                Some(_) => return None,
                None => None,
            };
            Some(Selection::Measures {
                anchor: MeasureIndex::new(anchor),
                focus: MeasureIndex::new(focus),
                track,
            })
        }
        FileSelection::Beats {
            track,
            staff,
            voice,
            anchor_measure,
            anchor_beat,
            focus_measure,
            focus_beat,
        } => {
            let spot = |measure: usize, beat: usize| {
                song.staff(TrackIndex::new(track), StaffIndex::new(staff))
                    .and_then(|track| track.measure(MeasureIndex::new(measure)))
                    .and_then(|measure| measure.voice(VoiceIndex::new(voice)))
                    .and_then(|voice| voice.beat(BeatIndex::new(beat)))
                    .map(|_| BeatSpot {
                        measure: MeasureIndex::new(measure),
                        beat: BeatIndex::new(beat),
                    })
            };
            Some(Selection::Beats {
                track: TrackIndex::new(track),
                staff: StaffIndex::new(staff),
                voice: VoiceIndex::new(voice),
                anchor: spot(anchor_measure, anchor_beat)?,
                focus: spot(focus_measure, focus_beat)?,
            })
        }
    }
}
