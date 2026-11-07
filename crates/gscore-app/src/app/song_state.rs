//! GuitarScore's own per-track state, kept in the song's free-text notes ("Notices" in
//! Guitar Pro) under a marked block:
//!
//! ```text
//! [GuitarScore 1]{"version":1,"tracks":[{"track":0,"comp_send":96}]}[/GuitarScore]
//! ```
//!
//! Guitar Pro keeps that field when it saves the file again, so a song carries its mixer
//! sends and the tones its tracks play without us inventing a place in the archive (which
//! Guitar Pro would drop). The block is one line, taken out of the notes when a song is
//! opened, so the notes the user sees are theirs alone.
//!
//! Everything here is tolerant of hand-edited and foreign files: a line that does not
//! parse is dropped, and a file without the block simply has no state.

use serde::{Deserialize, Serialize};

use crate::settings::PresetTone;

/// The block's opening marker; the version follows, then `]` and the JSON.
const OPEN: &str = "[GuitarScore ";
/// The block's closing marker.
const CLOSE: &str = "[/GuitarScore]";
/// The version of the state written now.
const VERSION: u8 = 1;

/// What GuitarScore keeps in a song's notes, per track. The block's marker carries the
/// version, so a future one can read this and newer forms apart.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct SongState {
    #[serde(default)]
    pub tracks: Vec<TrackState>,
}

/// One track's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TrackState {
    /// The track in the song.
    pub track: usize,
    /// Its send into the mixer's compressor bus (0-127).
    #[serde(default)]
    pub comp_send: u8,
    /// The tone it plays, when it is not its kind's built-in default.
    #[serde(default)]
    pub tone: Option<PresetTone>,
}

impl SongState {
    /// The state of `track`, if the song has one (for the tests).
    #[cfg(test)]
    fn track(&self, track: usize) -> Option<&TrackState> {
        self.tracks.iter().find(|state| state.track == track)
    }

    /// The state of `tracks`, or `None` when every track is at its defaults (nothing to
    /// keep).
    pub(crate) fn new(tracks: Vec<TrackState>) -> Option<Self> {
        let tracks: Vec<TrackState> = tracks
            .into_iter()
            .filter(|track| track.comp_send != 0 || track.tone.is_some())
            .collect();
        (!tracks.is_empty()).then_some(SongState { tracks })
    }
}

/// The notes a song is saved with: the user's own, and our block when there is state.
/// An older block is replaced, wherever it sits.
pub(crate) fn encode(
    notes: impl IntoIterator<Item = String>,
    state: Option<&SongState>,
) -> Vec<String> {
    let mut notes: Vec<String> = notes.into_iter().filter(|note| !is_block(note)).collect();
    if let Some(state) = state
        && let Ok(json) = serde_json::to_string(state)
    {
        notes.push(format!("{OPEN}{VERSION}]{json}{CLOSE}"));
    }
    notes
}

/// The user's notes and the state, from a song's notes. A block that cannot be read is
/// dropped (it is ours, not the user's text).
pub(crate) fn split(notes: impl IntoIterator<Item = String>) -> (Vec<String>, Option<SongState>) {
    let mut kept = Vec::new();
    let mut state = None;
    for note in notes {
        if is_block(&note) {
            if state.is_none() {
                state = parse(&note);
            }
        } else {
            kept.push(note);
        }
    }
    (kept, state)
}

/// Whether `note` is our block.
fn is_block(note: &str) -> bool {
    note.trim_start().starts_with(OPEN) && note.trim_end().ends_with(CLOSE)
}

/// The state in a block line, if it is one, its version is not from the future and it
/// reads.
fn parse(note: &str) -> Option<SongState> {
    let note = note.trim();
    let rest = note.strip_prefix(OPEN)?;
    // The version and its `]`, then the JSON up to the closing marker.
    let (version, rest) = rest.split_once(']')?;
    if version.trim().parse::<u8>().ok()? > VERSION {
        return None;
    }
    let json = rest.strip_suffix(CLOSE)?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use crate::chain_settings::{BlockSetting, ChainSetting, Tone};
    use crate::settings::StringSound;
    use gscore_play::StringKind;

    fn tone() -> PresetTone {
        PresetTone::of(
            27,
            &Tone {
                kind: StringKind::Steel,
                sound: StringSound::from(gscore_audio::default_string_model(StringKind::Steel)),
                chain: ChainSetting {
                    blocks: [
                        BlockSetting::of(crate::chain_settings::BlockChoice::Compressor),
                        BlockSetting::default(),
                        BlockSetting::default(),
                        BlockSetting::default(),
                        BlockSetting::default(),
                    ],
                },
            },
        )
    }

    #[test]
    fn a_songs_state_round_trips_in_its_notes() {
        let state = SongState::new(vec![
            TrackState {
                track: 0,
                comp_send: 96,
                tone: None,
            },
            TrackState {
                track: 2,
                comp_send: 0,
                tone: Some(tone()),
            },
            // Nothing to keep: dropped.
            TrackState {
                track: 3,
                comp_send: 0,
                tone: None,
            },
        ])
        .expect("two tracks have state");
        let notes = encode(
            vec!["A song by the band".to_owned(), "Tune down".to_owned()],
            Some(&state),
        );
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0], "A song by the band");
        let (kept, back) = split(notes.clone());
        assert_eq!(kept, vec!["A song by the band", "Tune down"]);
        let back = back.expect("the state is read");
        assert_eq!(back, state);
        assert_eq!(back.track(0).map(|track| track.comp_send), Some(96));
        assert_eq!(
            back.track(2).and_then(|track| track.tone.clone()),
            Some(tone())
        );
        // Saving again replaces the block instead of piling up.
        let again = encode(kept, Some(&state));
        assert_eq!(again, notes);
        // Without state the notes are just the user's.
        assert_eq!(encode(again.clone(), None).len(), 2);
    }

    #[test]
    fn a_foreign_or_broken_block_does_not_lose_the_users_notes() {
        // No block at all: every line is the user's.
        let (kept, state) = split(vec!["Just notes".to_owned()]);
        assert_eq!(kept, vec!["Just notes"]);
        assert!(state.is_none());
        // A block from a future version, or one that does not read: dropped, no panic.
        for block in [
            format!("{OPEN}9]{{}} {CLOSE}"),
            format!("{OPEN}1]not json{CLOSE}"),
        ] {
            let (kept, state) = split(vec!["Just notes".to_owned(), block]);
            assert_eq!(kept, vec!["Just notes"]);
            assert!(state.is_none());
        }
        // A line that only looks like one (no closing marker) is the user's text: kept.
        let (kept, state) = split(vec!["[GuitarScore".to_owned()]);
        assert_eq!(kept, vec!["[GuitarScore"]);
        assert!(state.is_none());
        // Two blocks: the first one wins.
        let one = SongState::new(vec![TrackState {
            track: 0,
            comp_send: 1,
            tone: None,
        }])
        .unwrap();
        let two = SongState::new(vec![TrackState {
            track: 0,
            comp_send: 2,
            tone: None,
        }])
        .unwrap();
        let notes = [encode([], Some(&one)), encode([], Some(&two))].concat();
        let (kept, state) = split(notes);
        assert!(kept.is_empty());
        assert_eq!(state, Some(one));
    }
}
