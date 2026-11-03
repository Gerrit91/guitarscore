//! Layout across edits: [`LayoutCache`] keeps each measure's layout between calls and lays
//! out again only the measures that changed.
//!
//! A measure's layout depends on the measure itself (keyed by the model's
//! [`Song::measure_stamp`], which changes whenever the measure may have changed), on
//! whether it shows its time signature, and on the track's settings (options, string count,
//! tuning). Line breaking and drawing still run over all measures; they are cheap compared
//! with laying out measures (see `gscore-cli bench`).

use std::collections::HashMap;
use std::sync::Arc;

use gscore_model::{Song, StringTuning, TrackIndex};

use crate::measure::MeasureLayout;
use crate::{Fonts, Layout, LayoutError, LayoutOptions, layout_with};

/// What every measure's layout of one track depends on besides the measure.
#[derive(Debug, Clone, PartialEq)]
struct TrackKey {
    track: TrackIndex,
    options: LayoutOptions,
    strings: usize,
    tuning: Option<Vec<StringTuning>>,
    capo: Option<u8>,
    /// Drum kits are written on a percussion staff.
    percussion: bool,
}

/// Measure layouts kept between calls, for one track at a time (a different track or
/// options start afresh). Use one cache per open song and track; the fonts must stay the
/// same.
#[derive(Debug, Default)]
pub struct LayoutCache {
    key: Option<TrackKey>,
    /// By measure stamp and whether the time signature is shown.
    measures: HashMap<(u64, bool), Arc<MeasureLayout>>,
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
        let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
        let key = TrackKey {
            track,
            // A measure's layout does not depend on the pages: switching between pages and
            // the endless page, or changing the page setup, keeps the cache.
            options: options.for_measures(),
            strings: view.string_count(),
            tuning: view.tuning().ok().flatten(),
            capo: view.capo().ok(),
            percussion: view.is_percussion(),
        };
        if self.key.as_ref() != Some(&key) {
            self.measures.clear();
            self.key = Some(key);
        }
        let mut kept = HashMap::with_capacity(self.measures.len());
        let mut laid_out = 0_usize;
        let old = &self.measures;
        let layout = layout_with(song, track, options, fonts, &mut |measure, show, ctx| {
            let key = (song.measure_stamp(measure.index()), show);
            let layout = match old.get(&key) {
                Some(cached) if cached.index() == measure.index() => Arc::clone(cached),
                Some(cached) => Arc::new(cached.moved_to(measure.index())),
                None => {
                    laid_out = laid_out.saturating_add(1);
                    Arc::new(MeasureLayout::new(measure, show, ctx)?)
                }
            };
            kept.insert(key, Arc::clone(&layout));
            Ok(layout)
        });
        // Keep only what this layout used: deleted and edited measures go.
        self.measures = kept;
        self.laid_out = laid_out;
        layout
    }

    /// How many measures the last call laid out afresh (the others came from the cache).
    pub fn laid_out(&self) -> usize {
        self.laid_out
    }
}
