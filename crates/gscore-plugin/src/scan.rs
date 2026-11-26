//! Where plugin libraries are installed, and finding them there. Only file names are
//! looked at: no library is opened here.

use std::path::{Path, PathBuf};

/// How deep to look below a plugin folder (vendors nest their plugins in folders).
const MAX_DEPTH: usize = 6;
/// At most this many libraries are listed (a folder full of links must not run away).
const MAX_BUNDLES: usize = 10_000;

/// The folders CLAP plugins are installed in on this system (CLAP's own list): those in
/// `CLAP_PATH` first, then the user's, then the system's.
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("CLAP_PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(home) = &home {
            dirs.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        }
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    } else if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Programs\\Common\\CLAP"));
        }
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            dirs.push(PathBuf::from(common).join("CLAP"));
        }
    } else {
        if let Some(home) = &home {
            dirs.push(home.join(".clap"));
        }
        dirs.push(PathBuf::from("/usr/lib/clap"));
        dirs.push(PathBuf::from("/usr/local/lib/clap"));
    }
    dirs
}

/// The plugin libraries (`.clap` files, or bundles on macOS) in `dirs` and below, sorted,
/// each once. Folders that cannot be read are skipped.
pub fn bundles_in(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in dirs {
        if is_bundle(dir) {
            found.push(dir.clone());
        } else {
            walk(dir, 0, &mut found);
        }
    }
    found.sort();
    found.dedup();
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || found.len() >= MAX_BUNDLES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if found.len() >= MAX_BUNDLES {
            return;
        }
        let path = entry.path();
        if is_bundle(&path) {
            found.push(path);
        } else if path.is_dir() {
            walk(&path, depth.saturating_add(1), found);
        }
    }
}

/// A `.clap` file, or a `.clap` folder (a macOS bundle), by its name.
fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
        && (path.is_file() || cfg!(target_os = "macos") && path.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_libraries_in_nested_folders_once() {
        let root = std::env::temp_dir().join(format!("gscore-plugin-scan-{}", std::process::id()));
        let nested = root.join("Vendor").join("Deeper");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("a.clap"), b"").unwrap();
        std::fs::write(nested.join("b.CLAP"), b"").unwrap();
        std::fs::write(nested.join("readme.txt"), b"").unwrap();
        let found = bundles_in(&[root.clone(), root.clone(), root.join("missing")]);
        assert_eq!(
            found,
            vec![root.join("Vendor/Deeper/b.CLAP"), root.join("a.clap")]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_system_folders_are_listed() {
        assert!(!default_dirs().is_empty());
    }
}
