//! Text checks on manifests and sources that `cargo tree` cannot see.
//!
//! These are deliberately simple line-based heuristics (no Rust parser). They catch the
//! common cases; review still matters for anything clever.

use std::path::{Path, PathBuf};

/// Max directory depth when walking a crate's `src/` (guards against symlink loops).
const MAX_DEPTH: usize = 16;

/// std APIs that do not work on `wasm32-unknown-unknown` and must be injected via traits.
const FORBIDDEN_IN_CORE: &[&str] = &["std::fs", "std::thread", "Instant", "SystemTime"];

/// Every crate must opt into `[workspace.lints]` with `[lints] workspace = true`.
pub(crate) fn check_manifests(root: &Path, members: &[String]) -> Vec<String> {
    let mut errors = Vec::new();
    for member in members {
        let manifest = crate_dir(root, member).join("Cargo.toml");
        match std::fs::read_to_string(&manifest) {
            Ok(text) if has_workspace_lints(&text) => {}
            Ok(_) => errors.push(format!(
                "{} lacks `[lints]\\nworkspace = true`",
                manifest.display()
            )),
            Err(e) => errors.push(format!("cannot read {}: {e}", manifest.display())),
        }
    }
    errors
}

/// The core crates must not use std APIs that panic or are missing on wasm.
pub(crate) fn check_wasm_std_usage(root: &Path, crates: &[&str]) -> Vec<String> {
    let mut errors = Vec::new();
    for name in crates {
        let mut files = Vec::new();
        if let Err(e) = rust_files(&crate_dir(root, name).join("src"), 0, &mut files) {
            errors.push(e);
        }
        for file in files {
            let text = match std::fs::read_to_string(&file) {
                Ok(text) => text,
                Err(e) => {
                    errors.push(format!("cannot read {}: {e}", file.display()));
                    continue;
                }
            };
            for (line_no, item) in forbidden_uses(&text) {
                errors.push(format!(
                    "{}:{line_no}: `{item}` is not allowed in core crate `{name}` \
                     (pass the capability in through a trait; see AGENTS.md \"Portability\")",
                    file.display()
                ));
            }
        }
    }
    errors
}

/// Where a workspace member lives: `xtask/` or `crates/<name>/`.
fn crate_dir(root: &Path, name: &str) -> PathBuf {
    if name == "xtask" {
        root.join("xtask")
    } else {
        root.join("crates").join(name)
    }
}

/// True if the manifest has a `[lints]` table whose first entry is `workspace = true`.
fn has_workspace_lints(manifest: &str) -> bool {
    let mut lines = manifest
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    while let Some(line) = lines.next() {
        if line == "[lints]" {
            return lines
                .next()
                .is_some_and(|l| l.replace(' ', "") == "workspace=true");
        }
    }
    false
}

/// (1-based line number, forbidden item) for each non-comment line using a forbidden API.
fn forbidden_uses(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for item in FORBIDDEN_IN_CORE {
            if contains_word(code, item) {
                found.push((idx.saturating_add(1), *item));
            }
        }
    }
    found
}

/// `needle` occurs in `hay` not preceded or followed by an identifier character.
fn contains_word(hay: &str, needle: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    hay.match_indices(needle).any(|(start, _)| {
        let before = hay.get(..start).and_then(|s| s.chars().next_back());
        let after = hay
            .get(start.saturating_add(needle.len())..)
            .and_then(|s| s.chars().next());
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// Collects `.rs` files under `dir`, recursing at most [`MAX_DEPTH`] levels.
fn rust_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{} is nested too deeply", dir.display()));
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            rust_files(&path, depth.saturating_add(1), out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_workspace_lints() {
        assert!(has_workspace_lints(
            "[package]\n\n[lints]\n# c\nworkspace = true\n"
        ));
        assert!(!has_workspace_lints("[lints]\nworkspace = false\n"));
        assert!(!has_workspace_lints("[package]\nname = \"x\"\n"));
    }

    #[test]
    fn finds_forbidden_std_uses() {
        let src = "use std::fs;\n// std::thread in a comment\nlet t = Instant::now();\n\
                   let i = MyInstant::new();\nuse std::time::{Duration, SystemTime};\n";
        assert_eq!(
            forbidden_uses(src),
            [(1, "std::fs"), (3, "Instant"), (5, "SystemTime")]
        );
    }
}
