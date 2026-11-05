//! The browser build's bundled SoundFont: download a pinned, checked GeneralUser GS at
//! build time (never commit the 30 MB file), copy it into the site with its license, and
//! write the manifest the page uses to find it.
//!
//! Source: <https://github.com/mrbumpy409/GeneralUser-GS> at commit
//! `9704918` (v2.0.3). The author asks that visitors never download from GitHub, so the
//! site serves its own copy; downloading once at build time is fine.

use std::path::{Path, PathBuf};

use crate::sha256;
use crate::{TaskResult, cmd};

/// The pinned commit of the GeneralUser GS repository (v2.0.3, 2026-02-23).
pub(crate) const COMMIT: &str = "97049183643d5fc5a9322a69c5b09efb667c6c3a";
/// The file's name in the repository, and its name inside the site.
pub(crate) const FILE: &str = "GeneralUser-GS-2.0.3.sf2";
/// The file's name in the upstream repository.
const UPSTREAM_FILE: &str = "GeneralUser-GS.sf2";
/// The display name in the manifest and the app.
pub(crate) const NAME: &str = "GeneralUser GS 2.0.3";
/// The exact size of v2.0.3 at [`COMMIT`].
pub(crate) const BYTES: u64 = 32_319_396;
/// The exact SHA-256 of v2.0.3 at [`COMMIT`].
pub(crate) const SHA256: &str = "9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe";
/// The license in the upstream repository at the same commit.
pub(crate) const LICENSE_FILE: &str = "GeneralUser-GS-LICENSE.txt";
/// The smallest file accepted from `--soundfont` (a real GM SoundFont is far larger).
const MIN_LOCAL_BYTES: u64 = 1_000_000;

/// Which SoundFont the site gets.
#[derive(Debug, Clone)]
pub(crate) enum Source {
    /// Download (or reuse the cached) GeneralUser GS at the pinned commit.
    Bundled,
    /// Use a local file; its size and RIFF header are checked, not its hash.
    Local(PathBuf),
    /// No SoundFont: playback is silent, and the build says so.
    None,
}

/// Puts the SoundFont, its license and `soundfont.json` into `out/assets/soundfonts`.
pub(crate) fn install(root: &Path, out: &Path, source: &Source) -> TaskResult {
    let dir = out.join("assets").join("soundfonts");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let (file_name, display_name, bytes) = match source {
        Source::None => {
            println!("web: --no-soundfont: the site has no SoundFont and playback is silent");
            return Ok(());
        }
        Source::Local(path) => local(root, path, &dir)?,
        Source::Bundled => bundled(root, &dir)?,
    };
    let hash = sha256::hex(&bytes);
    let manifest = manifest_json(&file_name, &display_name, bytes.len(), &hash);
    std::fs::write(dir.join("soundfont.json"), manifest)
        .map_err(|e| format!("cannot write the SoundFont manifest: {e}"))?;
    println!(
        "web: SoundFont {file_name}: {} (sha256 {hash})",
        human(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    );
    Ok(())
}

/// Reports the SoundFont's size in the site (raw and gzip), for the release notes.
pub(crate) fn report_size(out: &Path) {
    let path = out.join("assets").join("soundfonts").join(FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let gzip = crate::web::compressed_size("gzip", &["-9", "-c"], &path).unwrap_or(0);
    println!(
        "web: assets/soundfonts/{FILE}: {} raw, {} gzip",
        human(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        human(gzip)
    );
}

/// A local file the owner chose.
fn local(root: &Path, path: &Path, dir: &Path) -> Result<(String, String, Vec<u8>), String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read the SoundFont {}: {e}", path.display()))?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size < MIN_LOCAL_BYTES {
        return Err(format!(
            "{} is {size} bytes, too small for a SoundFont",
            path.display()
        ));
    }
    if !looks_like_soundfont(&bytes) {
        return Err(format!(
            "{} does not start with a SoundFont header (RIFF/sfbk)",
            path.display()
        ));
    }
    let name = path
        .file_name()
        .map(|name| sanitize(&name.to_string_lossy()))
        .unwrap_or_else(|| FILE.to_owned());
    let display = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "SoundFont".to_owned());
    std::fs::write(dir.join(&name), &bytes)
        .map_err(|e| format!("cannot write the SoundFont into the site: {e}"))?;
    copy_license(root, dir)?;
    Ok((name, display, bytes))
}

