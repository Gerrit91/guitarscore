//! Panic injection for tests (AGENTS.md "Panic-injection test"): a layout or a drawing of
//! the page that panics on demand, to test that the app contains it. [`checkpoint`] is
//! always there and does nothing unless the `test-support` feature is on; only then can a
//! test arm it with [`panic_next`].

/// Where a panic can be injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Laying out a track ([`crate::LayoutCache::layout_tracks`]).
    Layout,
    /// Drawing the page (the app calls the checkpoint where it draws).
    Draw,
}

#[cfg(feature = "test-support")]
thread_local! {
    static ARMED: std::cell::Cell<Option<Stage>> = const { std::cell::Cell::new(None) };
}

/// Makes the next pass through `at` on this thread panic (once).
#[cfg(feature = "test-support")]
pub fn panic_next(at: Stage) {
    ARMED.with(|armed| armed.set(Some(at)));
}

/// Panics here if a test armed `at` ([`panic_next`]); otherwise, and always without the
/// `test-support` feature, does nothing.
#[inline]
pub fn checkpoint(at: Stage) {
    #[cfg(feature = "test-support")]
    if ARMED.with(|armed| armed.get()) == Some(at) {
        ARMED.with(|armed| armed.set(None));
        #[allow(clippy::panic, reason = "the injected panic is the point of this hook")]
        {
            panic!("injected panic (gscore_layout::testing, {at:?})");
        }
    }
    #[cfg(not(feature = "test-support"))]
    let _ = at;
}
