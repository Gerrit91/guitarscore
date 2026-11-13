//! The update check: asks GitHub for the latest release and compares its version with
//! this build's. The app only tells the user and links to the release page; it never
//! downloads or replaces itself (owner decision, 2026-11-13).
//!
//! What GitHub answers is untrusted: the version is parsed, the link must point at this
//! project's releases, and the notes are cut to a size the window can show.

use std::time::Duration;

/// GitHub's latest full release (drafts and pre-releases are left out by GitHub).
const LATEST_URL: &str = "https://api.github.com/repos/Gerrit91/guitarscore/releases/latest";
/// The only place a release link may point to.
const RELEASES_PAGE: &str = "https://github.com/Gerrit91/guitarscore/releases";
/// The most of GitHub's answer that is read (a release is a few kilobytes).
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
/// The most of the release notes that is shown.
const MAX_NOTES_CHARS: usize = 4000;
/// How long the whole request may take.
const TIMEOUT: Duration = Duration::from_secs(20);
/// Seconds between two automatic checks.
pub(crate) const CHECK_EVERY_SECONDS: u64 = 24 * 60 * 60;

/// A release that is newer than this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Release {
    /// Its tag, as GitHub names it (`v0.3.0`).
    pub(crate) tag: String,
    /// Its title, or the tag if it has none.
    pub(crate) title: String,
    /// Its page on GitHub, where the downloads are.
    pub(crate) url: String,
    /// The release notes (Markdown as written), cut to [`MAX_NOTES_CHARS`].
    pub(crate) notes: String,
}

/// A version as `major.minor.patch`, with an optional pre-release (`rc.1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Version {
    numbers: [u64; 3],
    /// The dot-separated pre-release parts; empty for a release.
    pre: Vec<String>,
}

impl Version {
    /// Reads a tag (`v0.2.0`, `0.2.0-rc.1`) or this app's own version string, which can
    /// also be a build after a tag (`v0.2.0-3-gabc1234`, counted as that tag) or a build
    /// without one (`0.2.0-dev+abc1234`, counted as that version). Build metadata after
    /// `+` is ignored. `None` if it is not a version.
    pub(crate) fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split('+').next().unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = [0_u64; 3];
        let mut parts = core.split('.');
        for number in &mut numbers {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *number = part.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            // A local build: the version it was made from, not a pre-release of it.
            Some(pre) if pre == "dev" || is_describe_suffix(pre) => Vec::new(),
            Some(pre) => {
                let parts: Vec<String> = pre.split('.').map(str::to_owned).collect();
                let valid = |part: &String| {
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                };
                if !parts.iter().all(valid) {
                    return None;
                }
                parts
            }
        };
        Some(Version { numbers, pre })
    }

    /// Whether it is a pre-release (`rc.1`).
    #[cfg(test)]
    fn is_pre_release(&self) -> bool {
        !self.pre.is_empty()
    }
}

/// `3-gabc1234`: what `git describe` adds after a tag for later commits.
fn is_describe_suffix(text: &str) -> bool {
    let Some((count, sha)) = text.split_once("-g") else {
        return false;
    };
    !count.is_empty()
        && count.bytes().all(|b| b.is_ascii_digit())
        && !sha.is_empty()
        && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Semantic versioning's order: the numbers, then a release after its pre-releases,
    /// then pre-release parts one by one (numbers by value and before words).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        self.numbers.cmp(&other.numbers).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(a), Ok(b)) => a.cmp(&b),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if order != Ordering::Equal {
                            return order;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            }
        })
    }
}

