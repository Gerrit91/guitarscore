//! A plugin library file (`.clap`) opened, and the plugins it offers.
//!
//! UNSAFE EXCEPTION: this file holds the only `unsafe` code of GuitarScore's application
//! code (besides the test plugin). Approved by the owner on 2026-11-26 for plugin hosting
//! (M24), design note `docs/plugin-hosting.md`, checked by `cargo xtask layers`.
//!
//! Why it is needed: a plugin is native code in a dynamic library. Opening the library runs
//! its initializers and hands us function pointers that we must trust to follow the CLAP
//! ABI; Rust cannot check either, so `PluginEntry::load` is an `unsafe fn`. Everything after
//! the load goes through clack's safe wrappers, which check what can be checked (null
//! pointers, strings, thread rules).
#![allow(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};

use clack_host::prelude::PluginEntry;

use crate::PluginError;

/// An opened plugin library. Cheap to clone; the library stays loaded while a clone or an
/// effect made from it lives.
#[derive(Clone)]
pub struct Library {
    path: PathBuf,
    entry: PluginEntry,
}

impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library").field("path", &self.path).finish()
    }
}

/// One plugin a library offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    /// Unique and stable (reverse domain name, "com.example.reverb").
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    /// CLAP's feature words: "audio-effect", "instrument", "reverb", "stereo", ...
    pub features: Vec<String>,
}

impl PluginInfo {
    /// Whether the plugin makes sound from notes.
    pub fn is_instrument(&self) -> bool {
        self.features.iter().any(|f| f == "instrument")
    }

    /// Whether the plugin changes sound it is given.
    pub fn is_effect(&self) -> bool {
        self.features.iter().any(|f| f == "audio-effect")
    }
}

impl Library {
    /// Opens the plugin library at `path`.
    ///
    /// This runs the library's own code. A library that crashes while it loads takes the
    /// process with it; that is why scanning moves into the separate host process.
    pub fn open(path: &Path) -> Result<Self, PluginError> {
        let fail = |reason: String| PluginError::Open {
            path: path.to_owned(),
            reason,
        };
        // SAFETY: Loading a library runs its initializers, and clack then calls its
        // `clap_entry` functions, trusting them to follow the CLAP ABI. Neither can be
        // checked: the trust is the user's, who installed the plugin and chose it. Memory
        // safety on our side holds as long as the library is a correct CLAP library; a
        // faulty one is contained by the separate host process (design note
        // `docs/plugin-hosting.md`), not by Rust. clack keeps the library loaded for as
        // long as any `PluginEntry` clone or plugin instance made from it lives, so no
        // function pointer outlives its code.
        let entry = unsafe { PluginEntry::load(path) }.map_err(|e| fail(e.to_string()))?;
        Ok(Self {
            path: path.to_owned(),
            entry,
        })
    }

    /// Where the library was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The plugins in the library (none if it offers no plugin factory). Descriptors with
    /// no id are skipped: they cannot be made.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        let Some(factory) = self.entry.get_plugin_factory() else {
            return Vec::new();
        };
        let text = |s: Option<&std::ffi::CStr>| {
            s.map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        factory
            .plugin_descriptors()
            .filter_map(|d| {
                let id = d.id()?.to_string_lossy().into_owned();
                Some(PluginInfo {
                    name: text(d.name()),
                    vendor: text(d.vendor()),
                    version: text(d.version()),
                    features: d
                        .features()
                        .map(|f| f.to_string_lossy().into_owned())
                        .collect(),
                    id,
                })
            })
            .collect()
    }

    pub(crate) fn entry(&self) -> &PluginEntry {
        &self.entry
    }
}
