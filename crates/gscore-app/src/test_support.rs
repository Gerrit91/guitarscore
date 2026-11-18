//! Helpers for the library's unit tests.
//!
//! The snapshot tests each build a wgpu renderer, and so a Vulkan device. Tests run in
//! parallel, and on the CI runner the driver is Mesa's software Vulkan (lavapipe): several
//! devices rendering and being dropped at once segfault the whole test process (the
//! integration tests have the same guard, `tests/common/mod.rs`; 2026-11-18 one of these
//! took the lib tests down). One device at a time, as the real app has: a snapshot test
//! holds this for its whole life, so its `Harness` (and its device) is dropped before the
//! next test builds one.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Keeps this test the only one using a GPU device until the returned guard is dropped.
pub(crate) fn lock_gpu() -> MutexGuard<'static, ()> {
    static GPU: Mutex<()> = Mutex::new(());
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}
