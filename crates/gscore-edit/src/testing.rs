//! Test hooks (feature `test-support`): a command that panics on demand, to test that a
//! panic in a command is contained and rolled back (AGENTS.md "Panic-injection test").

use std::cell::Cell;

thread_local! {
    static PANIC_NEXT_COMMAND: Cell<bool> = const { Cell::new(false) };
}

/// Makes the next command applied on this thread panic before it changes anything.
pub fn panic_next_command() {
    PANIC_NEXT_COMMAND.with(|flag| flag.set(true));
}

/// Panics if [`panic_next_command`] asked for it (once).
pub(crate) fn maybe_panic() {
    if PANIC_NEXT_COMMAND.with(|flag| flag.replace(false)) {
        #[allow(clippy::panic, reason = "the injected panic is the point of this hook")]
        {
            panic!("injected panic (gscore_edit::testing::panic_next_command)");
        }
    }
}
