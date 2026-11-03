//! Drum notation: where each General MIDI drum sound sits on a five-line percussion staff,
//! and which notehead it gets.
//!
//! Drum tracks store the drum sound as the note's "fret" (its MIDI note number). The map
//! follows the common drum-set convention (as in Guitar Pro and most drum methods): bass
//! drums in the bottom space, snare in the third space, toms from the floor tom up to the
//! top of the staff, hi-hat and cymbals with cross heads above the staff, the pedal hi-hat
//! below it. Sounds without a place of their own share the middle line with a diamond head,
//! so they stay visible instead of disappearing.

use crate::glyph::Glyph;

/// The notehead a drum sound is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrumHead {
    /// An ordinary notehead (hollow for halves and wholes): drums and toms.
    Normal,
    /// A cross: hi-hat, cymbals.
    Cross,
    /// A circled cross: open hi-hat, side stick, effect cymbals.
    CircleCross,
    /// A diamond: ride bell, and sounds the map has no place for.
    Diamond,
    /// A triangle: cowbell and other small percussion.
    Triangle,
}

impl DrumHead {
    /// The glyph that replaces the note value's own head, or `None` for a normal head.
    pub(crate) fn glyph(self) -> Option<Glyph> {
        match self {
            DrumHead::Normal => None,
            DrumHead::Cross => Some(Glyph::NoteheadX),
            DrumHead::CircleCross => Some(Glyph::NoteheadCircleX),
            DrumHead::Diamond => Some(Glyph::NoteheadDiamond),
            DrumHead::Triangle => Some(Glyph::NoteheadTriangle),
        }
    }
}

/// Where a drum sound is written: its staff position (0 bottom line, 4 middle line, 8 top
/// line) and notehead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrumNote {
    pub(crate) position: i32,
    pub(crate) head: DrumHead,
}

/// Staff position of sounds the map does not know: the middle line.
const UNKNOWN_POSITION: i32 = 4;

/// The place of General MIDI drum sound `midi` on the percussion staff.
pub(crate) fn drum_note(midi: u8) -> DrumNote {
    use DrumHead::{CircleCross, Cross, Diamond, Normal, Triangle};
    let (position, head) = match midi {
        35 => (0, Normal),       // Acoustic bass drum
        36 => (1, Normal),       // Bass drum
        37 => (5, CircleCross),  // Side stick
        38 | 40 => (5, Normal),  // Snare (acoustic, electric)
        39 => (5, Cross),        // Hand clap
        41 => (2, Normal),       // Low floor tom
        42 => (9, Cross),        // Closed hi-hat
        43 => (3, Normal),       // High floor tom
        44 => (-1, Cross),       // Pedal hi-hat
        45 => (6, Normal),       // Low tom
        46 => (9, CircleCross),  // Open hi-hat
        47 => (7, Normal),       // Low-mid tom
        48 => (8, Normal),       // Hi-mid tom
        49 => (10, Cross),       // Crash cymbal 1
        50 => (9, Normal),       // High tom
        51 | 59 => (8, Cross),   // Ride cymbal 1 and 2
        52 => (10, CircleCross), // Chinese cymbal
        53 => (8, Diamond),      // Ride bell
        54 => (9, Triangle),     // Tambourine
        55 => (11, CircleCross), // Splash cymbal
        56 => (7, Triangle),     // Cowbell
        57 => (11, Cross),       // Crash cymbal 2
        _ => (UNKNOWN_POSITION, Diamond),
    };
    DrumNote { position, head }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drum_set_sits_where_drummers_expect_it() {
        assert_eq!(
            drum_note(36),
            DrumNote {
                position: 1,
                head: DrumHead::Normal
            }
        );
        assert_eq!(drum_note(38).position, 5, "snare in the third space");
        assert_eq!(drum_note(42).head, DrumHead::Cross, "hi-hat with a cross");
        assert!(drum_note(42).position > 8, "hi-hat above the staff");
        assert!(drum_note(44).position < 0, "pedal hi-hat below the staff");
        assert_ne!(
            drum_note(42),
            drum_note(46),
            "open and closed hi-hat differ"
        );
        // Toms go up with their pitch.
        let toms = [41, 43, 45, 47, 48, 50].map(|m| drum_note(m).position);
        assert!(toms.windows(2).all(|w| w[0] < w[1]), "{toms:?}");
    }

    #[test]
    fn unknown_sounds_stay_visible() {
        for midi in [0, 34, 82, 127] {
            assert_eq!(
                drum_note(midi),
                DrumNote {
                    position: UNKNOWN_POSITION,
                    head: DrumHead::Diamond
                }
            );
        }
        assert_eq!(DrumHead::Normal.glyph(), None);
        assert_eq!(DrumHead::Cross.glyph(), Some(Glyph::NoteheadX));
    }
}