/// The pinned GeneralUser GS, from the cache or downloaded once.
fn bundled(root: &Path, dir: &Path) -> Result<(String, String, Vec<u8>), String> {
    let cache_dir = root.join("target").join("web-soundfont");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("cannot create {}: {e}", cache_dir.display()))?;
    let cached = cache_dir.join(FILE);
    if !cached.is_file() {
        download(&cached)?;
    }
    let bytes = std::fs::read(&cached)
        .map_err(|e| format!("cannot read the SoundFont cache {}: {e}", cached.display()))?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let hash = sha256::hex(&bytes);
    if size != BYTES || hash != SHA256 {
        return Err(format!(
            "the cached SoundFont {} does not match the pinned GeneralUser GS 2.0.3:\n  \
             size {size}, sha256 {hash}\n  expected size {BYTES}, sha256 {SHA256}\n\
             Delete it and rebuild to download it again.",
            cached.display()
        ));
    }
    std::fs::write(dir.join(FILE), &bytes)
        .map_err(|e| format!("cannot write the SoundFont into the site: {e}"))?;
    copy_license(root, dir)?;
    Ok((FILE.to_owned(), NAME.to_owned(), bytes))
}

/// Downloads the pinned file into the cache through `curl`.
fn download(destination: &Path) -> TaskResult {
    let curl = which(&["curl"]).ok_or_else(|| {
        "`curl` is needed to download the SoundFont; install it, or pass --soundfont PATH, \
         or build with --no-soundfont"
            .to_owned()
    })?;
    let temporary = destination.with_extension("part");
    let destination_text = temporary.to_string_lossy().into_owned();
    println!("web: downloading GeneralUser GS 2.0.3 ({BYTES} bytes)");
    let url = format!(
        "https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/{COMMIT}/{UPSTREAM_FILE}"
    );
    cmd::run_status(
        &curl,
        &["-fL", "--retry", "3", "-o", &destination_text, &url],
        Path::new("."),
    )?;
    std::fs::rename(&temporary, destination).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        format!("cannot keep the downloaded SoundFont: {e}")
    })
}

/// Copies the license committed next to the other SoundFont licenses.
fn copy_license(root: &Path, dir: &Path) -> TaskResult {
    let from = root.join("assets").join("soundfonts").join(LICENSE_FILE);
    let to = dir.join(LICENSE_FILE);
    std::fs::copy(&from, &to).map_err(|e| {
        format!(
            "cannot copy the SoundFont license {} to {}: {e}",
            from.display(),
            to.display()
        )
    })?;
    Ok(())
}

/// The manifest the page reads before fetching the font. The name carries the version, so
/// a new font gets a new URL and the service worker can drop the old cache entry.
pub(crate) fn manifest_json(file: &str, name: &str, bytes: usize, sha256: &str) -> String {
    format!(
        "{{\n  \"url\": \"{}\",\n  \"name\": \"{}\",\n  \"bytes\": {},\n  \"sha256\": \"{}\"\n}}\n",
        escape(file),
        escape(name),
        bytes,
        escape(sha256)
    )
}

/// Whether the first bytes are a SoundFont 2 RIFF container.
pub(crate) fn looks_like_soundfont(bytes: &[u8]) -> bool {
    bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"sfbk")
}

/// A file name safe for a URL and the filesystem.
fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out.starts_with('.') {
        out.insert(0, '_');
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn human(bytes: u64) -> String {
    let bytes = u32::try_from(bytes).map(f64::from).unwrap_or(f64::MAX);
    if bytes >= 1024.0 * 1024.0 {
        format!("{:.2} MiB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.0} KiB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}

/// The first program found on `PATH`.
fn which(names: &[&str]) -> Option<std::ffi::OsString> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_is_the_json_the_page_reads() {
        let text = manifest_json("GeneralUser-GS-2.0.3.sf2", "GeneralUser GS 2.0.3", 12, "ab");
        assert!(text.contains("\"url\": \"GeneralUser-GS-2.0.3.sf2\""));
        assert!(text.contains("\"name\": \"GeneralUser GS 2.0.3\""));
        assert!(text.contains("\"bytes\": 12"));
        assert!(text.contains("\"sha256\": \"ab\""));
        // A quote in a name cannot break the JSON.
        let text = manifest_json("a.sf2", "a \"quoted\" font", 1, "00");
        assert!(text.contains("a \\\"quoted\\\" font"));
    }

    #[test]
    fn the_soundfont_header_and_local_names_are_checked() {
        let mut bytes = vec![0_u8; 16];
        bytes[..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"sfbk");
        assert!(looks_like_soundfont(&bytes));
        bytes[8] = b'x';
        assert!(!looks_like_soundfont(&bytes));
        assert!(!looks_like_soundfont(b"RIFF"));
        assert_eq!(sanitize("My Font 1.2.sf2"), "My_Font_1.2.sf2");
        assert_eq!(sanitize("../x"), "_.._x");
    }
}
