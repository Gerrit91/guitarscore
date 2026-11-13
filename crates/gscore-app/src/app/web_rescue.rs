//! The browser's rescue copies: a panic aborts the wasm instance (there is no unwinding
//! to contain it, as on the desktop), and with it every unsaved edit. So each tab with
//! unsaved edits hands the page a Guitar Pro 7 copy of its song, the same bytes Save would
//! download, once the edits have rested for [`REST`]. The copies live in the page's
//! JavaScript (`globalThis.guitarscoreRescue`, by tab), outside the instance: when it has
//! panicked, the page's crash screen (`web/index.html`) offers them for download.
//!
//! They are kept in memory only, so they do not survive a closed tab or a reload (a
//! storage-backed journal is still open, `docs/gaps.md`).

use std::collections::HashMap;

use wasm_bindgen::JsValue;

use super::GuitarScoreApp;
use super::tab::TabId;

/// How long the edits must rest before a copy is made: writing a large song takes a few
/// milliseconds, too long to do on every keystroke.
const REST: f64 = 1.0;
/// The page's object holding the copies.
const STORE: &str = "guitarscoreRescue";

/// What has been handed to the page.
#[derive(Debug, Default)]
pub(super) struct Rescue {
    /// By tab: the revision the page holds a copy of.
    copied: HashMap<TabId, u64>,
    /// By tab: the revision last seen and when (egui time), to know when edits rest.
    seen: HashMap<TabId, (u64, f64)>,
}

impl GuitarScoreApp {
    /// Brings the page's rescue copies up to date (called every frame; cheap unless a copy
    /// is due).
    pub(super) fn keep_rescue_copies(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let open: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        let gone: Vec<TabId> = self
            .rescue
            .copied
            .keys()
            .filter(|id| !open.contains(id))
            .copied()
            .collect();
        for id in gone {
            forget(id);
            self.rescue.copied.remove(&id);
        }
        self.rescue.seen.retain(|id, _| open.contains(id));
        for index in 0..self.tabs.len() {
            let Some(tab) = self.tabs.get(index) else {
                continue;
            };
            let (id, revision) = (tab.id, tab.document.editor().revision());
            if !tab.document.is_dirty() {
                // Saved (or never changed): nothing to rescue.
                if self.rescue.copied.remove(&id).is_some() {
                    forget(id);
                }
                self.rescue.seen.remove(&id);
                continue;
            }
            if self.rescue.copied.get(&id) == Some(&revision) {
                continue;
            }
            let since = match self.rescue.seen.get(&id) {
                Some(&(seen, since)) if seen == revision => since,
                _ => {
                    self.rescue.seen.insert(id, (revision, now));
                    now
                }
            };
            if now - since < REST {
                // Come back when the edits have rested, even without input.
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(REST));
                continue;
            }
            let name = tab.document.path().file_name().map_or_else(
                || "song.gp".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
            let song = self.song_with_state(tab);
            // As Save writes it; what Guitar Pro 7 cannot hold is left out either way. A
            // song that cannot be written now is tried again after the next edit.
            let options = gscore_session::SaveOptions {
                format: gscore_session::FileFormat::Gp,
                allow_losses: true,
            };
            if let Ok((bytes, _)) = gscore_session::save_bytes(&song, options) {
                keep(id, &name, &bytes);
                self.rescue.copied.insert(id, revision);
            }
        }
    }
}

/// The page's store of copies, made on first use.
fn store() -> Option<js_sys::Object> {
    let global = js_sys::global();
    let key = JsValue::from_str(STORE);
    let existing = js_sys::Reflect::get(&global, &key).ok()?;
    if existing.is_object() {
        return Some(existing.into());
    }
    let object = js_sys::Object::new();
    js_sys::Reflect::set(&global, &key, &object).ok()?;
    Some(object)
}

fn tab_key(id: TabId) -> JsValue {
    JsValue::from_str(&format!("tab-{}", id.number()))
}

/// Hands the page a copy of tab `id`'s song, `bytes` to be downloaded as `name`.
fn keep(id: TabId, name: &str, bytes: &[u8]) {
    let Some(store) = store() else {
        return;
    };
    let entry = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&entry, &"name".into(), &JsValue::from_str(name));
    let _ = js_sys::Reflect::set(
        &entry,
        &"bytes".into(),
        &js_sys::Uint8Array::from(bytes).into(),
    );
    let _ = js_sys::Reflect::set(&store, &tab_key(id), &entry);
}

/// Takes tab `id`'s copy back (saved or closed).
fn forget(id: TabId) {
    if let Some(store) = store() {
        let _ = js_sys::Reflect::delete_property(&store, &tab_key(id));
    }
}
