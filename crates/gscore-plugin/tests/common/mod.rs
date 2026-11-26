//! Shared by the plugin tests: where `cargo test` built the test plugin.

use std::path::PathBuf;

/// The test plugin's id.
pub(crate) const ID: &str = "org.guitarscore.test.gain";
/// Its gain parameter.
pub(crate) const GAIN: u32 = 1;

/// The test plugin's library: `cargo test` builds the examples into the target folder's
/// `examples`, a few folders above the test binary (how many depends on cargo's layout).
pub(crate) fn library_path() -> PathBuf {
    let name = format!(
        "{}test_plugin{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .skip(1)
        .take(6)
        .map(|dir| dir.join("examples").join(&name))
        .find(|path| path.exists())
        .unwrap_or_else(|| exe.with_file_name(&name))
}
