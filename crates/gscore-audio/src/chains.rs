//! Each track's effect chain inside the engine (the blocks after its sound: amps, drives,
//! cabinet or body responses, rooms, reverbs), for every track the mixer plays, the
//! SoundFont's as well as the modelled strings'. (A modelled track's own string model is
//! kept with the strings, `guitars`.) A chain is prepared off the audio
//! thread, swapped in whole, and the one it replaces handed back to be freed elsewhere.
//!
//! Tracks are known by their index in the song (`PerformanceTrack::track`, which an
//! excerpt of one track keeps), so a beat preview plays with its track's tone;
//! [`MAX_TONES`] tracks can have a tone of their own (the room for them is made with the
//! engine, so nothing allocates here).

use gscore_guitar::{BLOCKS, BlockSettings, Chain, ChainSpec};

/// The tracks that can have a tone of their own: those with an index below this. A track
/// past it plays its kind's sound, dry.
pub const MAX_TONES: usize = 64;

/// Whether `spec` does nothing (every place empty): then it need not be run at all.
pub fn chain_is_empty(spec: &ChainSpec) -> bool {
    spec.blocks.iter().all(Option::is_none)
}

/// `spec` prepared at `sample_rate` with `settings`, ready to run for one track, or
/// `None` when it does nothing. Allocates: call it off the audio thread.
pub fn prepare_chain(
    spec: &ChainSpec,
    settings: &[BlockSettings; BLOCKS],
    sample_rate: u32,
) -> Option<Box<Chain>> {
    (!chain_is_empty(spec)).then(|| Box::new(spec.prepare(sample_rate).instance(settings)))
}

/// The chains of the tracks, by track index.
pub(crate) struct TrackTones {
    chains: Vec<Option<Box<Chain>>>,
    settings: Vec<[BlockSettings; BLOCKS]>,
}

impl std::fmt::Debug for TrackTones {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackTones")
            .field("chains", &self.chains.iter().flatten().count())
            .finish()
    }
}

impl TrackTones {
    /// Room for [`MAX_TONES`] tracks. Allocates.
    pub(crate) fn new() -> Self {
        TrackTones {
            chains: (0..MAX_TONES).map(|_| None).collect(),
            settings: vec![[BlockSettings::default(); BLOCKS]; MAX_TONES],
        }
    }

    /// The chain of `track`, if it has one.
    pub(crate) fn chain(&mut self, track: usize) -> Option<&mut Chain> {
        self.chains.get_mut(track).and_then(Option::as_deref_mut)
    }

    /// Gives `track` the chain `chain` (`None`: none) with `settings` (the knob values of
    /// its own blocks: those kept for the chain it replaces may fit other blocks).
    /// Returns the chain it replaces, to be freed off the audio thread (or `chain` itself
    /// when `track` cannot have one).
    pub(crate) fn set_chain(
        &mut self,
        track: usize,
        chain: Option<Box<Chain>>,
        settings: [BlockSettings; BLOCKS],
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        let (Some(slot), Some(kept)) = (self.chains.get_mut(track), self.settings.get_mut(track))
        else {
            return chain;
        };
        *kept = settings;
        if let Some(chain) = &mut chain {
            chain.set_settings(&settings);
        }
        std::mem::replace(slot, chain)
    }

    /// New knob values for the chain of `track`, heard at once.
    pub(crate) fn set_settings(&mut self, track: usize, settings: [BlockSettings; BLOCKS]) {
        if let Some(slot) = self.settings.get_mut(track) {
            *slot = settings;
        }
        if let Some(Some(chain)) = self.chains.get_mut(track) {
            chain.set_settings(&settings);
        }
    }
}
