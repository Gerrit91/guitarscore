//! Recent songs kept in the browser (M25): a song opened or saved in the browser build is
//! copied into `localStorage`, so the recent files list opens it again without the file
//! picker, also offline once the site is installed. A desktop has the files themselves.
//!
//! `localStorage` holds a few megabytes per site and is shared with the settings, so the
//! copies have a budget: songs up to [`MAX_SONG_BYTES`], at most [`MAX_SONGS`] of them
//! and [`BUDGET_BYTES`] together; the songs used longest ago go first. Everything read
//! back is treated as untrusted (another tab, an extension or a full disk may have
//! changed it): a broken index or copy is dropped, never a reason to fail.

// The budget is used by the browser build only; the desktop keeps no copies.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// The largest song that is kept (Guitar Pro files of whole albums exist; a practice
/// copy is not worth the space).
pub(crate) const MAX_SONG_BYTES: usize = 768 * 1024;
/// How many songs are kept.
pub(crate) const MAX_SONGS: usize = 8;
/// How many bytes of songs are kept together (stored as base64, a third more).
pub(crate) const BUDGET_BYTES: usize = 3 * 1024 * 1024;

/// One kept song: its file name and size, newest first in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// The file name it was opened or saved as.
    pub name: String,
    /// Its size in bytes.
    pub size: usize,
}

/// The index after `name` (of `size` bytes) is kept: it first, then the others newest
/// first while they fit in the budget. Returns the new index and the names that no
/// longer fit (their copies are to be removed). `None` when the song itself is too large
/// to keep.
pub(crate) fn plan(index: &[Entry], name: &str, size: usize) -> Option<(Vec<Entry>, Vec<String>)> {
    if size > MAX_SONG_BYTES || name.is_empty() {
        return None;
    }
    let mut kept = vec![Entry {
        name: name.to_owned(),
        size,
    }];
    let mut total = size;
    let mut dropped = Vec::new();
    for entry in index {
        // A name listed twice (a tampered index) is kept once; its copy stays.
        if kept.iter().any(|other| other.name == entry.name) {
            continue;
        }
        let fits = kept.len() < MAX_SONGS
            && entry.size <= MAX_SONG_BYTES
            && total.saturating_add(entry.size) <= BUDGET_BYTES;
        if fits {
            total += entry.size;
            kept.push(entry.clone());
        } else if !dropped.contains(&entry.name) {
            dropped.push(entry.name.clone());
        }
    }
    Some((kept, dropped))
}

#[cfg(target_arch = "wasm32")]
pub(crate) use web::{is_kept, keep, load};

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Entry, MAX_SONG_BYTES, plan};

    /// The index of kept songs.
    const INDEX: &str = "guitarscore.songs";
    /// The prefix of a kept song's entry; its file name follows.
    const SONG: &str = "guitarscore.song:";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    fn read_index(storage: &web_sys::Storage) -> Vec<Entry> {
        storage
            .get_item(INDEX)
            .ok()
            .flatten()
            .filter(|text| text.len() < 64 * 1024)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keeps a copy of the song `name` (its file name). Too large a song, or a full
    /// storage, keeps nothing new; older copies are dropped to make room first.
    pub(crate) fn keep(name: &str, bytes: &[u8]) {
        let Some(storage) = storage() else {
            return;
        };
        let index = read_index(&storage);
        let Some((kept, dropped)) = plan(&index, name, bytes.len()) else {
            return;
        };
        for old in &dropped {
            let _ = storage.remove_item(&format!("{SONG}{old}"));
        }
        let text = crate::base64::encode(bytes);
        if storage.set_item(&format!("{SONG}{name}"), &text).is_err() {
            // Full (the settings and other sites' data share the quota): this song is
            // not kept, the index keeps what is still there.
            let rest: Vec<Entry> = kept
                .into_iter()
                .filter(|entry| entry.name != name)
                .collect();
            if let Ok(text) = serde_json::to_string(&rest) {
                let _ = storage.set_item(INDEX, &text);
            }
            return;
        }
        if let Ok(text) = serde_json::to_string(&kept) {
            let _ = storage.set_item(INDEX, &text);
        }
    }

    /// Whether a copy of the song `name` is kept (for the recent songs list).
    pub(crate) fn is_kept(name: &str) -> bool {
        storage().is_some_and(|storage| read_index(&storage).iter().any(|entry| entry.name == name))
    }

    /// The kept copy of the song `name`, if there is one and it is intact.
    pub(crate) fn load(name: &str) -> Option<Vec<u8>> {
        let storage = storage()?;
        let key = format!("{SONG}{name}");
        let text = storage.get_item(&key).ok()??;
        // Base64 is four characters for three bytes.
        if text.len() > MAX_SONG_BYTES / 3 * 4 + 8 {
            let _ = storage.remove_item(&key);
            return None;
        }
        let bytes = crate::base64::decode(&text);
        if bytes.is_none() {
            let _ = storage.remove_item(&key);
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: usize) -> Entry {
        Entry {
            name: name.to_owned(),
            size,
        }
    }

    #[test]
    fn a_kept_song_goes_first_and_the_oldest_leave_when_the_budget_is_full() {
        let index = vec![entry("b.gp", 1_000), entry("a.gp", 2_000)];
        let (kept, dropped) = plan(&index, "a.gp", 2_500).unwrap();
        assert_eq!(kept, vec![entry("a.gp", 2_500), entry("b.gp", 1_000)]);
        assert!(dropped.is_empty());

        let full: Vec<Entry> = (0..MAX_SONGS)
            .map(|i| entry(&format!("{i}.gp"), 10))
            .collect();
        let (kept, dropped) = plan(&full, "new.gp", 10).unwrap();
        assert_eq!(kept.len(), MAX_SONGS);
        assert_eq!(dropped, vec![format!("{}.gp", MAX_SONGS - 1)]);

        let big = vec![entry("x.gp", BUDGET_BYTES - 100), entry("y.gp", 50)];
        let (kept, dropped) = plan(&big, "z.gp", 200).unwrap();
        assert_eq!(kept, vec![entry("z.gp", 200), entry("y.gp", 50)]);
        assert_eq!(dropped, vec!["x.gp".to_owned()]);
    }

    #[test]
    fn too_large_a_song_or_a_hostile_index_keeps_nothing_wrong() {
        assert!(plan(&[], "huge.gp", MAX_SONG_BYTES + 1).is_none());
        assert!(plan(&[], "", 10).is_none());
        // Duplicates and impossible sizes from a tampered index are dropped.
        let index = vec![
            entry("a.gp", 10),
            entry("a.gp", 10),
            entry("b.gp", usize::MAX),
        ];
        let (kept, dropped) = plan(&index, "c.gp", 10).unwrap();
        assert_eq!(kept, vec![entry("c.gp", 10), entry("a.gp", 10)]);
        assert_eq!(dropped, vec!["b.gp".to_owned()]);
    }
}
