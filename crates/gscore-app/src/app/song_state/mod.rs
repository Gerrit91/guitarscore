//! GuitarScore's own per-track state, kept in the song's free-text notes ("Notices" in
//! Guitar Pro) under a marked block.
//!
//! The block is the compact JSON of [`SongState`], deflated and base64-encoded, split into
//! short ASCII lines (see [`codec`]):
//!
//! ```text
//! ::GS1:1/2:8f3a1c02:eyJ0cmFja3MiOlt7InRyYWNrIjowLCJjb21wX3NlbmQiOjk2fV19
//! ::GS1:2/2:8f3a1c02:...
//! ```
//!
//! A line is plain ASCII on purpose: GP3-5 store notices in a single-byte code page, so
//! the state survives a round trip through Guitar Pro (which keeps the notes when it
//! saves) and through programs that never heard of it. Base64 also keeps every line under
//! the 255 bytes a GP3-5 notice can hold.
//!
//! The older one-line block `[GuitarScore 1]{json}[/GuitarScore]` is still read and is
//! replaced by the new form on save. Everything here is tolerant of hand-edited and
//! foreign files: a block that does not read completely is dropped as a whole, and a file
//! without one simply has no state. The user's own notes keep their order.

mod codec;

use serde::{Deserialize, Serialize};

use crate::settings::PresetTone;

/// The old block's opening marker; the version follows, then `]` and the JSON.
const LEGACY_OPEN: &str = "[GuitarScore ";
/// The old block's closing marker.
const LEGACY_CLOSE: &str = "[/GuitarScore]";
/// The version of the old block written by earlier versions.
const LEGACY_VERSION: u8 = 1;

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
    let mut notes: Vec<String> = notes.into_iter().filter(|note| !is_ours(note)).collect();
    if let Some(state) = state
        && let Ok(json) = serde_json::to_vec(state)
        && let Some(lines) = codec::encode(&json)
    {
        notes.extend(lines);
    }
    notes
}

/// The user's notes and the state, from a song's notes. A block that cannot be read is
/// dropped (it is ours, not the user's text).
pub(crate) fn split(notes: impl IntoIterator<Item = String>) -> (Vec<String>, Option<SongState>) {
    let mut kept = Vec::new();
    let mut ours = Vec::new();
    let mut legacy = None;
    for note in notes {
        if codec::is_ours(&note) {
            ours.push(note);
        } else if is_legacy(&note) {
            // Two old blocks: the first one wins (the second is dropped either way).
            if legacy.is_none() {
                legacy = Some(note);
            }
        } else {
            kept.push(note);
        }
    }
    // A new block is authoritative when present: if it is broken, the song has no state
    // rather than a stale one from an old block that was left next to it.
    let state = if ours.is_empty() {
        legacy.as_deref().and_then(parse_legacy)
    } else {
        codec::decode(ours.iter().map(String::as_str))
            .and_then(|json| serde_json::from_slice(&json).ok())
    };
    (kept, state)
}

/// Whether `note` is ours: a line of the current block (any version) or an old block.
fn is_ours(note: &str) -> bool {
    codec::is_ours(note) || is_legacy(note)
}

/// Whether `note` is the old one-line block.
fn is_legacy(note: &str) -> bool {
    note.trim_start().starts_with(LEGACY_OPEN) && note.trim_end().ends_with(LEGACY_CLOSE)
}

