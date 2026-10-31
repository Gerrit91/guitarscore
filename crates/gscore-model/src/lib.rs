//! The `gscore-model` crate provides the core data structures for a guitar score,
//! completely independent of any specific file format or IO crate.
//!
//! This crate defines the "Internal Representation" (IR) that the rest of the 
//! application (layout, playback, editing) depends on.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::unreachable)]
#![deny(clippy::indexing_slicing)]

//! A thin layer over the guitarpro crate for GuitarScore
//!
//! This crate provides:
//! - Helper methods for musical element access
//! - Position types (track/measure/voice/beat indices)
//! - Invariant checks to maintain data integrity
//! - Stable, non-panicking interface

/// Represents a complete musical score.
#[derive(Debug, Clone)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub tempo: i16,
    pub tracks: Vec<Track>,
}

impl Song {
    pub fn new(title: String, artist: String, tempo: i16, tracks: Vec<Track>) -> Self {
        Self { title, artist, tempo, tracks }
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn measure_count(&self) -> usize {
        self.tracks.first().map(|t| t.measures.len()).unwrap_or(0)
    }

    pub fn get_tempo(&self) -> i16 {
        self.tempo
    }
}

/// Represents a single instrument track.
#[derive(Debug, Clone)]
pub struct Track {
    pub name: String,
    pub measures: Vec<Measure>,
    pub tuning: Vec<u8>, // MIDI note numbers for each string
}

/// Represents a single measure (bar) of music.
#[derive(Debug, Clone)]
pub struct Measure {
    pub number: usize,
    pub voices: Vec<Voice>,
    pub time_signature: (u8, u8), // (numerator, denominator)
}

/// Represents a musical voice within a measure.
#[derive(Debug, Clone)]
pub struct Voice {
    pub beats: Vec<Beat>,
}

/// Represents a beat (a time slice) within a voice.
#[derive(Debug, Clone)]
pub struct Beat {
    pub duration: f32, // duration in beats (e.g., 0.25 for 16th note)
    pub notes: Vec<Note>,
}

/// Represents a single note played on a string.
#[derive(Debug, Clone)]
pub struct Note {
    pub fret: i8,
    pub string: u8,
    pub duration: f32,
    pub is_bent: bool,
    pub is_slide: bool,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_song_creation() {
        let tracks = vec![Track { 
            name: "Guitar 1".to_string(), 
            measures: vec![], 
            tuning: vec![64, 59, 55, 50, 45, 40] 
        }];
        let song = Song::new(
            "Test Song".to_string(), 
            "Test Artist".to_string(), 
            120, 
            tracks
        );
        assert_eq!(song.track_count(), 1);
    }
}