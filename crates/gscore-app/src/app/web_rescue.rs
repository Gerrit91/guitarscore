//! The browser's rescue copies: each tab with unsaved edits hands the page a Guitar Pro 7
//! copy of its song, the same bytes Save would download, once the edits have rested for
//! [`REST`]. The page (`web/recovery.js`) keeps them outside the wasm instance, in memory
//! and in IndexedDB:
//!
//! - a panic aborts the instance (there is no unwinding to contain it, as on the desktop):
//!   the page's crash screen (`web/index.html`) offers this page's copies for download;
//! - a closed tab, a reload or a crash leaves the copies stored: at the next start the
//!   copies of pages that are gone are offered one by one (Restore, Discard, Later), as
//!   the desktop offers a crashed document's journal (`app/recovery.rs`). A restored copy
//!   opens unsaved, without its undo history.

use std::collections::HashMap;

use wasm_bindgen::{JsCast, JsValue};

use super::GuitarScoreApp;
use super::alert::{AlertIcon, Answer, Reply, alert};
use super::files::Dialog;
use super::tab::TabId;
use crate::document::Document;

/// How long the edits must rest before a copy is made: writing a large song takes a few
/// milliseconds, too long to do on every keystroke.
const REST: f64 = 1.0;
/// The page's recovery store (`web/recovery.js`).
const STORE: &str = "guitarscoreRescue";
/// How long the page may take to read its stored copies before none are offered.
const FIND_FOR: f64 = 10.0;

/// What has been handed to the page.
#[derive(Debug, Default)]
pub(super) struct Rescue {
    /// By tab: the revision the page holds a copy of.
    copied: HashMap<TabId, u64>,
    /// By tab: the revision last seen and when (egui time), to know when edits rest.
    seen: HashMap<TabId, (u64, f64)>,
    /// The copies of pages that are gone, still to be asked about (newest last); `None`
    /// until the page has read them.
    found: Option<Vec<FoundCopy>>,
}

/// A copy a page that is gone left in storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FoundCopy {
    /// The page's key for it.
    id: String,
    /// The song's file name.
    name: String,
    /// When it was kept, as the browser writes a local date and time.
    when: String,
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

impl GuitarScoreApp {
    /// Offers the copies earlier pages left, once the page has read them (called every
    /// frame; cheap once they are all asked about).
    pub(super) fn ask_about_rescued_copies(&mut self, ctx: &egui::Context) {
        if self.rescue.found.is_none() {
            match found_copies() {
                Some(mut copies) => {
                    // Asked newest first; `pop` takes from the end.
                    copies.reverse();
                    self.rescue.found = Some(copies);
                }
                None if ctx.input(|input| input.time) < FIND_FOR => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                None => self.rescue.found = Some(Vec::new()),
            }
        }
        if self.dialog.is_none()
            && let Some(copy) = self.rescue.found.as_mut().and_then(Vec::pop)
        {
            mark_page("asking");
            self.dialog = Some(Dialog::RecoverCopy(copy));
        }
    }

