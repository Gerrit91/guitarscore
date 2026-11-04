//! The version shown to users, computed at build time (see `build.rs`).

/// The full version: the release tag (`GSCORE_VERSION`), or `<version>-dev+<short-sha>`
/// for a development build.
pub const VERSION: &str = env!("GSCORE_VERSION_STRING");

/// The short git SHA this binary was built from (`unknown` outside a git checkout).
pub const SHA: &str = env!("GSCORE_BUILD_SHA");
