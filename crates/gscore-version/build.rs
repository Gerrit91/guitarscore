//! Records the version at build time.
//!
//! A release build sets the `GSCORE_VERSION` variable to the release tag (the release
//! workflow does this). A local build takes `git describe --tags`, so a build of a tagged
//! commit reports that tag. Without either (no git, no tags, a shallow clone) the build is
//! labelled with the crate version and the short SHA. `src/choose.rs` holds the order.
//! The results are published as `cargo:rustc-env` values, read by `env!` in `lib.rs`.

use std::process::Command;

#[path = "src/choose.rs"]
mod choose;

fn main() {
    let sha = git_sha();
    let release = std::env::var("GSCORE_VERSION").ok();
    let describe = git(&["describe", "--tags", "--match", "v[0-9]*"]);
    let version = choose::version(
        release.as_deref(),
        describe.as_deref(),
        env!("CARGO_PKG_VERSION"),
        &sha,
    );
    println!("cargo:rustc-env=GSCORE_VERSION_STRING={version}");
    println!("cargo:rustc-env=GSCORE_BUILD_SHA={sha}");
    println!("cargo:rerun-if-env-changed=GSCORE_VERSION");
    // A new commit, a branch switch or a new tag must bake in the new SHA and describe
    // output: watch git's HEAD, the branch it points at, the tags folder and the packed
    // refs (where `git gc` moves branch tips and tags).
    for path in git_paths_to_watch() {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// The files that change when HEAD moves; `git rev-parse --git-path` resolves them for
/// worktrees too. Empty outside a git checkout.
fn git_paths_to_watch() -> Vec<String> {
    let mut names = vec![
        "HEAD".to_owned(),
        "packed-refs".to_owned(),
        "refs/tags".to_owned(),
    ];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        names.push(branch);
    }
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    names
        .iter()
        .filter_map(|name| git(&["rev-parse", "--git-path", name]))
        .map(|path| std::path::Path::new(&dir).join(path).display().to_string())
        .collect()
}

/// The short SHA of the current commit, or `unknown` outside a git checkout (or when git
/// is not installed). A failure here is not an error: the build still gets a version.
fn git_sha() -> String {
    git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned())
}

/// `git args`' trimmed output, or `None` if git is missing, fails or prints nothing.
fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|text| !text.is_empty())
}
