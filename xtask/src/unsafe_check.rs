//! The workspace's one exception to `unsafe_code = "forbid"` (owner's approval,
//! 2026-11-26; `docs/plugin-hosting.md`): `gscore-plugin` loads native plugin libraries.
//!
//! It cannot take `[workspace.lints]` as they are (a `forbid` cannot be allowed locally),
//! so it copies them with `unsafe_code = "deny"`. This module checks that the copy matches
//! the workspace's table in every other entry, and that `unsafe_code` is allowed only in the
//! files the design note names, in no other file of any crate.

use std::path::Path;

/// The crate with the exception.
pub(crate) const UNSAFE_CRATE: &str = "gscore-plugin";
/// The only files (relative to the crate) that may allow `unsafe_code`.
const UNSAFE_FILES: &[&str] = &["src/library.rs", "examples/test_plugin.rs"];

/// The exception crate's lints match the workspace's, `unsafe_code` = "deny" aside.
pub(crate) fn check_lints(root: &Path) -> Vec<String> {
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
    };
    let workspace_path = root.join("Cargo.toml");
    let crate_path = root.join("crates").join(UNSAFE_CRATE).join("Cargo.toml");
    let (workspace, manifest) = match (read(&workspace_path), read(&crate_path)) {
        (Ok(w), Ok(c)) => (w, c),
        (Err(e), _) | (_, Err(e)) => return vec![e],
    };
    let mut errors = Vec::new();
    for (ours, theirs) in [
        ("workspace.lints.rust", "lints.rust"),
        ("workspace.lints.clippy", "lints.clippy"),
    ] {
        let wanted = entries(&workspace, ours);
        let have = entries(&manifest, theirs);
        if wanted.is_empty() {
            errors.push(format!("{}: no [{ours}] table", workspace_path.display()));
        }
        for (key, value) in &wanted {
            let expected = if key == "unsafe_code" {
                "\"deny\""
            } else {
                value.as_str()
            };
            match have.iter().find(|(k, _)| k == key) {
                Some((_, v)) if v == expected => {}
                Some((_, v)) => errors.push(format!(
                    "{}: [{theirs}] {key} = {v}, but must be {expected} (it copies the \
                     workspace's lints; see docs/plugin-hosting.md)",
                    crate_path.display()
                )),
                None => errors.push(format!(
                    "{}: [{theirs}] lacks {key} = {expected} (it copies the workspace's \
                     lints; see docs/plugin-hosting.md)",
                    crate_path.display()
                )),
            }
        }
    }
    errors
}

/// No file allows `unsafe_code` except [`UNSAFE_FILES`] of [`UNSAFE_CRATE`].
pub(crate) fn check_sources(root: &Path, members: &[String]) -> Vec<String> {
    let mut errors = Vec::new();
    for member in members {
        let dir = if member == "xtask" {
            root.join("xtask")
        } else {
            root.join("crates").join(member)
        };
        let mut files = Vec::new();
        for sub in ["src", "tests", "benches", "examples", "build.rs"] {
            collect(&dir.join(sub), 0, &mut files);
        }
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                errors.push(format!("cannot read {}", file.display()));
                continue;
            };
            let relative = file
                .strip_prefix(&dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let allowed = member == UNSAFE_CRATE && UNSAFE_FILES.contains(&relative.as_str());
            for line_no in allows_unsafe(&text) {
                if !allowed {
                    errors.push(format!(
                        "{}:{line_no}: `unsafe_code` may only be allowed in {UNSAFE_CRATE}'s \
                         {} (docs/plugin-hosting.md)",
                        file.display(),
                        UNSAFE_FILES.join(", ")
                    ));
                }
            }
        }
    }
    errors
}

/// `key = value` lines of the TOML table `[table]`, comments and blanks skipped.
fn entries(manifest: &str, table: &str) -> Vec<(String, String)> {
    let header = format!("[{table}]");
    let mut inside = false;
    let mut found = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.split(" #").next().unwrap_or_default();
            found.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    found
}

/// 1-based numbers of non-comment lines with an attribute allowing `unsafe_code`.
fn allows_unsafe(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or_default().replace(' ', "");
            ["allow(", "expect(", "warn("]
                .iter()
                .any(|attr| code.contains(attr) && code.contains("unsafe_code"))
        })
        .map(|(index, _)| index.saturating_add(1))
        .collect()
}

fn collect(path: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    if depth > 16 {
        return;
    }
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "rs") {
            out.push(path.to_owned());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect(&entry.path(), depth.saturating_add(1), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_tables_entries() {
        let toml = "[a]\nx = 1\n[lints.rust]\n# c\nunsafe_code = \"deny\" # why\nfoo = { level = \"warn\" }\n[b]\ny = 2\n";
        assert_eq!(
            entries(toml, "lints.rust"),
            [
                ("unsafe_code".to_owned(), "\"deny\"".to_owned()),
                ("foo".to_owned(), "{ level = \"warn\" }".to_owned())
            ]
        );
    }

    #[test]
    fn finds_attributes_allowing_unsafe() {
        let src = "#![allow(unsafe_code)]\n// #[allow(unsafe_code)]\n#[allow(dead_code)]\n#[allow( unsafe_code )]\n";
        assert_eq!(allows_unsafe(src), [1, 4]);
    }

    #[test]
    fn the_workspace_follows_the_rule() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(check_lints(root), Vec::<String>::new());
    }
}
