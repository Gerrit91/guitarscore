//! A single tone in a file of its own (a `.json` file), for sharing one sound: the strings'
//! sound, the chain after them and what plays them, as a saved tone keeps them, plus the
//! impulse responses (WAV files) its blocks load, so the tone sounds the same on another
//! machine. JSON, with the WAV files in Base64 (the same as the song's own state uses).
//!
//! Importing writes the impulse responses into a folder of this machine's (named by their
//! content, so the same file imported twice is kept once) and points the blocks there.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::ChainPreset;

/// The file name extension of a tone file.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) const EXTENSION: &str = "json";
/// What the file says it is, so another JSON file is not taken for a tone.
const FORMAT: &str = "guitarscore-tone";
/// The format's version; a newer file is refused with a message rather than misread.
const VERSION: u32 = 1;
/// The largest tone file read (impulse responses are short; this is generous).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The largest impulse response put into a file.
const MAX_IMPULSE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct ToneFile {
    format: String,
    version: u32,
    tone: ChainPreset,
    /// The impulse responses the tone's blocks load, by place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    impulses: Vec<Impulse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Impulse {
    /// The chain's place whose block loads it.
    place: usize,
    /// Its file name where it came from (for the name it is kept under).
    name: String,
    /// The WAV file, in Base64.
    wav: String,
}

/// A tone file read: the tone, with its blocks pointing at this machine's copies of its
/// impulse responses, and the blocks whose file could not be kept.
#[derive(Debug)]
pub(crate) struct Imported {
    pub(crate) preset: ChainPreset,
    /// The impulse responses that are missing (their file names), for a message.
    pub(crate) missing: Vec<String>,
}

/// `preset` as a tone file, with the impulse responses its blocks load read from disk.
/// The second value names the files that could not be read (the tone is written without
/// them, its blocks keep their paths).
pub(crate) fn write(preset: &ChainPreset) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut impulses = Vec::new();
    let mut unreadable = Vec::new();
    for (place, block) in preset.chain.blocks.iter().enumerate() {
        let Some(path) = &block.file else {
            continue;
        };
        let name = file_name(path);
        let read = std::fs::metadata(path)
            .map_err(|error| error.to_string())
            .and_then(|metadata| {
                if metadata.len() > MAX_IMPULSE_BYTES {
                    Err("too large".to_owned())
                } else {
                    std::fs::read(path).map_err(|error| error.to_string())
                }
            });
        match read {
            Ok(bytes) => impulses.push(Impulse {
                place,
                name,
                wav: crate::base64::encode(&bytes),
            }),
            Err(_) => unreadable.push(name),
        }
    }
    let file = ToneFile {
        format: FORMAT.to_owned(),
        version: VERSION,
        tone: preset.clone(),
        impulses,
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
    Ok((json, unreadable))
}

/// Reads a tone file, writing its impulse responses into `impulse_dir` (made if needed);
/// without a folder they count as missing.
pub(crate) fn read(bytes: &[u8], impulse_dir: Option<&Path>) -> Result<Imported, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "this is not a tone file".to_owned())?;
    if value.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err("this is not a GuitarScore tone file".to_owned());
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version > u64::from(VERSION) {
        return Err("this tone was saved by a newer GuitarScore; update to import it".to_owned());
    }
    let file: ToneFile = serde_json::from_value(value)
        .map_err(|error| format!("the tone file is damaged: {error}"))?;
    let mut preset = file.tone;
    if preset.name.trim().is_empty() {
        "Imported tone".clone_into(&mut preset.name);
    }
    let mut missing = Vec::new();
    let mut kept = vec![false; preset.chain.blocks.len()];
    for impulse in &file.impulses {
        let Some(block) = preset.chain.blocks.get_mut(impulse.place) else {
            continue;
        };
        let Some(wav) = crate::base64::decode(&impulse.wav) else {
            missing.push(impulse.name.clone());
            continue;
        };
        let kept_at = impulse_dir
            .ok_or_else(|| "no folder".to_owned())
            .and_then(|dir| keep_impulse(&wav, &impulse.name, dir));
        match kept_at {
            Ok(path) => {
                block.file = Some(path);
                if let Some(slot) = kept.get_mut(impulse.place) {
                    *slot = true;
                }
            }
            Err(_) => missing.push(impulse.name.clone()),
        }
    }
    // A block that loads a file the tone did not bring, and that is not here either.
    for (block, kept) in preset.chain.blocks.iter().zip(kept) {
        if let Some(path) = &block.file
            && !kept
            && !path.exists()
        {
            let name = file_name(path);
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
    }
    Ok(Imported { preset, missing })
}

