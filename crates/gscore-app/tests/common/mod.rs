//! Helpers shared by the app's integration tests.
//!
//! Each test file is its own binary and gets its own copy of this module, so the lock below
//! is per test binary — which is where the GPU lives.

use std::sync::{Mutex, MutexGuard};

/// The app's snapshot tests each build a wgpu renderer, and so a Vulkan device. Tests run in
/// parallel, and on the CI runner the driver is Mesa's software Vulkan (lavapipe): four
/// devices rendering and being dropped at once made it segfault the whole test process
/// (2026-11-14, `test_multitrack` after 12 of its 20 tests; 60 runs with the runner's own
/// lavapipe build here, four test threads on four cores, never crashed). One device at a
/// time, as the real app has: a snapshot test holds this for its whole life, so its
/// `Harness` (and its device) is dropped before the next test builds one.
///
/// A poisoned lock only means an earlier test failed; keep going, as `test_midi_out` does
/// for its ALSA port.
static GPU: Mutex<()> = Mutex::new(());

/// Keeps this test the only one using a GPU device until the returned guard is dropped.
pub(crate) fn lock_gpu() -> MutexGuard<'static, ()> {
    GPU.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
