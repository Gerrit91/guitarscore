//! The errors of plugin hosting: every one is something the user can act on (pick another
//! plugin, reinstall it), never a reason to stop the app.

use std::path::PathBuf;

/// What went wrong with a plugin.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The library could not be opened, or is not a CLAP plugin library.
    #[error("cannot open the plugin library {path}: {reason}")]
    Open { path: PathBuf, reason: String },
    /// The library has no plugin with this id.
    #[error("the library has no plugin \"{0}\"")]
    NotFound(String),
    /// The plugin refused to start, or failed while starting.
    #[error("the plugin \"{id}\" could not be started: {reason}")]
    Start { id: String, reason: String },
    /// The plugin does something GuitarScore does not support (yet).
    #[error("the plugin \"{id}\" is not supported: {reason}")]
    Unsupported { id: String, reason: String },
    /// Saving or loading the plugin's settings failed.
    #[error("the plugin's settings could not be {action}: {reason}")]
    State {
        action: &'static str,
        reason: String,
    },
    /// The plugin reported an error while processing sound.
    #[error("the plugin failed while processing sound: {0}")]
    Process(String),
}
