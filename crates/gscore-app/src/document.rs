//! The open document: the song, where it came from, and its layouts.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;

use gscore_layout::{Fonts, Layout, LayoutOptions, layout_track};
use gscore_model::{Song, TrackIndex};

/// A song shown in the app (read-only for now).
#[derive(Debug)]
pub struct Document {
    path: PathBuf,
    song: Song,
    /// Layouts by track. The song does not change yet, so they stay valid; with editing
    /// (M7) they are invalidated per measure and revision.
    layouts: HashMap<usize, Result<Arc<Layout>, String>>,
}

impl Document {
    /// A document for `song`, read from `path`.
    pub fn new(path: PathBuf, song: Song) -> Self {
        Document {
            path,
            song,
            layouts: HashMap::new(),
        }
    }

    /// The file the song came from.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The song.
    pub fn song(&self) -> &Song {
        &self.song
    }

    /// The title to show: the song's title, or the file name.
    pub fn display_title(&self) -> String {
        if self.song.title().is_empty() {
            self.path.file_name().map_or_else(
                || "Untitled".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
        } else {
            self.song.title().to_owned()
        }
    }

    /// The layout of `track`, computed once. A panic inside layout is contained and shown
    /// as an error (the panic hook has already written a crash report).
    pub fn layout(
        &mut self,
        track: usize,
        options: &LayoutOptions,
        fonts: &Fonts,
    ) -> Result<Arc<Layout>, String> {
        let song = &self.song;
        self.layouts
            .entry(track)
            .or_insert_with(|| {
                let laid_out = catch_unwind(AssertUnwindSafe(|| {
                    layout_track(song, TrackIndex::new(track), options, fonts)
                }));
                match laid_out {
                    Ok(Ok(layout)) => Ok(Arc::new(layout)),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err(
                        "drawing this track failed unexpectedly; a crash report was written"
                            .to_owned(),
                    ),
                }
            })
            .clone()
    }
}
