//! The version shown to users, computed at build time (see `build.rs`).

/// The full version: the release tag (`GSCORE_VERSION`), else `git describe --tags`
/// (`v0.1.1` on a tagged commit, `v0.1.1-3-g1a2b3c4` after it), else
/// `<version>-dev+<short-sha>` when no tag is reachable.
pub const VERSION: &str = env!("GSCORE_VERSION_STRING");

/// The short git SHA this binary was built from (`unknown` outside a git checkout).
pub const SHA: &str = env!("GSCORE_BUILD_SHA");

#[cfg(test)]
mod choose;

#[cfg(test)]
mod tests {
    use super::choose::version;

    #[test]
    fn the_release_tag_wins() {
        let v = version(
            Some("v0.2.0"),
            Some("v0.1.1-3-gabc1234"),
            "0.1.1",
            "abc1234",
        );
        assert_eq!(v, "v0.2.0");
    }

    #[test]
    fn a_local_build_uses_git_describe() {
        assert_eq!(version(None, Some("v0.1.1"), "0.1.1", "abc1234"), "v0.1.1");
        let after = version(None, Some("v0.1.1-3-gabc1234"), "0.1.1", "abc1234");
        assert_eq!(after, "v0.1.1-3-gabc1234");
    }

    #[test]
    fn without_tags_it_falls_back_to_the_crate_version() {
        assert_eq!(version(None, None, "0.1.1", "abc1234"), "0.1.1-dev+abc1234");
    }

    #[test]
    fn blank_values_count_as_missing() {
        let v = version(Some("  "), Some(""), "0.1.1", "unknown");
        assert_eq!(v, "0.1.1-dev+unknown");
    }

    #[test]
    fn values_are_trimmed() {
        assert_eq!(version(Some(" v0.1.1\n"), None, "0.1.1", "x"), "v0.1.1");
    }
}
