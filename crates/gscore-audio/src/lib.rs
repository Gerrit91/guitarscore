//! gscore-audio crate - Audio output backends

//! This crate provides audio output functionality for GuitarScore,
//! including MIDI output and internal synth implementations.

// Placeholder for actual audio implementation
pub mod midi;
pub mod player;
pub mod synth;

/// A placeholder struct for the audio crate
#[derive(Debug, Default)]
pub struct AudioPlaceholder;

impl AudioPlaceholder {
    /// Placeholder function
    pub fn new() -> Self {
        AudioPlaceholder
    }
}
