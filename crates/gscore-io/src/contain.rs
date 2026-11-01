//! Running the Guitar Pro reader/writer of `gscore-gp` with panics contained.

use std::fmt::Debug;
use std::panic;

use crate::error::IoError;
use crate::format::FileFormat;

/// Runs `work` inside `catch_unwind`. A `gscore-gp` error or a panic becomes
/// [`IoError::GuitarProError`], prefixed with the format and what was being done.
pub(crate) fn contain<T, E: Debug>(
    format: FileFormat,
    doing: &str,
    work: impl FnOnce() -> Result<T, E> + panic::UnwindSafe,
) -> Result<T, IoError> {
    match panic::catch_unwind(work) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(IoError::GuitarProError(format!("{format}: {e:?}"))),
        Err(payload) => Err(IoError::GuitarProError(format!(
            "{format}: the {doing} panicked: {}",
            panic_message(payload.as_ref())
        ))),
    }
}

/// Best-effort text of a panic payload (`panic!` passes a `&str` or a `String`).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload")
}
