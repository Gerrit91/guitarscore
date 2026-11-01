//! Problems that do not prevent opening or editing a song: measures that are not filled,
//! notes on strings the track lacks, two notes on one string, and similar.
//!
//! Guitar Pro files contain these (an incomplete last bar; drum or piano tracks that the
//! importer maps onto tab strings), so they are reported for the user, never rejected.
//! The notes themselves stay in the song unchanged.

use std::fmt;

use crate::duration::Duration;
use crate::error::ModelError;
use crate::position::{BeatIndex, MeasureIndex, StringNumber, TrackIndex, VoiceIndex};
use crate::song::Song;
use crate::validate::MAX_STRINGS;
use crate::views::{BeatView, VoiceView};

/// A problem at one voice (and possibly one beat) of one measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The track.
    pub track: TrackIndex,
    /// The measure.
    pub measure: MeasureIndex,
    /// The voice.
    pub voice: VoiceIndex,
    /// The beat, for problems with a single beat's notes.
    pub beat: Option<BeatIndex>,
    /// What is wrong.
    pub kind: IssueKind,
}

/// What is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueKind {
    /// The beats are shorter than the time signature asks for.
    Underfull {
        /// Length of a full measure.
        expected: Duration,
        /// Summed length of the beats.
        actual: Duration,
    },
    /// The beats are longer than the time signature allows.
    Overfull {
        /// Length of a full measure.
        expected: Duration,
        /// Summed length of the beats.
        actual: Duration,
    },
    /// A note's string or fret value is out of range (e.g. a negative fret).
    InvalidNote(ModelError),
    /// A note is on a string the track does not have.
    NoteOnMissingString {
        /// The note's string.
        string: StringNumber,
        /// How many strings the track has.
        string_count: usize,
    },
    /// Two or more notes of one beat are on the same string.
    DuplicateString {
        /// The string.
        string: StringNumber,
    },
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}, {}", self.track, self.measure, self.voice)?;
        if let Some(beat) = self.beat {
            write!(f, ", {beat}")?;
        }
        match &self.kind {
            IssueKind::Underfull { expected, actual } => {
                write!(f, ": measure not filled ({actual} of {expected})")
            }
            IssueKind::Overfull { expected, actual } => {
                write!(f, ": measure overfilled ({actual} of {expected})")
            }
            IssueKind::InvalidNote(ModelError::Invalid(msg)) => write!(f, ": {msg}"),
            IssueKind::InvalidNote(err) => write!(f, ": {err}"),
            IssueKind::NoteOnMissingString {
                string,
                string_count,
            } => write!(
                f,
                ": note on {string}, but the track has {string_count} strings"
            ),
            IssueKind::DuplicateString { string } => {
                write!(f, ": several notes on {string}")
            }
        }
    }
}

/// Finds every issue in the song, in track, measure, voice, beat order.
pub(crate) fn find(song: &Song) -> Vec<Issue> {
    let mut issues = Vec::new();
    for track in song.tracks() {
        let string_count = track.string_count();
        for measure in track.measures() {
            for voice in measure.voices() {
                let mut push = |beat: Option<BeatIndex>, kind: IssueKind| {
                    issues.push(Issue {
                        track: track.index(),
                        measure: measure.index(),
                        voice: voice.index(),
                        beat,
                        kind,
                    });
                };
                // Validated on creation; an error here would only skip the check.
                if let Ok(signature) = measure.time_signature()
                    && let Some(kind) = fill_issue(&voice, signature.measure_length())
                {
                    push(None, kind);
                }
                for beat in voice.beats() {
                    for kind in note_issues(&beat, string_count) {
                        push(Some(beat.index()), kind);
                    }
                }
            }
        }
    }
    issues
}

/// Under- or overfilled voice; empty voices are fine.
fn fill_issue(voice: &VoiceView<'_>, expected: Duration) -> Option<IssueKind> {
    if voice.is_empty() {
        return None;
    }
    let actual = voice.total_duration().ok()?;
    if actual < expected {
        Some(IssueKind::Underfull { expected, actual })
    } else if actual > expected {
        Some(IssueKind::Overfull { expected, actual })
    } else {
        None
    }
}

/// Problems with the notes of one beat. Each string is reported at most once.
fn note_issues(beat: &BeatView<'_>, string_count: usize) -> Vec<IssueKind> {
    let mut kinds = Vec::new();
    // Number of notes seen per string; index 0 is string 1.
    let mut seen = [0_u8; MAX_STRINGS];
    for note in beat.notes() {
        if let Err(err) = note.fret() {
            kinds.push(IssueKind::InvalidNote(err));
        }
        let string = match note.string() {
            Ok(string) => string,
            Err(err) => {
                kinds.push(IssueKind::InvalidNote(err));
                continue;
            }
        };
        let number = usize::from(string.get());
        if number > string_count {
            kinds.push(IssueKind::NoteOnMissingString {
                string,
                string_count,
            });
            continue;
        }
        if let Some(count) = seen.get_mut(number.saturating_sub(1)) {
            *count = count.saturating_add(1);
            if *count == 2 {
                kinds.push(IssueKind::DuplicateString { string });
            }
        }
    }
    kinds
}
