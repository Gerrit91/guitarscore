//! The effect chains of the modelled strings, one per kind of string: the blocks after
//! the strings (cabinet or body responses, rooms, reverb), with one running instance per
//! track of that kind. Made off the audio thread ([`ChainSet::new`]), swapped into the
//! engine whole, and handed back to be freed elsewhere.

use gscore_guitar::{BLOCKS, BlockSettings, Chain, ChainSpec};
use gscore_play::{Sequence, StringKind, TrackEngine};

/// The most chain instances one kind gets (as many tracks as the modelled strings play).
pub(crate) const MAX_INSTANCES: usize = crate::guitars::SLOTS;

/// The chain of one kind of string, ready to run: an instance per track.
#[derive(Debug)]
pub struct ChainSet {
    pub(crate) instances: Vec<Chain>,
}

impl ChainSet {
    /// `spec` prepared at `sample_rate` with `settings`, for `tracks` tracks (at least one,
    /// at most the modelled strings' track count). Allocates: call it off the audio thread.
    pub fn new(
        spec: &ChainSpec,
        settings: &[BlockSettings; BLOCKS],
        sample_rate: u32,
        tracks: usize,
    ) -> Self {
        let prepared = spec.prepare(sample_rate);
        ChainSet {
            instances: (0..tracks.clamp(1, MAX_INSTANCES))
                .map(|_| prepared.instance(settings))
                .collect(),
        }
    }

    /// Whether it does nothing (every place empty): then it need not be run at all.
    pub fn is_empty(spec: &ChainSpec) -> bool {
        spec.blocks.iter().all(Option::is_none)
    }
}

/// How many tracks of `sequence` the modelled strings play as `kind`.
pub fn tracks_of_kind(sequence: &Sequence, kind: StringKind) -> usize {
    sequence
        .performance
        .tracks
        .iter()
        .filter(|track| track.engine == TrackEngine::Modelled(kind))
        .count()
}