/// The fields of GitHub's release that are used.
#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Reads GitHub's answer: the release if it is newer than `current`, `None` if this build
/// is up to date. An answer that cannot be trusted is an error.
pub(crate) fn newer_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let release: GitHubRelease = serde_json::from_str(json)
        .map_err(|e| format!("GitHub's answer could not be read ({e})"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let latest = Version::parse(&release.tag_name).ok_or_else(|| {
        format!(
            "the latest release has no version number ({})",
            cut(&release.tag_name, 40)
        )
    })?;
    let current = Version::parse(current)
        .ok_or_else(|| format!("this build's version {current} is unknown"))?;
    if latest <= current {
        return Ok(None);
    }
    // Only this project's own release pages are ever offered as a link.
    let page = format!("{RELEASES_PAGE}/");
    if !release.html_url.starts_with(&page) || release.html_url.contains(char::is_whitespace) {
        return Err("the latest release links somewhere unexpected".to_owned());
    }
    let title = release
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| release.tag_name.clone());
    Ok(Some(Release {
        tag: cut(&release.tag_name, 40),
        title: cut(&title, 80),
        url: release.html_url,
        notes: cut(
            release.body.as_deref().unwrap_or_default().trim(),
            MAX_NOTES_CHARS,
        ),
    }))
}

/// At most `max` characters of `text`, with an ellipsis if it was longer.
fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", text.get(..end).unwrap_or_default()),
        None => text.to_owned(),
    }
}

/// Whether an automatic check is due: never checked, a day since the last one, or a clock
/// that went backwards. Times are seconds since 1970.
pub(crate) fn check_due(last_checked: Option<u64>, now: u64) -> bool {
    match last_checked {
        None => true,
        Some(last) if last > now => true,
        Some(last) => now.saturating_sub(last) >= CHECK_EVERY_SECONDS,
    }
}