/// Writes `wav` into `dir` under a name made of `name` and its content's checksum (the
/// same file is kept once), unless it is there already.
fn keep_impulse(wav: &[u8], name: &str, dir: &Path) -> Result<PathBuf, String> {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| {
            stem.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "impulse".to_owned());
    let path = dir.join(format!("{stem}-{:08x}.wav", crc32fast::hash(wav)));
    if !path.exists() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        std::fs::write(&path, wav).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_settings::{BlockChoice, BlockSetting, ChainSetting};

    fn preset(file: Option<PathBuf>) -> ChainPreset {
        let mut chain = ChainSetting::default();
        if let Some(slot) = chain.blocks.get_mut(0) {
            *slot = BlockSetting::of(BlockChoice::Amp);
        }
        if let Some(slot) = chain.blocks.get_mut(2) {
            let mut room = BlockSetting::of(BlockChoice::Room);
            room.file = file;
            *slot = room;
        }
        ChainPreset {
            name: "Shared Lead".to_owned(),
            kind: "electric".to_owned(),
            sound: crate::settings::StringSound::default(),
            chain,
            engine: "electric".to_owned(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gscore-tone-file-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp dir");
        dir
    }

    #[test]
    fn a_tone_with_its_impulse_response_comes_back_on_another_machine() {
        let here = temp("export");
        let wav = here.join("Big Hall.wav");
        std::fs::write(&wav, b"RIFF fake wav data").expect("the wav");
        let original = preset(Some(wav.clone()));
        let (bytes, unreadable) = write(&original).expect("written");
        assert!(unreadable.is_empty());
        // The other machine: the original file is gone.
        std::fs::remove_dir_all(&here).expect("removed");
        let there = temp("import");
        let imported = read(&bytes, Some(&there)).expect("read");
        assert!(imported.missing.is_empty(), "{:?}", imported.missing);
        assert_eq!(imported.preset.name, "Shared Lead");
        let file = imported
            .preset
            .chain
            .blocks
            .get(2)
            .and_then(|block| block.file.clone())
            .expect("the room's file");
        assert!(file.starts_with(&there), "{}", file.display());
        assert_eq!(std::fs::read(&file).expect("kept"), b"RIFF fake wav data");
        // Everything else is the tone as it was.
        let mut expected = original;
        if let Some(block) = expected.chain.blocks.get_mut(2) {
            block.file = Some(file);
        }
        assert_eq!(imported.preset, expected);
        let _ = std::fs::remove_dir_all(&there);
    }

    #[test]
    fn a_missing_impulse_response_is_named_and_other_json_is_refused() {
        let dir = temp("missing");
        let (bytes, unreadable) =
            write(&preset(Some(dir.join("gone.wav")))).expect("written without it");
        assert_eq!(unreadable, ["gone.wav"]);
        let imported = read(&bytes, Some(&dir)).expect("read");
        assert_eq!(imported.missing, ["gone.wav"]);

        assert!(read(b"{\"name\": \"x\"}", Some(&dir)).is_err());
        assert!(read(b"not json", Some(&dir)).is_err());
        let newer = br#"{"format": "guitarscore-tone", "version": 99, "tone": {}}"#;
        assert!(
            read(newer, Some(&dir))
                .err()
                .is_some_and(|error| error.contains("newer"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
