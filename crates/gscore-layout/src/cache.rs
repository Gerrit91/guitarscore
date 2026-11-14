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

use gscore_model::{Song, StaffIndex, StringTuning, TrackIndex};

use crate::measure::MeasureLayout;
use crate::score::layout_with;
use crate::{Fonts, Layout, LayoutError, LayoutOptions};

/// What every measure's layout of one staff depends on besides the measure.
#[derive(Debug, Clone, PartialEq)]
struct StaffKey {
    options: LayoutOptions,
    strings: usize,
    tuning: Option<Vec<StringTuning>>,
    capo: Option<u8>,
    /// Drum kits are written on a percussion staff.
    percussion: bool,
    /// A track with several staves shows them in notation only.
    keyboard: bool,
}

/// A measure's cache entry: its stamp, whether the time signature is shown, the key and
/// the key before, the clef and the clef before, and its width scale in thousandths.
type Entry = (
    u64,
    bool,
    bool,
    i8,
    Option<i8>,
    crate::notation::Clef,
    Option<crate::notation::Clef>,
    i64,
);

/// One measure lined up across tracks: from these layouts, this result.
type Aligned = (Vec<Arc<MeasureLayout>>, Vec<Arc<MeasureLayout>>);

/// One track's measure layouts.
#[derive(Debug)]
struct TrackCache {
    key: StaffKey,
    measures: HashMap<Entry, Arc<MeasureLayout>>,
}

/// Measure layouts kept between calls, per staff (a staff whose settings changed starts
/// afresh). Use one cache per open song; the fonts must stay the same.
#[derive(Debug, Default)]
pub struct LayoutCache {
    staves: HashMap<(TrackIndex, StaffIndex), TrackCache>,
    /// Measures lined up across tracks, by measure index: the tracks' layouts they were
    /// made from (kept alive, so an unchanged measure is recognised by identity) and the
    /// result.
    aligned: HashMap<usize, Aligned>,
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
        type Key = (TrackIndex, StaffIndex);
        let mut old: HashMap<Key, HashMap<Entry, Arc<MeasureLayout>>> = HashMap::new();
        let mut kept: HashMap<Key, (StaffKey, HashMap<Entry, Arc<MeasureLayout>>)> = HashMap::new();
        for &track in tracks {
            let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
            // A track with several staves shows them in notation only.
            let keyboard = view.staff_count() > 1;
            for staff in view.all_staves() {
                let staff_view = staff.view();
                let key = StaffKey {
                    options: measure_options.clone(),
                    strings: staff_view.string_count(),
                    tuning: staff_view.tuning().ok().flatten(),
                    capo: staff_view.capo().ok(),
                    percussion: staff_view.is_percussion(),
                    keyboard,
                };
                let at = (track, staff.index());
                if let Some(cache) = self.staves.remove(&at)
                    && cache.key == key
                {
                    old.insert(at, cache.measures);
                }
                kept.insert(at, (key, HashMap::new()));
            }
        }
        let mut laid_out = 0_usize;
        let old_aligned = std::mem::take(&mut self.aligned);
        let mut aligned: HashMap<usize, Aligned> = HashMap::new();
        let spacing = options.spacing;
        let layout = layout_with(
            song,
            tracks,
            primary,
            options,
            fonts,
            &mut |track,
                  staff,
                  measure,
                  show,
                  free_time,
                  key,
                  previous_key,
                  clef,
                  previous_clef,
                  ctx| {
                let scale = song.measure_scale(track, staff, measure.index());
                let entry = (
                    song.measure_stamp(measure.index()),
                    show,
                    free_time,
                    key,
                    previous_key,
                    clef,
                    previous_clef,
                    // Thousandths: the key must be an integer.
                    (scale * 1000.0).round() as i64,
                );
                let at = (track, staff);
                let cached = old.get(&at).and_then(|measures| measures.get(&entry));
                let layout = match cached {
                    Some(cached) if cached.index() == measure.index() => Arc::clone(cached),
                    Some(cached) => Arc::new(cached.moved_to(measure.index())),
                    None => {
                        laid_out = laid_out.saturating_add(1);
                        Arc::new(MeasureLayout::new(
                            measure,
                            show,
                            free_time,
                            key,
                            previous_key,
                            clef,
                            previous_clef,
                            scale,
                            ctx,
                        )?)
                    }
                };
                if let Some((_, measures)) = kept.get_mut(&at) {
                    measures.insert(entry, Arc::clone(&layout));
                }
                Ok(layout)
            },
            &mut |index, column| {
                // The same layouts as last time (the same tracks, nothing edited): the
                // same alignment.
                let result = match old_aligned.get(&index) {
                    Some((from, result))
                        if from.len() == column.len()
                            && from.iter().zip(column).all(|(a, b)| Arc::ptr_eq(a, b)) =>
                    {
                        result.clone()
                    }
                    _ => crate::measure::align(column, &spacing),
                };
                aligned.insert(index, (column.to_vec(), result.clone()));
                result
            },
        );
        self.aligned = aligned;
        // Keep only what this layout used: deleted and edited measures go.
        for (at, (key, measures)) in kept {
            self.staves.insert(at, TrackCache { key, measures });
        }
        self.laid_out = laid_out;
        layout
    }

    /// How many measures the last call laid out afresh (the others came from the cache).
    pub fn laid_out(&self) -> usize {
        self.laid_out
    }
}