/// Seconds since 1970 now (0 if the clock is before it).
pub(crate) fn now_seconds() -> u64 {
    crate::SystemTime::now()
        .duration_since(crate::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Asks GitHub for the latest release (blocking: call it on a thread of its own). Only
/// the request itself is sent: no data about the user or their songs. The user agent
/// names the app and its version, as GitHub asks of API clients.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn fetch_latest() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(format!("GuitarScore/{}", gscore_version::VERSION))
        .build()
        .into();
    let mut response = agent
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "no release has been published yet".to_owned(),
            ureq::Error::StatusCode(403 | 429) => {
                "GitHub refused the request for now; try again later".to_owned()
            }
            other => format!("GitHub could not be reached ({other})"),
        })?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| format!("GitHub's answer could not be read ({e})"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn tags_and_build_versions_are_read() {
        assert_eq!(v("v0.2.0"), v("0.2.0"));
        assert!(v("v0.2.0-rc.1").is_pre_release());
        // A build after a tag counts as that tag, a build without one as its version.
        assert_eq!(v("v0.2.0-3-gabc1234"), v("v0.2.0"));
        assert_eq!(v("0.2.0-dev+abc1234"), v("0.2.0"));
        assert_eq!(v("0.2.0-dev+unknown"), v("0.2.0"));
        assert_eq!(v(" v1.10.3\n"), v("1.10.3"));
    }

    #[test]
    fn what_is_not_a_version_is_refused() {
        for text in [
            "",
            "v",
            "latest",
            "1.2",
            "1.2.3.4",
            "1..3",
            "-1.2.3",
            "1.2.x",
            "1.2.3-",
            "1.2.3-a..b",
            "1.2.3-ä",
            "99999999999999999999.0.0",
            "v 1.2.3",
            "１.２.３",
        ] {
            assert_eq!(Version::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn versions_are_ordered_as_semantic_versioning_orders_them() {
        let ordered = [
            "0.1.0",
            "0.1.1",
            "0.2.0-alpha",
            "0.2.0-alpha.1",
            "0.2.0-alpha.beta",
            "0.2.0-beta.2",
            "0.2.0-beta.11",
            "0.2.0-rc.1",
            "0.2.0",
            "0.10.0",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    fn answer(tag: &str, url: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "name": "GuitarScore 0.3.0",
            "html_url": url,
            "body": "## Changes\n- Faster.",
            "draft": false,
            "prerelease": false,
            "assets": [],
        })
        .to_string()
    }

    const PAGE: &str = "https://github.com/Gerrit91/guitarscore/releases/tag/v0.3.0";

    #[test]
    fn a_newer_release_is_offered() {
        let release = newer_release(&answer("v0.3.0", PAGE), "v0.2.0")
            .unwrap()
            .unwrap();
        assert_eq!(release.tag, "v0.3.0");
        assert_eq!(release.title, "GuitarScore 0.3.0");
        assert_eq!(release.url, PAGE);
        assert_eq!(release.notes, "## Changes\n- Faster.");
        // A release candidate hears about the full release.
        assert!(
            newer_release(&answer("v0.3.0", PAGE), "v0.3.0-rc.2")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_not_offered() {
        for current in [
            "v0.3.0",
            "v0.3.0-4-g1234abc",
            "0.3.0-dev+abc",
            "v0.4.0-rc.1",
        ] {
            assert_eq!(
                newer_release(&answer("v0.3.0", PAGE), current),
                Ok(None),
                "{current}"
            );
        }
    }

    #[test]
    fn drafts_and_pre_releases_are_not_offered() {
        let mut json: serde_json::Value = serde_json::from_str(&answer("v0.3.0", PAGE)).unwrap();
        json["prerelease"] = true.into();
        assert_eq!(newer_release(&json.to_string(), "v0.2.0"), Ok(None));
        json["prerelease"] = false.into();
        json["draft"] = true.into();
        assert_eq!(newer_release(&json.to_string(), "v0.2.0"), Ok(None));
    }

    #[test]
    fn a_link_to_anywhere_else_is_refused() {
        for url in [
            "https://evil.example/releases/tag/v0.3.0",
            "https://github.com/Gerrit91/guitarscore-fake/releases/tag/v0.3.0",
            "http://github.com/Gerrit91/guitarscore/releases/tag/v0.3.0",
            "https://github.com/Gerrit91/guitarscore/releases/tag/v0.3.0 javascript:x",
            "",
        ] {
            assert!(
                newer_release(&answer("v0.3.0", url), "v0.2.0").is_err(),
                "{url}"
            );
        }
    }

    #[test]
    fn broken_answers_are_errors_not_panics() {
        for json in [
            "",
            "null",
            "[]",
            "{}",
            "{\"tag_name\": 3}",
            "{\"tag_name\": \"v0.3.0\"}",
            "{\"message\": \"Not Found\"}",
            "\u{0}\u{1}garbage",
        ] {
            assert!(newer_release(json, "v0.2.0").is_err(), "{json:?}");
        }
        assert!(newer_release(&answer("nightly", PAGE), "v0.2.0").is_err());
        assert!(newer_release(&answer("v0.3.0", PAGE), "weird").is_err());
    }

    #[test]
    fn long_notes_and_titles_are_cut_at_a_character() {
        let mut json: serde_json::Value = serde_json::from_str(&answer("v0.3.0", PAGE)).unwrap();
        json["body"] = "ü".repeat(10_000).into();
        json["name"] = "  ".into();
        let release = newer_release(&json.to_string(), "v0.2.0").unwrap().unwrap();
        assert_eq!(release.notes.chars().count(), MAX_NOTES_CHARS + 1);
        assert!(release.notes.ends_with('…'));
        assert_eq!(
            release.title, "v0.3.0",
            "a blank title falls back to the tag"
        );
    }

    /// The real request: `cargo test -p gscore-app --lib -- --ignored github`.
    #[test]
    #[ignore = "asks GitHub over the network"]
    fn github_answers_with_the_latest_release() {
        let json = fetch_latest().unwrap();
        let release = newer_release(&json, "0.0.1").unwrap().unwrap();
        assert!(Version::parse(&release.tag).is_some());
        assert!(release.url.starts_with(RELEASES_PAGE));
        assert_eq!(newer_release(&json, &release.tag), Ok(None));
    }

    #[test]
    fn a_check_is_due_once_a_day() {
        let day = CHECK_EVERY_SECONDS;
        assert!(check_due(None, 1000));
        assert!(!check_due(Some(1000), 1000 + day - 1));
        assert!(check_due(Some(1000), 1000 + day));
        assert!(check_due(Some(5000), 1000), "a clock that went back");
    }
}
