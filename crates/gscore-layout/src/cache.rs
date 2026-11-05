//! Layout across edits: [`LayoutCache`] keeps each measure's layout between calls and lays
//! out again only the measures that changed.
//!
//! Each track keeps its own measures. A measure's layout depends on the measure itself (keyed by the model's
//! [`Song::measure_stamp`], which changes whenever the measure may have changed), on
//! whether it shows its time signature, and on the track's settings (options, string count,
//! tuning). Line breaking and drawing still run over all measures; they are cheap compared
//! with laying out measures (see `gscore-cli bench`).

use std::collections::HashMap;
use std::sync::Arc;

use gscore_model::{Song, StringTuning, TrackIndex};

use crate::measure::MeasureLayout;
use crate::score::layout_with;
use crate::{Fonts, Layout, LayoutError, LayoutOptions};

/// What every measure's layout of one track depends on besides the measure.
#[derive(Debug, Clone, PartialEq)]
struct TrackKey {
    options: LayoutOptions,
    strings: usize,
    tuning: Option<Vec<StringTuning>>,
    capo: Option<u8>,
    /// Drum kits are written on a percussion staff.
    percussion: bool,
}

/// A measure's cache entry: its stamp, whether the time signature is shown, the key and
/// the key before.
type Entry = (u64, bool, i8, Option<i8>);

/// One track's measure layouts.
#[derive(Debug)]
struct TrackCache {
    key: TrackKey,
    measures: HashMap<Entry, Arc<MeasureLayout>>,
}

/// Measure layouts kept between calls, per track (a track whose settings changed starts
/// afresh). Use one cache per open song; the fonts must stay the same.
#[derive(Debug, Default)]
pub struct LayoutCache {
    tracks: HashMap<TrackIndex, TrackCache>,
    /// Measures the last call laid out afresh.
    laid_out: usize,
}

impl LayoutCache {
    /// The layout of `track` in `song`, the same as [`crate::layout_track`] gives, reusing
    /// the layouts of measures that did not change since the last call.
    pub fn layout(
        &mut self,
        song: &Song,
        track: TrackIndex,
        options: &LayoutOptions,
        fonts: &Fonts,
    ) -> Result<Layout, LayoutError> {
        self.layout_tracks(song, &[track], track, options, fonts)
    }

    /// The layout of `tracks` together, the same as [`crate::layout_tracks`] gives,
    /// reusing the layouts of measures that did not change since the last call. Tracks
    /// not shown keep their measures for when they are shown again.
    pub fn layout_tracks(
        &mut self,
        song: &Song,
        tracks: &[TrackIndex],
        primary: TrackIndex,
        options: &LayoutOptions,
        fonts: &Fonts,
    ) -> Result<Layout, LayoutError> {
        // A measure's layout does not depend on the pages: switching between pages and the
        // endless page, or changing the page setup, keeps the cache.
        let measure_options = options.for_measures();
        let mut old: HashMap<TrackIndex, HashMap<Entry, Arc<MeasureLayout>>> = HashMap::new();
        let mut kept: HashMap<TrackIndex, (TrackKey, HashMap<Entry, Arc<MeasureLayout>>)> =
            HashMap::new();
        for &track in tracks {
            let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
            let key = TrackKey {
                options: measure_options.clone(),
                strings: view.string_count(),
                tuning: view.tuning().ok().flatten(),
                capo: view.capo().ok(),
                percussion: view.is_percussion(),
            };
            if let Some(cache) = self.tracks.remove(&track)
                && cache.key == key
            {
                old.insert(track, cache.measures);
            }
            kept.insert(track, (key, HashMap::new()));
        }
        let mut laid_out = 0_usize;
        let layout = layout_with(
            song,
            tracks,
            primary,
            options,
            fonts,
            &mut |track, measure, show, key, previous_key, ctx| {
                let entry = (song.measure_stamp(measure.index()), show, key, previous_key);
                let cached = old.get(&track).and_then(|measures| measures.get(&entry));
                let layout = match cached {
                    Some(cached) if cached.index() == measure.index() => Arc::clone(cached),
                    Some(cached) => Arc::new(cached.moved_to(measure.index())),
                    None => {
                        laid_out = laid_out.saturating_add(1);
                        Arc::new(MeasureLayout::new(measure, show, key, previous_key, ctx)?)
                    }
                };
                if let Some((_, measures)) = kept.get_mut(&track) {
                    measures.insert(entry, Arc::clone(&layout));
                }
                Ok(layout)
            },
        );
        // Keep only what this layout used: deleted and edited measures go.
        for (track, (key, measures)) in kept {
            self.tracks.insert(track, TrackCache { key, measures });
        }
        self.laid_out = laid_out;
        layout
    }

    /// How many measures the last call laid out afresh (the others came from the cache).
    pub fn laid_out(&self) -> usize {
        self.laid_out
    }
}
