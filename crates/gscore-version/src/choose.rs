//! Picks the version string from what the build can see. Shared by `build.rs` (which
//! includes this file with `#[path]`) and the unit tests in `lib.rs`.

/// The version to bake in, in order of preference:
///
/// 1. `release`: the `GSCORE_VERSION` variable the release workflow sets to the tag;
/// 2. `describe`: `git describe --tags` of the checkout, which is the tag itself on a
///    tagged commit (`v0.1.1`) and `<tag>-<commits since>-g<sha>` after it;
/// 3. `<crate version>-dev+<sha>` when there is neither (no git, no tags, or a shallow
///    clone without them).
///
/// Blank values count as missing.
pub(crate) fn version(
    release: Option<&str>,
    describe: Option<&str>,
    crate_version: &str,
    sha: &str,
) -> String {
    fn present(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|text| !text.is_empty())
    }
    present(release)
        .or_else(|| present(describe))
        .map_or_else(|| format!("{crate_version}-dev+{sha}"), str::to_owned)
}