/// The state in an old block line, if it reads.
fn parse_legacy(note: &str) -> Option<SongState> {
    let note = note.trim();
    let rest = note.strip_prefix(LEGACY_OPEN)?;
    // The version and its `]`, then the JSON up to the closing marker.
    let (version, rest) = rest.split_once(']')?;
    if version.trim().parse::<u8>().ok()? > LEGACY_VERSION {
        return None;
    }
    let json = rest.strip_suffix(LEGACY_CLOSE)?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use crate::chain_settings::{BlockChoice, BlockSetting, ChainSetting, Tone};
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
                        BlockSetting::of(BlockChoice::Compressor),
                        BlockSetting::default(),
                        BlockSetting::default(),
                        BlockSetting::default(),
                        BlockSetting::default(),
                    ],
                },
            },
        )
    }

    /// `count` tracks whose tones carry a distinct, barely compressible IR path, so the
    /// state needs more than one line.
    fn long_state(count: usize) -> SongState {
        let tracks = (0..count)
            .map(|index| {
                let mut tone = tone();
                let mut cabinet = BlockSetting::of(BlockChoice::Cabinet);
                cabinet.file = Some(std::path::PathBuf::from(format!(
                    "/impulse responses/{index}/{:x}{index}/cabinet-{index}.wav",
                    index.wrapping_mul(2_654_435_761)
                )));
                tone.chain.blocks[0] = cabinet;
                TrackState {
                    track: index,
                    comp_send: u8::try_from(index % 128).unwrap_or(0),
                    tone: Some(tone),
                }
            })
            .collect();
        SongState::new(tracks).expect("tracks have state")
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
        assert_eq!(notes[0], "A song by the band");
        assert_eq!(notes[1], "Tune down");
        assert!(notes.len() > 2, "the state is kept");
        for line in &notes[2..] {
            assert!(line.starts_with("::GS1:"), "{line}");
            assert!(line.is_ascii() && line.len() <= 200, "{}", line.len());
        }
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
    fn a_long_state_uses_several_short_ascii_lines_after_the_users_notes() {
        let state = long_state(60);
        let notes = encode(
            vec!["First note".to_owned(), "Second note".to_owned()],
            Some(&state),
        );
        assert_eq!(notes[0], "First note");
        assert_eq!(notes[1], "Second note");
        let block = &notes[2..];
        assert!(block.len() > 1, "{} lines", block.len());
        for line in block {
            assert!(line.starts_with("::GS1:"), "{line}");
            assert!(line.is_ascii(), "base64 keeps the line ASCII");
            assert!(line.len() <= 200, "{} chars", line.len());
        }
        // Every line names the same part count and checksum.
        let header = |line: &str| {
            let rest = line.strip_prefix("::GS1:").unwrap();
            let (part_count, rest) = rest.split_once(':').unwrap();
            let (_part, _count) = part_count.split_once('/').unwrap();
            let (crc, _chunk) = rest.split_once(':').unwrap();
            crc.to_owned()
        };
        let first = header(&block[0]);
        assert!(block.iter().all(|line| header(line) == first));
        let (kept, back) = split(notes);
        assert_eq!(kept.len(), 2);
        assert_eq!(back, Some(state));
    }

    #[test]
    fn a_foreign_or_broken_block_does_not_lose_the_users_notes() {
        // No block at all: every line is the user's.
        let (kept, state) = split(vec!["Just notes".to_owned()]);
        assert_eq!(kept, vec!["Just notes"]);
        assert!(state.is_none());

        let state = SongState::new(vec![TrackState {
            track: 0,
            comp_send: 1,
            tone: None,
        }])
        .unwrap();
        let good = encode([], Some(&state));

        // A block from a future version, a wrong checksum, a broken body and a missing
        // line: dropped as a whole, the user's notes kept.
        let mut cases: Vec<Vec<String>> = Vec::new();
        cases.push(
            good.iter()
                .map(|line| line.replace("::GS1:", "::GS9:"))
                .collect(),
        );
        {
            let mut edited = good.clone();
            let line = edited.first_mut().unwrap();
            let last = line.pop().unwrap();
            line.push(if last == 'A' { 'B' } else { 'A' });
            cases.push(edited);
        }
        // A part number outside the count.
        cases.push(
            good.iter()
                .map(|line| line.replacen(":1/", ":0/", 1))
                .collect(),
        );
        if good.len() > 1 {
            cases.push(good.iter().skip(1).cloned().collect());
        }
        for block in cases {
            let mut notes = vec!["Just notes".to_owned()];
            notes.extend(block);
            let (kept, state) = split(notes);
            assert_eq!(kept, vec!["Just notes"]);
            assert!(state.is_none());
        }

        // A line that only looks like one (no closing marker) is the user's text: kept.
        let (kept, state) = split(vec!["[GuitarScore".to_owned()]);
        assert_eq!(kept, vec!["[GuitarScore"]);
        assert!(state.is_none());
        // A user note that merely starts with `::GS` is ours by convention.
        let (kept, state) = split(vec!["::GS is how we store state".to_owned()]);
        assert!(kept.is_empty());
        assert!(state.is_none());

        // Two blocks would double every part, which is as unreadable as a broken line:
        // no state is applied rather than one of them at random.
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
        assert!(state.is_none());
    }

    #[test]
    fn non_ascii_state_travels_as_ascii_lines() {
        // A tone name with an umlaut would be mangled in a GP3-5 code page; base64 keeps
        // it intact.
        let mut named = tone();
        named.kind = "Grüße & Saiten".to_owned();
        let state = SongState::new(vec![TrackState {
            track: 0,
            comp_send: 0,
            tone: Some(named.clone()),
        }])
        .unwrap();
        let notes = encode(["Notizen für die Band".to_owned()], Some(&state));
        assert_eq!(notes[0], "Notizen für die Band");
        for line in &notes[1..] {
            assert!(line.is_ascii(), "{line}");
        }
        let (kept, back) = split(notes);
        assert_eq!(kept, vec!["Notizen für die Band"]);
        assert_eq!(
            back.and_then(|state| state.tracks.first().cloned())
                .and_then(|track| track.tone),
            Some(named)
        );
    }

    #[test]
    fn the_old_block_is_still_read_and_replaced_on_save() {
        let state = SongState::new(vec![TrackState {
            track: 0,
            comp_send: 96,
            tone: None,
        }])
        .unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let old = format!("{LEGACY_OPEN}{LEGACY_VERSION}]{json}{LEGACY_CLOSE}");
        let notes = vec!["A song by the band".to_owned(), old];
        let (kept, back) = split(notes);
        assert_eq!(kept, vec!["A song by the band"]);
        assert_eq!(back, Some(state.clone()));
        // Saving replaces it with the new form.
        let saved = encode(kept, back.as_ref());
        assert_eq!(saved.len(), 2);
        assert!(saved[1].starts_with("::GS1:"), "{}", saved[1]);
        assert!(!saved.iter().any(|note| is_legacy(note)));
        // An old block from the future is dropped like any unreadable one.
        let future = format!("{LEGACY_OPEN}9]{{}}{LEGACY_CLOSE}");
        let (kept, back) = split(vec!["Just notes".to_owned(), future]);
        assert_eq!(kept, vec!["Just notes"]);
        assert!(back.is_none());
    }
}
