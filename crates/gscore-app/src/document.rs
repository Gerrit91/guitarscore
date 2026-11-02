//! The open document: the song being edited (through a [`TabEditor`]), where it came from,
//! whether it has unsaved changes, its layouts, and its recovery journal.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gscore_edit::TabEditor;
use gscore_layout::{Fonts, Layout, LayoutCache, LayoutOptions};
use gscore_model::{MeasureIndex, Repeat, Section, Song, TrackIndex};
use gscore_play::Timeline;
use gscore_session::{FileFormat, Journal};

/// One track's layout: the revision and options it shows, and the measure cache that
/// makes the next layout after an edit cheap.
#[derive(Debug, Default)]
struct TrackLayout {
    shown: Option<(u64, LayoutOptions, Result<Arc<Layout>, String>)>,
    cache: LayoutCache,
}

/// What the song grid shows: which measures have notes (one row per track), and the
/// section markers and repeats above the measure columns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GridData {
    /// One row per track, one `true` per measure that holds notes.
    pub cells: Vec<Vec<bool>>,
    /// The section marker at each measure, if any.
    pub sections: Vec<Option<Section>>,
    /// The repeat signs at each measure.
    pub repeats: Vec<Repeat>,
}

/// A song open in the app.
#[derive(Debug)]
pub struct Document {
    path: PathBuf,
    /// The format the file is in; only `.gp` files are saved in place.
    format: FileFormat,
    editor: TabEditor,
    /// The editor revision last saved (or opened).
    saved_revision: u64,
    /// Layouts by track. An edit makes them stale; the shown track is laid out again on
    /// the next frame, reusing every measure that did not change.
    layouts: HashMap<usize, TrackLayout>,
    /// The song grid's squares and markers, with the revision they show.
    grid: Option<(u64, Arc<GridData>)>,
    /// The song's played measures and tempo, with the revision they show, for the time
    /// readout.
    timeline: Option<(u64, Option<Arc<Timeline>>)>,
    /// The recovery journal every edit is recorded in (`None` without a recovery
    /// directory, or if it could not be created).
    journal: Option<Journal>,
    /// The journal's error, once reported.
    journal_reported: bool,
}

impl Document {
    /// A document for `song`, read from `path` in `format`.
    pub fn new(path: PathBuf, song: Song, format: FileFormat) -> Self {
        Document::with_editor(path, TabEditor::new(song, TrackIndex::new(0)), format, None)
    }

    /// A document whose edits are recorded in `journal`.
    pub fn with_journal(path: PathBuf, song: Song, format: FileFormat, journal: Journal) -> Self {
        Document::with_editor(
            path,
            TabEditor::new(song, TrackIndex::new(0)),
            format,
            Some(journal),
        )
    }

    /// A recovered document: `editor` holds the replayed edits (not saved yet).
    pub fn recovered(
        path: PathBuf,
        editor: TabEditor,
        format: FileFormat,
        journal: Option<Journal>,
    ) -> Self {
        let mut document = Document::with_editor(path, editor, format, journal);
        // Nothing of it is saved: no revision ever matches.
        document.saved_revision = u64::MAX;
        document
    }

    fn with_editor(
        path: PathBuf,
        editor: TabEditor,
        format: FileFormat,
        journal: Option<Journal>,
    ) -> Self {
        Document {
            path,
            format,
            saved_revision: editor.revision(),
            editor,
            layouts: HashMap::new(),
            grid: None,
            timeline: None,
            journal,
            journal_reported: false,
        }
    }

    /// Hands the edits since the last call to the recovery journal (the writing happens
    /// in the background). Returns the journal's error the first time there is one.
    pub fn record_changes(&mut self) -> Option<String> {
        let applied = self.editor.take_applied();
        let journal = self.journal.as_mut()?;
        journal.record(&applied, self.editor.song());
        if self.journal_reported {
            return None;
        }
        let error = journal.error()?;
        self.journal_reported = true;
        Some(format!(
            "{error}. Edits are not protected against a crash; save often."
        ))
    }

    /// Writes a recovery snapshot of the song now (after a contained panic).
    pub fn snapshot(&mut self) {
        let _ = self.record_changes();
        if let Some(journal) = &mut self.journal {
            journal.snapshot(self.editor.song());
        }
    }

    /// Whether edits are recorded for crash recovery.
    pub fn is_journaled(&self) -> bool {
        self.journal.is_some()
    }

    /// The document is closed normally (saved, or its changes discarded): its recovery
    /// folder goes. The journal finishes on a thread of its own, unless `wait` (at exit).
    pub fn close(mut self, wait: bool) {
        let _ = self.record_changes();
        if let Some(journal) = self.journal.take() {
            if wait {
                journal.close();
            } else {
                // The thread only exists to keep file work off the UI thread; if it cannot
                // start, the folder stays and is cleaned up on the next start.
                let _ = std::thread::Builder::new()
                    .name("gscore-journal-close".to_owned())
                    .spawn(move || journal.close());
            }
        }
    }

