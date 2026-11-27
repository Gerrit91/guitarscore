//! Hosting CLAP audio plugins (M24, design note `docs/plugin-hosting.md`).
//!
//! A plugin is native code in a library file (`.clap`) that the user installed: GuitarScore
//! opens the library ([`Library`]), lists the plugins in it ([`PluginInfo`]), makes one an
//! effect ([`Effect`]) and runs its sound through it ([`Processor`]).
//!
//! **This crate is the workspace's one exception to `unsafe_code = "forbid"`** (owner's
//! approval, 2026-11-26): opening a native library cannot be written in safe Rust. The
//! `unsafe` is confined to `src/library.rs` (and the test plugin,
//! `examples/test_plugin.rs`); everything else goes through clack's safe wrappers.
//!
//! Native builds only: there are no plugins in the browser. A plugin's code can crash or
//! hang in ways Rust cannot catch, so the app runs each plugin in a host process of its
//! own ([`remote`], [`serve`]): a crash takes only that process down.

mod editor;
mod effect;
mod error;
mod host;
mod library;
pub mod remote;
mod scan;
mod serve;
mod wire;

pub use effect::{AudioConfig, Effect, Param, Processor, Requests};
pub use error::PluginError;
pub use library::{Library, PluginInfo};
pub use scan::{bundles_in, default_dirs};
pub use serve::serve;

/// How the host introduces itself to plugins.
const HOST_NAME: &str = "GuitarScore";
