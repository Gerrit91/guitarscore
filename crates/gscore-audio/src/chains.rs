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

use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, Chain, ChainSpec};

/// The tracks that can have a tone of their own: those with an index below this. A track
/// past it plays its kind's sound, dry.
pub const MAX_TONES: usize = 64;

/// A chain's knob values: its blocks' and the output level after them, in dB.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChainKnobs {
    pub settings: [BlockSettings; BLOCKS],
    /// After every block, before the mixer's fader.
    pub output_db: f32,
}

/// Whether `spec` does nothing (every place empty): then it need not be run at all.
pub fn chain_is_empty(spec: &ChainSpec) -> bool {
    spec.blocks.iter().all(Option::is_none)
}

/// The room a drum kit's parts share: one short room behind the whole kit, fed by each
/// part's Room send, so a kit sounds like one kit in one place (as a real room's
/// overheads glue it together).
pub fn drum_room_spec() -> ChainSpec {
    ChainSpec {
        blocks: [Some(BlockKind::Reverb), None, None, None, None],
    }
}

/// The knob values of [`drum_room_spec`]: small and short, all wet (it is a send: the
/// parts' Room knobs decide how much reaches it).
pub fn drum_room_knobs() -> ChainKnobs {
    let mut settings = BlockSettings::default();
    settings.reverb.size = 0.25;
    settings.reverb.decay = 0.6;
    settings.reverb.pre_delay = 5.0;
    settings.reverb.damping = 0.5;
    settings.reverb.modulation = 0.3;
    settings.reverb.width = 0.6;
    settings.reverb.mix = 1.0;
    settings.reverb.low_cut = 120.0;
    settings.reverb.high_cut = 9000.0;
    let mut blocks = [BlockSettings::default(); BLOCKS];
    if let Some(slot) = blocks.first_mut() {
        *slot = settings;
    }
    ChainKnobs {
        settings: blocks,
        output_db: 0.0,
    }
}

/// A drum kit's room, prepared for `sample_rate`.
pub fn prepare_drum_room(sample_rate: u32) -> Option<Box<Chain>> {
    prepare_chain(&drum_room_spec(), &drum_room_knobs(), sample_rate)
}

/// `spec` prepared at `sample_rate` with `knobs`, ready to run for one track, or `None`
/// when it does nothing (an empty chain at unity output).
pub fn prepare_chain(spec: &ChainSpec, knobs: &ChainKnobs, sample_rate: u32) -> Option<Box<Chain>> {
    let output = knobs.output_db.is_finite() && knobs.output_db.abs() > 1e-4;
    (!chain_is_empty(spec) || output).then(|| {
        Box::new(
            spec.prepare(sample_rate)
                .instance(&knobs.settings, knobs.output_db),
        )
    })
}

/// The chains of the tracks, by track index.
pub(crate) struct TrackTones {
    chains: Vec<Option<Box<Chain>>>,
    knobs: Vec<ChainKnobs>,
}

impl std::fmt::Debug for TrackTones {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackTones")
            .field("chains", &self.chains.iter().flatten().count())
            .finish()
    }
}

impl TrackTones {
    /// Tells every chain the tempo played now (a synced delay follows it).
    pub(crate) fn set_tempo(&mut self, bpm: f32) {
        for chain in self.chains.iter_mut().flatten() {
            chain.set_tempo(bpm);
        }
    }

    /// Room for [`MAX_TONES`] tracks. Allocates.
    pub(crate) fn new() -> Self {
        TrackTones {
            chains: (0..MAX_TONES).map(|_| None).collect(),
            knobs: vec![ChainKnobs::default(); MAX_TONES],
        }
    }

    /// The chain of `track`, if it has one.
    pub(crate) fn chain(&mut self, track: usize) -> Option<&mut Chain> {
        self.chains.get_mut(track).and_then(Option::as_deref_mut)
    }

    /// Gives `track` the chain `chain` (`None`: none) with `knobs` (the knob values of
    /// its own blocks: those kept for the chain it replaces may fit other blocks).
    /// Returns the chain it replaces, to be freed off the audio thread (or `chain` itself
    /// when `track` cannot have one).
    pub(crate) fn set_chain(
        &mut self,
        track: usize,
        chain: Option<Box<Chain>>,
        knobs: ChainKnobs,
    ) -> Option<Box<Chain>> {
        let mut chain = chain;
        let (Some(slot), Some(kept)) = (self.chains.get_mut(track), self.knobs.get_mut(track))
        else {
            return chain;
        };
        *kept = knobs;
        if let Some(chain) = &mut chain {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
        std::mem::replace(slot, chain)
    }

    /// New knob values for the chain of `track`, heard at once.
    pub(crate) fn set_settings(&mut self, track: usize, knobs: ChainKnobs) {
        if let Some(slot) = self.knobs.get_mut(track) {
            *slot = knobs;
        }
        if let Some(Some(chain)) = self.chains.get_mut(track) {
            chain.set_settings(&knobs.settings);
            chain.set_output_db(knobs.output_db);
        }
    }
}
