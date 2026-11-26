//! A block whose sound is made outside this crate: a plugin (M24, design note
//! `docs/plugin-hosting.md`). The chain only knows these two small traits; what is behind
//! them (a plugin in this process, later one in a host process) is the caller's, so this
//! crate stays pure and builds for wasm.

use std::fmt;
use std::sync::Arc;

/// One running instance of an external block, in one chain.
pub trait ExternalBlock: Send {
    /// Runs `left` and `right` (equally long, stereo) through the block, in place. Called
    /// on the audio thread: must not allocate, wait on locks or panic.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Forgets the sound so far (a stop, a jump): no tail rings on.
    fn reset(&mut self);
}

/// What makes the instances of an external block: one per chain it is placed in.
pub trait ExternalSource: Send + Sync {
    /// Identifies what the block is (its plugin and settings): two sources with the same
    /// key are the same block, and a chain need not be made again for it.
    fn key(&self) -> &str;

    /// A new instance for a chain at `sample_rate`. Called off the audio thread; may
    /// allocate and wait. `None` if it cannot be made (the place then stays silent-free:
    /// the sound passes unchanged).
    fn instance(&self, sample_rate: u32) -> Option<Box<dyn ExternalBlock>>;
}

/// An external block in a [`crate::ChainSpec`].
#[derive(Clone)]
pub struct External(pub Arc<dyn ExternalSource>);

impl fmt::Debug for External {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("External").field(&self.0.key()).finish()
    }
}

impl PartialEq for External {
    fn eq(&self, other: &Self) -> bool {
        self.0.key() == other.0.key()
    }
}

/// A running instance inside a chain.
pub(crate) struct Instance(pub(crate) Box<dyn ExternalBlock>);

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Instance")
    }
}