    /// The file the song came from (or was last saved to).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The song as edited so far.
    pub fn song(&self) -> &Song {
        self.editor.song()
    }

    /// The editor: cursor, history, revision.
    pub fn editor(&self) -> &TabEditor {
        &self.editor
    }

    /// The editor, to edit.
    pub fn editor_mut(&mut self) -> &mut TabEditor {
        &mut self.editor
    }

    /// Whether the song changed since it was opened or last saved.
    pub fn is_dirty(&self) -> bool {
        self.editor.revision() != self.saved_revision
    }

    /// Whether Save can write back to the same file: only Guitar Pro 7+ files can be
    /// written, and a file opened in another format is never overwritten in a new one.
    pub fn saves_in_place(&self) -> bool {
        self.format == FileFormat::Gp
    }

    /// Records that `revision` of the song was saved to `path` (as `.gp`). The journal
    /// learns of it only if nothing changed while saving: later edits are not saved yet
    /// and must stay recoverable.
    pub fn mark_saved(&mut self, path: PathBuf, revision: u64) {
        let _ = self.record_changes();
        if revision == self.editor.revision()
            && let Some(journal) = &mut self.journal
        {
            journal.saved(&path);
        }
        self.path = path;
        self.format = FileFormat::Gp;
        self.saved_revision = revision;
    }

    /// The title to show: the song's title, or the file name.
    pub fn display_title(&self) -> String {
        if self.song().title().is_empty() {
            self.path.file_name().map_or_else(
                || "Untitled".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
        } else {
            self.song().title().to_owned()
        }
    }

    /// Drops the cached track layouts. Call after an edit that changes the score or the
    /// track order (the layout cache is keyed by track index).
    pub fn forget_layouts(&mut self) {
        self.layouts.clear();
    }

    /// What the song grid shows for the current revision, computed once per revision.
    pub fn grid(&mut self) -> Arc<GridData> {
        let revision = self.editor.revision();
        if let Some((cached, grid)) = &self.grid
            && *cached == revision
        {
            return Arc::clone(grid);
        }
        let song = self.editor.song();
        let measures = song.measure_count();
        let cells = song
            .tracks()
            .map(|track| {
                (0..measures)
                    .map(|index| {
                        track
                            .measure(MeasureIndex::new(index))
                            .is_some_and(|measure| measure.has_notes())
                    })
                    .collect()
            })
            .collect();
        let grid = Arc::new(GridData {
            cells,
            sections: (0..measures)
                .map(|index| song.section(MeasureIndex::new(index)))
                .collect(),
            repeats: (0..measures)
                .map(|index| song.repeat(MeasureIndex::new(index)).unwrap_or_default())
                .collect(),
        });
        self.grid = Some((revision, Arc::clone(&grid)));
        grid
    }

    /// The song's played measures and tempo for the current revision, computed once per
    /// revision; `None` if the song's timing cannot be read.
    pub fn timeline(&mut self) -> Option<Arc<Timeline>> {
        let revision = self.editor.revision();
        if let Some((cached, timeline)) = &self.timeline
            && *cached == revision
        {
            return timeline.clone();
        }
        let timeline = Timeline::new(self.editor.song()).ok().map(Arc::new);
        self.timeline = Some((revision, timeline.clone()));
        timeline
    }

    /// The layout of `track` for the current revision. A panic inside layout is contained
    /// and shown as an error (the panic hook has already written a crash report).
    pub fn layout(
        &mut self,
        track: usize,
        options: &LayoutOptions,
        fonts: &Fonts,
    ) -> Result<Arc<Layout>, String> {
        let revision = self.editor.revision();
        let entry = self.layouts.entry(track).or_default();
        if let Some((shown, shown_options, layout)) = &entry.shown
            && *shown == revision
            && shown_options == options
        {
            return layout.clone();
        }
        let song = self.editor.song();
        let cache = &mut entry.cache;
        let laid_out = catch_unwind(AssertUnwindSafe(|| {
            cache.layout(song, TrackIndex::new(track), options, fonts)
        }));
        let layout = match laid_out {
            Ok(Ok(layout)) => Ok(Arc::new(layout)),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => {
                // The cache may be half updated: start it afresh.
                entry.cache = LayoutCache::default();
                Err("drawing this track failed unexpectedly; a crash report was written".to_owned())
            }
        };
        entry.shown = Some((revision, options.clone(), layout.clone()));
        layout
    }
}
