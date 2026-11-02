//! The open document: the song being edited (through a [`TabEditor`]), where it came from,
//! whether it has unsaved changes, and its layouts.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gscore_edit::TabEditor;
use gscore_layout::{Fonts, Layout, LayoutOptions, layout_track};
use gscore_model::{Song, TrackIndex};
use gscore_session::FileFormat;

/// A layout and the song revision it was made from.
type CachedLayout = (u64, Result<Arc<Layout>, String>);

/// A song open in the app.
#[derive(Debug)]
pub struct Document {
    path: PathBuf,
    /// The format the file is in; only `.gp` files are saved in place.
    format: FileFormat,
    editor: TabEditor,
    /// The editor revision last saved (or opened).
    saved_revision: u64,
    /// Layouts by track, each with the revision it shows. An edit makes them stale; the
    /// shown track is laid out again on the next frame (about 15 ms for 236 measures in a
    /// release build; per-measure caching is still open, docs/gaps.md).
    layouts: HashMap<usize, CachedLayout>,
}

impl Document {
    /// A document for `song`, read from `path` in `format`.
    pub fn new(path: PathBuf, song: Song, format: FileFormat) -> Self {
        Document {
            path,
            format,
            editor: TabEditor::new(song, TrackIndex::new(0)),
            saved_revision: 0,
            layouts: HashMap::new(),
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

    /// Records that `revision` of the song was saved to `path` (as `.gp`).
    pub fn mark_saved(&mut self, path: PathBuf, revision: u64) {
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

    /// The layout of `track` for the current revision. A panic inside layout is contained
    /// and shown as an error (the panic hook has already written a crash report).
    pub fn layout(
        &mut self,
        track: usize,
        options: &LayoutOptions,
        fonts: &Fonts,
    ) -> Result<Arc<Layout>, String> {
        let revision = self.editor.revision();
        if let Some((cached, layout)) = self.layouts.get(&track)
            && *cached == revision
        {
            return layout.clone();
        }
        let song = self.editor.song();
        let laid_out = catch_unwind(AssertUnwindSafe(|| {
            layout_track(song, TrackIndex::new(track), options, fonts)
        }));
        let layout = match laid_out {
            Ok(Ok(layout)) => Ok(Arc::new(layout)),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => {
                Err("drawing this track failed unexpectedly; a crash report was written".to_owned())
            }
        };
        self.layouts.insert(track, (revision, layout.clone()));
        layout
    }
}
