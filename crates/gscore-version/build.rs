//! Records the version at build time.
//!
//! A release build sets the `GSCORE_VERSION` variable to the release tag (the release
//! workflow does this); every other build is a development build, labelled with the crate
//! version and the short SHA of the commit it came from. The results are published as
//! `cargo:rustc-env` values, read by `env!` in `lib.rs`.

use std::process::Command;

fn main() {
    let sha = git_sha();
    let version = match std::env::var("GSCORE_VERSION") {
        Ok(tag) if !tag.trim().is_empty() => tag.trim().to_owned(),
        _ => format!("{}-dev+{sha}", env!("CARGO_PKG_VERSION")),
    };
    println!("cargo:rustc-env=GSCORE_VERSION_STRING={version}");
    println!("cargo:rustc-env=GSCORE_BUILD_SHA={sha}");
    println!("cargo:rerun-if-env-changed=GSCORE_VERSION");
}

/// The short SHA of the current commit, or `unknown` outside a git checkout (or when git
/// is not installed). A failure here is not an error: the build still gets a version.
fn git_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}
