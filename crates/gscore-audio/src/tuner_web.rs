//! The tuner in a browser build: its tone generator is not built yet (the synthesizer runs
//! in an AudioWorklet, and a sine there comes later). Starting it reports that, and the
//! rest of the app goes on.

use gscore_play::PlayError;

/// A tuner that is not available; keeps the app's API the same on every target.
#[derive(Debug)]
pub struct TunerPlayer;

impl TunerPlayer {
    /// Always fails: the browser build has no tuning tone yet.
    pub fn start() -> Result<Self, PlayError> {
        Err(PlayError::Device(
            "the tuner is not available in the browser build yet".to_owned(),
        ))
    }

    /// Always fails, for the same reason.
    pub fn play(&self, _frequency: f32) -> Result<(), PlayError> {
        Err(PlayError::Device(
            "the tuner is not available in the browser build yet".to_owned(),
        ))
    }

    /// Nothing sounds.
    pub fn stop(&self) {}

    /// Nothing ever sounds.
    pub fn is_playing(&self) -> bool {
        false
    }

    /// The reason it does not work.
    pub fn error(&self) -> Option<String> {
        Some("the tuner is not available in the browser build yet".to_owned())
    }

    /// No device is opened.
    pub fn device(&self) -> Option<String> {
        None
    }
}