    /// The question about one copy.
    pub(super) fn recover_copy_dialog(&mut self, ctx: &egui::Context, copy: FoundCopy) {
        let text = format!(
            "GuitarScore was closed while “{}” had unsaved edits. They were kept on {} \
             and can be brought back.",
            copy.name, copy.when
        );
        let reply = alert(
            ctx,
            "Recovery",
            AlertIcon::Restore,
            "Recover unsaved changes?",
            |ui| {
                ui.label(text);
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Later keeps them for the next visit.")
                        .small()
                        .weak(),
                );
            },
            &[
                Answer {
                    text: "Discard",
                    main: false,
                },
                Answer {
                    text: "Later",
                    main: false,
                },
                Answer {
                    text: "Restore",
                    main: true,
                },
            ],
        );
        match reply {
            Reply::Answer(2) => {
                self.dialog = None;
                self.restore_copy(&copy);
            }
            Reply::Answer(0) => {
                self.dialog = None;
                call(&"discard".into(), &[JsValue::from_str(&copy.id)]);
                mark_page("discarded");
            }
            Reply::Answer(_) | Reply::Closed => {
                // Kept for the next visit.
                self.dialog = None;
                mark_page("kept");
            }
            Reply::Open => {}
        }
    }

    /// Opens `copy` in a new tab, unsaved; the page keeps it as that tab's copy from now
    /// on. A copy that cannot be read stays stored and is offered again next time.
    fn restore_copy(&mut self, copy: &FoundCopy) {
        let bytes = call(&"bytes".into(), &[JsValue::from_str(&copy.id)])
            .filter(|value| value.is_instance_of::<js_sys::Uint8Array>())
            .map(|value| js_sys::Uint8Array::from(value).to_vec());
        let Some(bytes) = bytes else {
            self.notice = Some(format!(
                "Could not recover “{}”: its copy is gone.",
                copy.name
            ));
            mark_page("failed");
            return;
        };
        let path = std::path::PathBuf::from(&copy.name);
        let imported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gscore_session::open_bytes(&path, bytes)
        }));
        let song = match imported {
            Ok(Ok((imported, _))) => super::files::with_song_state(imported.song),
            Ok(Err(error)) => {
                self.notice = Some(format!("Could not recover “{}”: {error}", copy.name));
                mark_page("failed");
                return;
            }
            Err(_) => {
                self.notice = Some(format!(
                    "Could not recover “{}”: reading it failed unexpectedly",
                    copy.name
                ));
                mark_page("failed");
                return;
            }
        };
        let editor = gscore_edit::TabEditor::new(song, gscore_model::TrackIndex::new(0));
        // Not reopened at the next start: the browser's copy of the file under that name
        // is the song before these edits.
        self.push_tab(Document::recovered(
            path,
            editor,
            gscore_session::FileFormat::Gp,
            None,
            false,
        ));
        if let Some(tab) = self.tabs.last() {
            call(
                &"adopt".into(),
                &[JsValue::from_str(&copy.id), tab_key(tab.id)],
            );
            self.rescue
                .copied
                .insert(tab.id, tab.document.editor().revision());
        }
        self.status = Some(format!("Recovered “{}”, not saved yet", copy.name));
        mark_page("restored");
    }
}

/// The copies the page found (`found` in `web/recovery.js`), or `None` while it is still
/// reading them. Entries that are not well-formed are left out.
fn found_copies() -> Option<Vec<FoundCopy>> {
    let store: JsValue = store()?.into();
    let found = js_sys::Reflect::get(&store, &"found".into()).ok()?;
    if !js_sys::Array::is_array(&found) {
        return None;
    }
    let text = |entry: &JsValue, key: &str| {
        js_sys::Reflect::get(entry, &key.into())
            .ok()
            .and_then(|value| value.as_string())
    };
    Some(
        js_sys::Array::from(&found)
            .iter()
            .filter_map(|entry| {
                Some(FoundCopy {
                    id: text(&entry, "id")?,
                    name: text(&entry, "name")?,
                    when: text(&entry, "when")?,
                })
            })
            .collect(),
    )
}

/// Tells the page (and the smoke test) where the recovery question stands:
/// `data-guitarscore-recovery` on `<body>`.
fn mark_page(state: &str) {
    if let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    {
        let _ = body.set_attribute("data-guitarscore-recovery", state);
    }
}

/// Calls the store's method `name` with `args`; `None` without a store or on an error.
fn call(name: &JsValue, args: &[JsValue]) -> Option<JsValue> {
    let store = store()?;
    let function: js_sys::Function = js_sys::Reflect::get(&store, name).ok()?.dyn_into().ok()?;
    let args: js_sys::Array = args.iter().collect();
    function.apply(&store, &args).ok()
}

/// The page's store (`web/recovery.js`), if the page has one.
fn store() -> Option<js_sys::Object> {
    let store = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str(STORE)).ok()?;
    store.is_object().then(|| store.into())
}

fn tab_key(id: TabId) -> JsValue {
    JsValue::from_str(&format!("tab-{}", id.number()))
}

/// Hands the page a copy of tab `id`'s song, `bytes` to be downloaded as `name`.
fn keep(id: TabId, name: &str, bytes: &[u8]) {
    call(
        &"keep".into(),
        &[
            tab_key(id),
            JsValue::from_str(name),
            js_sys::Uint8Array::from(bytes).into(),
        ],
    );
}

/// Takes tab `id`'s copy back (saved or closed).
fn forget(id: TabId) {
    call(&"forget".into(), &[tab_key(id)]);
}
