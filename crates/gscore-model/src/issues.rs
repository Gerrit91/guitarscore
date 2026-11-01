//! Musical problems that do not prevent editing, such as measures that are not filled.
//!
//! Guitar Pro files can contain these (an incomplete last bar, a bar left half-entered), so
//! they are reported for the user to fix, never rejected.

use crate::duration::Duration;
use crate::position::{MeasureIndex, TrackIndex, VoiceIndex};
use crate::song::Song;

/// A problem at one voice of one measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The track.
    pub track: TrackIndex,
    /// The measure.
    pub measure: MeasureIndex,
    /// The voice.
    pub voice: VoiceIndex,
    /// What is wrong.
    pub kind: IssueKind,
}

/// What is wrong with a voice.
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
}

/// Finds every issue in the song, in track, measure, voice order.
pub(crate) fn find(song: &Song) -> Vec<Issue> {
    let mut issues = Vec::new();
    for track in song.tracks() {
        for measure in track.measures() {
            // Validated on creation; a failure here would only skip the measure.
            let Ok(signature) = measure.time_signature() else {
                continue;
            };
            let expected = signature.measure_length();
            for voice in measure.voices().filter(|v| !v.is_empty()) {
                let Ok(actual) = voice.total_duration() else {
                    continue;
                };
                let kind = if actual < expected {
                    IssueKind::Underfull { expected, actual }
                } else if actual > expected {
                    IssueKind::Overfull { expected, actual }
                } else {
                    continue;
                };
                issues.push(Issue {
                    track: track.index(),
                    measure: measure.index(),
                    voice: voice.index(),
                    kind,
                });
            }
        }
    }
    issues
}
