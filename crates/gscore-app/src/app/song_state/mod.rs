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

use gscore_model::{Song, TrackIndex};
use gscore_play::{StringKind, TrackEngine};
use serde::{Deserialize, Serialize};

use crate::chain_settings::{ChainSetting, DrumPart};
use crate::playback::{MidiOutRoute, TrackDrumPart};
use crate::settings::{ChorusBus, CompressorBus, PresetTone, ReverbBus};

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
    /// The song's own mixer buses. A bus the song does not set follows the program's
    /// settings; editing it in the mixer gives the song its own copy.
    #[serde(default, skip_serializing_if = "Buses::is_default")]
    pub buses: Buses,
}

/// The mixer's song-wide buses: the shared reverb and the parallel compressor. Each is
/// `None` when the song follows the program's settings for it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Buses {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverb: Option<ReverbBus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressor: Option<CompressorBus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chorus: Option<ChorusBus>,
    /// The master's effect chain (after the tracks and buses), when the song has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master: Option<ChainSetting>,
}

impl Buses {
    /// The buses the document keeps for `song` (a payload that does not read counts as
    /// none, so a damaged file never stops a song from opening).
    pub(crate) fn of_song(song: &Song) -> Self {
        song.sound_state()
            .and_then(|json| serde_json::from_str::<Buses>(json).ok())
            .map(Buses::sanitized)
            .unwrap_or_default()
    }

    /// A copy with values the rest of the program can rely on (a broken file may hold
    /// anything).
    fn sanitized(mut self) -> Self {
        self.reverb = self.reverb.map(ReverbBus::sanitized);
        self.compressor = self.compressor.map(CompressorBus::sanitized);
        self.chorus = self.chorus.map(ChorusBus::sanitized);
        self
    }

    /// The JSON the document keeps for these buses, or `None` when neither is the song's.
    pub(crate) fn json(&self) -> Option<String> {
        if *self == Buses::default() {
            return None;
        }
        serde_json::to_string(self).ok()
    }

    /// Whether the song follows the program's settings for both buses (nothing to keep).
    pub(crate) fn is_default(&self) -> bool {
        *self == Buses::default()
    }
}

/// One track's state in the block: its position and identity in the song, and the sound
/// settings GuitarScore adds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TrackState {
    /// The track in the song.
    pub track: usize,
    /// How the track was recognised when the block was written. `None` in a block from a
    /// version before this existed: then only the number can be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<TrackIdentity>,
    /// Its sound settings, flat in the entry (an old block stored `comp_send` and `tone`
    /// at this level, which is where they still read from).
    #[serde(flatten)]
    pub sound: TrackSound,
}

/// Everything GuitarScore knows about a track's sound that the Guitar Pro format has no
/// field for: what plays it, its tone and chain, its compressor send, the MIDI-out route.
///
/// The same JSON is kept on the track in the document (so track edits carry it) and in the
/// song's notes block (so it travels with the file).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TrackSound {
    /// What plays it, when it is not the default for its preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<SoundEngine>,
    /// The hardware synthesizer's port and channel, for a track played by MIDI out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_out: Option<MidiOutRoute>,
    /// Its send into the mixer's compressor bus (0-127).
    #[serde(skip_serializing_if = "is_zero")]
    pub comp_send: u8,
    /// The tone it plays, when it is not the built-in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<PresetTone>,
    /// The parts of a drum kit the song gives a tone or mixing level of their own; a part
    /// missing here plays this machine's kit setting (or the built-in tone) at unity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drum_parts: Vec<DrumPartSound>,
    /// What the track sounds like relative to what is written, in semitones (0 when it is
    /// not transposed). It changes playback only: the notes, the tablature and the score
    /// stay as they are.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub transpose: i32,
}

/// One drum part's own tone in a song: its chain and its mixing level in the drum track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DrumPartSound {
    /// The part's id (`kick`, `snare`, `hi_hat`, `toms`, `cymbals`, `percussion`).
    pub part: String,
    /// Its blocks and knobs.
    #[serde(default)]
    pub chain: ChainSetting,
    /// Its mixing level (0-127, 127 unity).
    #[serde(default = "unity_level", skip_serializing_if = "is_unity")]
    pub level: u8,
    /// Its pan inside the kit (0 left, 64 centre, 127 right).
    #[serde(default = "centre_pan", skip_serializing_if = "is_centre")]
    pub pan: u8,
    /// Whether the part is muted inside the kit.
    #[serde(default, skip_serializing_if = "is_false")]
    pub muted: bool,
    /// Whether the part is soloed inside the kit.
    #[serde(default, skip_serializing_if = "is_false")]
    pub soloed: bool,
}

/// `true` for `false`, so a default flag is not written.
fn is_false(value: &bool) -> bool {
    !*value
}

/// The level of a part the song's payload leaves out.
fn unity_level() -> u8 {
    127
}

/// The pan of a part the song's payload leaves out: the middle.
fn centre_pan() -> u8 {
    64
}

/// `true` for the middle, so a default pan is not written.
fn is_centre(pan: &u8) -> bool {
    *pan == 64
}

/// `true` for the unity level, so a default number is not written.
fn is_unity(level: &u8) -> bool {
    *level == 127
}

impl DrumPartSound {
    /// The state of `part` as playback has it: its chain, level, pan, mute and solo.
    pub(crate) fn of(part: &TrackDrumPart) -> Self {
        DrumPartSound {
            part: part.part.id().to_owned(),
            chain: part.chain.clone(),
            level: part.level.min(127),
            pan: part.pan.min(127),
            muted: part.muted,
            soloed: part.soloed,
        }
    }

    /// The part it is for (an unknown id from a newer version is ignored).
    pub(crate) fn part(&self) -> Option<DrumPart> {
        DrumPart::from_id(&self.part)
    }

    /// The part as playback takes it.
    pub(crate) fn track_part(&self) -> Option<TrackDrumPart> {
        Some(TrackDrumPart {
            part: self.part()?,
            chain: self.chain.clone(),
            level: self.level.min(127),
            pan: self.pan.min(127),
            muted: self.muted,
            soloed: self.soloed,
        })
    }
}

/// `true` for 0, so a default number is not written.
/// Serde helper: `true` for 0, so a default setting is left out of the JSON.
fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

/// What plays a track, as the block writes it: a plain word, not an enum's variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SoundEngine {
    /// The General MIDI path (a SoundFont or a MIDI device).
    SoundFont,
    /// Modelled nylon-string guitar.
    Nylon,
    /// Modelled steel-string guitar.
    Steel,
    /// Modelled electric guitar.
    Electric,
    /// Modelled bass.
    Bass,
    /// A hardware synthesizer on a MIDI output port.
    MidiOut,
}

impl SoundEngine {
    /// The block's word for `engine`.
    pub(crate) fn of(engine: TrackEngine) -> Self {
        match engine {
            TrackEngine::SoundFont => SoundEngine::SoundFont,
            TrackEngine::MidiOut => SoundEngine::MidiOut,
            TrackEngine::Modelled(StringKind::Nylon) => SoundEngine::Nylon,
            TrackEngine::Modelled(StringKind::Steel) => SoundEngine::Steel,
            TrackEngine::Modelled(StringKind::Electric) => SoundEngine::Electric,
            TrackEngine::Modelled(StringKind::Bass) => SoundEngine::Bass,
        }
    }

    /// The playback engine this word means.
    pub(crate) fn engine(self) -> TrackEngine {
        match self {
            SoundEngine::SoundFont => TrackEngine::SoundFont,
            SoundEngine::MidiOut => TrackEngine::MidiOut,
            SoundEngine::Nylon => TrackEngine::Modelled(StringKind::Nylon),
            SoundEngine::Steel => TrackEngine::Modelled(StringKind::Steel),
            SoundEngine::Electric => TrackEngine::Modelled(StringKind::Electric),
            SoundEngine::Bass => TrackEngine::Modelled(StringKind::Bass),
        }
    }
}

impl TrackSound {
    /// One track's state, from the document (a payload that does not read counts as none,
    /// so a damaged file never stops a song from opening).
    pub(crate) fn of_song(song: &Song, track: usize) -> Self {
        let mut state = song
            .track(TrackIndex::new(track))
            .and_then(|view| view.sound_state())
            .and_then(|json| serde_json::from_str::<TrackSound>(json).ok())
            .map(TrackSound::sanitized)
            .unwrap_or_default();
        // The transposition itself lives in the document (playback reads it there); the
        // payload keeps it so it travels with the file.
        state.transpose = song
            .track(TrackIndex::new(track))
            .map_or(0, |view| view.transposition());
        state
    }

    /// A copy with values the rest of the program can rely on (a broken file may hold
    /// anything).
    fn sanitized(mut self) -> Self {
        self.comp_send = self.comp_send.min(127);
        self.midi_out = self.midi_out.map(MidiOutRoute::sanitized);
        self.transpose = self
            .transpose
            .clamp(-gscore_model::MAX_TRANSPOSE, gscore_model::MAX_TRANSPOSE);
        for part in &mut self.drum_parts {
            part.level = part.level.min(127);
        }
        self
    }

    /// The JSON the document keeps for this state, or `None` when it is all defaults.
    pub(crate) fn json(&self) -> Option<String> {
        if *self == TrackSound::default() {
            return None;
        }
        serde_json::to_string(self).ok()
    }

    /// Whether every setting is at its default (nothing to keep).
    pub(crate) fn is_default(&self) -> bool {
        *self == TrackSound::default()
    }
}

/// How the block recognises a track after someone reordered or deleted tracks in another
/// program and kept the notes: its name and instrument.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct TrackIdentity {
    /// The track's name.
    #[serde(default)]
    pub name: String,
    /// Its General MIDI program (0-127).
    #[serde(default)]
    pub program: u8,
    /// Whether it is a drum kit.
    #[serde(default)]
    pub drums: bool,
}

impl TrackIdentity {
    /// The identity of track `track` of `song`, or `None` if the song has no such track.
    pub(crate) fn of(song: &Song, track: usize) -> Option<Self> {
        let index = TrackIndex::new(track);
        let view = song.track(index)?;
        let sound = song.track_sound(index)?;
        Some(TrackIdentity {
            name: view.name().to_owned(),
            program: sound.program,
            drums: sound.percussion,
        })
    }

    /// Whether `track` (a track of the song as it is now) is the one this identity was
    /// written for. Names are also compared as GP3-5 keep them: a save to those formats
    /// cuts a long name and replaces characters their code page lacks, and the track is
    /// still the same.
    fn fits(&self, track: &TrackIdentity) -> bool {
        self.program == track.program
            && self.drums == track.drums
            && (self.name == track.name
                || gscore_model::track_name_in_binary_formats(&self.name)
                    == gscore_model::track_name_in_binary_formats(&track.name))
    }
}

impl SongState {
    /// The state of `track`, if the song has one (for the tests).
    #[cfg(test)]
    fn track(&self, track: usize) -> Option<&TrackState> {
        self.tracks.iter().find(|state| state.track == track)
    }

    /// The state of `tracks` and `buses`, or `None` when nothing needs keeping.
    pub(crate) fn new(tracks: Vec<TrackState>, buses: Buses) -> Option<Self> {
        let tracks: Vec<TrackState> = tracks
            .into_iter()
            .filter(|track| !track.sound.is_default())
            .collect();
        (!tracks.is_empty() || !buses.is_default()).then_some(SongState { tracks, buses })
    }

    /// Writes the state into `song`: each track's entry on the track it belongs to (see
    /// [`Self::resolve`]) and the buses on the song. Called once when a song opens, before
    /// it can be edited.
    pub(crate) fn apply_to_song(&self, song: &mut Song) {
        let resolved: Vec<(usize, Option<String>)> = self
            .resolve(song)
            .into_iter()
            .map(|(track, state)| (track, state.sound.json()))
            .collect();
        for (track, state) in resolved {
            let _ = song.set_track_sound_state(TrackIndex::new(track), state);
        }
        // The transposition travels in the block but is applied to the document, where
        // playback reads it.
        for (track, state) in self.resolve(song) {
            if state.sound.transpose != 0 {
                let at = TrackIndex::new(track);
                let _ = song.transpose_track_to(at, state.sound.transpose);
            }
        }
        if let Some(json) = self.buses.json() {
            let _ = song.set_sound_state(Some(json));
        }
    }

    /// The entries with the track of `song` each belongs to, in the block's order.
    ///
    /// An entry goes to the track with its number when that is still the same track;
    /// otherwise to the one remaining track with the same name and instrument; otherwise
    /// it is dropped rather than applied to the wrong track. A block written before
    /// identities existed can only rely on the number.
    pub(crate) fn resolve<'a>(&'a self, song: &Song) -> Vec<(usize, &'a TrackState)> {
        // Every track of the song, so a missing one can never shift the indices.
        let tracks: Vec<TrackIdentity> = (0..song.track_count())
            .map(|track| TrackIdentity::of(song, track).unwrap_or_default())
            .collect();
        self.resolve_in(&tracks)
    }

    /// [`Self::resolve`] with the tracks given as identities (what the matching sees).
    fn resolve_in<'a>(&'a self, tracks: &[TrackIdentity]) -> Vec<(usize, &'a TrackState)> {
        let mut taken = vec![false; tracks.len()];
        let mut matched: Vec<Option<usize>> = vec![None; self.tracks.len()];

        // First by number, so two reordered tracks cannot take each other's place.
        for (entry_index, state) in self.tracks.iter().enumerate() {
            let fits = match &state.identity {
                Some(identity) => tracks
                    .get(state.track)
                    .is_some_and(|track| identity.fits(track)),
                None => tracks.get(state.track).is_some(),
            };
            if !fits || taken.get(state.track).copied().unwrap_or(true) {
                continue;
            }
            if let Some(slot) = taken.get_mut(state.track) {
                *slot = true;
            }
            if let Some(slot) = matched.get_mut(entry_index) {
                *slot = Some(state.track);
            }
        }

        // Then by the one remaining track with the same name and instrument.
        for (entry_index, state) in self.tracks.iter().enumerate() {
            if matched.get(entry_index).is_some_and(Option::is_some) {
                continue;
            }
            let Some(identity) = state.identity.as_ref() else {
                continue;
            };
            let mut found = None;
            let mut found_count = 0_usize;
            for (track, candidate) in tracks.iter().enumerate() {
                if taken.get(track).copied().unwrap_or(true) || !identity.fits(candidate) {
                    continue;
                }
                found_count = found_count.saturating_add(1);
                found = Some(track);
            }
            // None or several: dropping the entry beats applying it to the wrong track.
            if found_count != 1 {
                continue;
            }
            if let Some(track) = found {
                if let Some(slot) = taken.get_mut(track) {
                    *slot = true;
                }
                if let Some(slot) = matched.get_mut(entry_index) {
                    *slot = Some(track);
                }
            }
        }

        matched
            .into_iter()
            .zip(self.tracks.iter())
            .filter_map(|(track, state)| track.map(|track| (track, state)))
            .collect()
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
    use crate::chain_settings::{
        BlockChoice, BlockSetting, ChainSetting, DrumPart, Tone, default_drum_part_chain,
    };
    use crate::settings::StringSound;
    use gscore_play::StringKind;

    /// The state of `tracks`, with no song-wide buses (most tests).
    fn state_of(tracks: Vec<TrackState>) -> Option<SongState> {
        SongState::new(tracks, Buses::default())
    }

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
                    output_db: 0.0,
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
                    identity: None,
                    sound: TrackSound {
                        comp_send: u8::try_from(index % 128).unwrap_or(0),
                        tone: Some(tone),
                        ..TrackSound::default()
                    },
                }
            })
            .collect();
        SongState::new(tracks, Buses::default()).expect("tracks have state")
    }

    #[test]
    fn a_songs_state_round_trips_in_its_notes() {
        let state = state_of(vec![
            TrackState {
                track: 0,
                identity: None,
                sound: TrackSound {
                    comp_send: 96,
                    tone: None,
                    ..TrackSound::default()
                },
            },
            TrackState {
                track: 2,
                identity: None,
                sound: TrackSound {
                    comp_send: 0,
                    tone: Some(tone()),
                    ..TrackSound::default()
                },
            },
            // Nothing to keep: dropped.
            TrackState {
                track: 3,
                identity: None,
                sound: TrackSound {
                    comp_send: 0,
                    tone: None,
                    ..TrackSound::default()
                },
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
        assert_eq!(back.track(0).map(|track| track.sound.comp_send), Some(96));
        assert_eq!(
            back.track(2).and_then(|track| track.sound.tone.clone()),
            Some(tone())
        );
        // Saving again replaces the block instead of piling up.
        let again = encode(kept, Some(&state));
        assert_eq!(again, notes);
        // Without state the notes are just the user's.
        assert_eq!(encode(again.clone(), None).len(), 2);
    }

    #[test]
    fn the_songs_buses_travel_in_the_block() {
        let reverb = ReverbBus {
            level: 96,
            muted: true,
            ..ReverbBus::default()
        }
        .sanitized();
        let compressor = CompressorBus {
            level: 64,
            muted: true,
            ..CompressorBus::default()
        }
        .sanitized();
        let mut master = ChainSetting::default();
        master.blocks[0] = BlockSetting::of(BlockChoice::Eq);
        let buses = Buses {
            reverb: Some(reverb.clone()),
            compressor: Some(compressor.clone()),
            chorus: Some(ChorusBus::default()),
            master: Some(master),
        };
        let state = SongState::new(Vec::new(), buses.clone()).expect("the buses are kept");
        let notes = encode([], Some(&state));
        assert!(!notes.is_empty());
        let (kept, back) = split(notes);
        assert!(kept.is_empty());
        let back = back.expect("the state is read");
        assert_eq!(back.buses, buses);

        // Only the bus the song sets is kept.
        let only = Buses {
            reverb: Some(reverb),
            compressor: None,
            chorus: None,
            master: None,
        };
        let state = SongState::new(Vec::new(), only.clone()).expect("the reverb is kept");
        assert_eq!(state.buses, only);

        // Applying the state gives the song its own buses (the model's payload).
        let mut song = Song::default();
        assert_eq!(Buses::of_song(&song), Buses::default());
        state.apply_to_song(&mut song);
        assert_eq!(Buses::of_song(&song), only);

        // Buses alone are enough for a state; when everything is default there is none.
        assert!(SongState::new(Vec::new(), Buses::default()).is_none());
    }

    #[test]
    fn a_tracks_drum_parts_travel_in_the_block() {
        let mut chain = default_drum_part_chain(DrumPart::Kick);
        if let Some(block) = chain.blocks.first_mut() {
            *block = BlockSetting::of(BlockChoice::Eq);
            block.low_cut = 45.0;
        }
        let sound = TrackSound {
            drum_parts: vec![
                DrumPartSound::of(&TrackDrumPart {
                    part: DrumPart::Kick,
                    chain: chain.clone(),
                    level: 96,
                    pan: 30,
                    muted: false,
                    soloed: false,
                }),
                DrumPartSound::of(&TrackDrumPart {
                    part: DrumPart::Snare,
                    chain: default_drum_part_chain(DrumPart::Snare),
                    level: 127,
                    pan: 64,
                    muted: true,
                    soloed: false,
                }),
            ],
            ..TrackSound::default()
        };
        let state = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound,
        }])
        .expect("the drum parts are state");
        let notes = encode([], Some(&state));
        let (kept, back) = split(notes);
        assert!(kept.is_empty());
        let back = back.expect("the state is read");
        let part = back
            .track(0)
            .and_then(|track| track.sound.drum_parts.first())
            .cloned()
            .expect("the kick");
        assert_eq!(part.part(), Some(DrumPart::Kick));
        assert_eq!((part.level, part.pan), (96, 30));
        assert_eq!(part.chain, chain);
        // A part at the unity level in the middle writes no level or pan, and comes back
        // at unity in the middle (older blocks have no pan either).
        let text = serde_json::to_string(&state.tracks).unwrap_or_default();
        assert_eq!(text.matches("\"pan\"").count(), 1, "{text}");
        let snare = back
            .track(0)
            .and_then(|track| track.sound.drum_parts.get(1))
            .cloned()
            .expect("the snare");
        assert_eq!((snare.level, snare.pan), (127, 64));
        assert_eq!(back, state);
    }

    #[test]
    fn the_block_survives_every_format_we_write() {
        use gscore_io::{FileFormat, export_to_bytes, import_guitar_pro_from_bytes};

        // A state long enough for several lines, with a non-ASCII tone name, and the
        // user's own (ASCII) notes around it.
        let song =
            gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], 1).expect("a song");
        let mut state = long_state(60);
        if let Some(track) = state.tracks.first_mut()
            && let Some(tone) = &mut track.sound.tone
        {
            tone.kind = "Grüße & Saiten".to_owned();
        }
        let user = vec!["A song by the band".to_owned(), "Tune down".to_owned()];
        let song = song.with_notes(encode(user.clone(), Some(&state)));
        for format in [
            FileFormat::Gp,
            FileFormat::Gpx,
            FileFormat::Gp5,
            FileFormat::Gp4,
            FileFormat::Gp3,
        ] {
            let bytes = export_to_bytes(&song, format).expect("the song is written");
            let reread = import_guitar_pro_from_bytes(&bytes)
                .expect("the song is read back")
                .song;
            let (kept, back) = split(reread.notes().map(str::to_owned));
            assert_eq!(kept, user, "{format:?} lost the user's notes");
            assert_eq!(back, Some(state.clone()), "{format:?} lost the block");
        }
    }

    #[test]
    fn a_track_keeps_its_state_when_an_older_format_cuts_or_recodes_its_name() {
        use gscore_io::{FileFormat, export_to_bytes, import_guitar_pro_from_bytes};

        // GP3-5 cut a name to 40 bytes and keep only what Windows-1252 can hold; the
        // entry must still find its track after such a save.
        for name in [
            "Lead guitar, verse and chorus, double-tracked",
            "Гитара",
            "Gitarre ä",
        ] {
            let mut song =
                gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], 1).expect("song");
            let _ = song
                .rename_track(TrackIndex::new(0), name)
                .expect("renamed");
            let state = SongState::new(
                vec![TrackState {
                    track: 0,
                    identity: TrackIdentity::of(&song, 0),
                    sound: TrackSound {
                        comp_send: 90,
                        ..TrackSound::default()
                    },
                }],
                Buses::default(),
            )
            .expect("a state");
            let song = song.with_notes(encode(Vec::new(), Some(&state)));
            for format in [
                FileFormat::Gp,
                FileFormat::Gpx,
                FileFormat::Gp5,
                FileFormat::Gp4,
                FileFormat::Gp3,
            ] {
                let bytes = export_to_bytes(&song, format).expect("written");
                let reread = import_guitar_pro_from_bytes(&bytes).expect("read").song;
                let (_, back) = split(reread.notes().map(str::to_owned));
                let back = back.expect("the block reads");
                let resolved: Vec<usize> = back
                    .resolve(&reread)
                    .into_iter()
                    .map(|(track, _)| track)
                    .collect();
                assert_eq!(resolved, vec![0], "{name:?} through {format:?}");
            }
        }
        // A different name still does not fit, however it is stored.
        let written = identity("Lead guitar, verse and chorus, double-tracked", 27, false);
        assert!(!written.fits(&identity("Rhythm", 27, false)));
        assert!(written.fits(&identity(
            "Lead guitar, verse and chorus, double-tr",
            27,
            false
        )));
        assert!(!written.fits(&identity(
            "Lead guitar, verse and chorus, double-tr",
            33,
            false
        )));
    }

    #[test]
    fn a_block_from_before_the_sound_record_still_reads() {
        // The first form of the compact block kept `comp_send` and `tone` at the entry's
        // own level; the flat sound record still reads them there.
        let json = format!(
            r#"{{"tracks":[{{"track":1,"comp_send":42,"tone":{}}}]}}"#,
            serde_json::to_string(&tone()).expect("a tone")
        );
        let notes = codec::encode(json.as_bytes()).expect("a block");
        let (kept, state) = split(notes);
        assert!(kept.is_empty());
        let state = state.expect("the old form is read");
        let track = state.track(1).expect("the track");
        assert_eq!(track.sound.comp_send, 42);
        assert_eq!(track.sound.tone, Some(tone()));
        assert!(track.identity.is_none());
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

        let state = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound: TrackSound {
                comp_send: 1,
                tone: None,
                ..TrackSound::default()
            },
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
        let one = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound: TrackSound {
                comp_send: 1,
                tone: None,
                ..TrackSound::default()
            },
        }])
        .unwrap();
        let two = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound: TrackSound {
                comp_send: 2,
                tone: None,
                ..TrackSound::default()
            },
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
        let state = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound: TrackSound {
                comp_send: 0,
                tone: Some(named.clone()),
                ..TrackSound::default()
            },
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
                .and_then(|track| track.sound.tone),
            Some(named)
        );
    }

    #[test]
    fn the_old_block_is_still_read_and_replaced_on_save() {
        let state = state_of(vec![TrackState {
            track: 0,
            identity: None,
            sound: TrackSound {
                comp_send: 96,
                tone: None,
                ..TrackSound::default()
            },
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

    /// A track as the matching sees it: name, GM program, drum or not.
    fn identity(name: &str, program: u8, drums: bool) -> TrackIdentity {
        TrackIdentity {
            name: name.to_owned(),
            program,
            drums,
        }
    }

    fn entry(track: usize, comp_send: u8, identity: Option<TrackIdentity>) -> TrackState {
        TrackState {
            track,
            identity,
            sound: TrackSound {
                comp_send,
                ..TrackSound::default()
            },
        }
    }

    /// The track each entry went to.
    fn resolved(state: &SongState, tracks: &[TrackIdentity]) -> Vec<(usize, u8)> {
        state
            .resolve_in(tracks)
            .into_iter()
            .map(|(track, state)| (track, state.sound.comp_send))
            .collect()
    }

    #[test]
    fn entries_follow_their_track_when_the_order_changed() {
        let guitar = identity("Guitar", 25, false);
        let bass = identity("Bass", 33, false);
        let state = state_of(vec![
            entry(0, 10, Some(guitar.clone())),
            entry(1, 20, Some(bass.clone())),
        ])
        .unwrap();
        // Unchanged: by number.
        assert_eq!(
            resolved(&state, &[guitar.clone(), bass.clone()]),
            [(0, 10), (1, 20)]
        );

        // The guitar moved down: each keeps its own settings.
        assert_eq!(
            resolved(&state, &[bass.clone(), guitar.clone()]),
            [(1, 10), (0, 20)]
        );

        // A track inserted in between shifts both.
        assert_eq!(
            resolved(
                &state,
                &[identity("Drums", 0, true), guitar.clone(), bass.clone()]
            ),
            [(1, 10), (2, 20)]
        );
    }

    #[test]
    fn an_entry_dropped_or_changed_in_guitar_pro_is_not_applied_to_the_wrong_track() {
        let guitar = identity("Guitar", 25, false);

        // The guitar was deleted and the piano moved into its place: the entry is dropped.
        let state = state_of(vec![entry(0, 10, Some(guitar.clone()))]).unwrap();
        assert!(resolved(&state, &[identity("Piano", 0, false)]).is_empty());

        // The instrument changed after we saved (same name, now a piano): a guitar tone
        // must not land on a piano.
        assert!(resolved(&state, &[identity("Guitar", 0, false)]).is_empty());
        assert!(resolved(&state, &[identity("Guitar", 25, true)]).is_empty());

        // The number's track is a different one, but the guitar is still there under
        // another number: it is found by name and instrument.
        let tracks = [identity("Piano", 0, false), guitar.clone()];
        assert_eq!(resolved(&state, &tracks), [(1, 10)]);

        // Two tracks named like it and the number no longer pointing at either: no way
        // to tell, so the entry is dropped. (When the number still fits, it wins.)
        let moved = state_of(vec![entry(5, 10, Some(guitar.clone()))]).unwrap();
        let tracks = [guitar.clone(), guitar.clone()];
        assert!(resolved(&moved, &tracks).is_empty());
        assert_eq!(
            resolved(&state, &[guitar.clone(), guitar]),
            [(0, 10)],
            "the number still points at a track with that identity"
        );
    }

    #[test]
    fn a_block_from_before_identities_keeps_going_by_number() {
        let state = state_of(vec![entry(1, 40, None)]).unwrap();
        // Same number: applied as before.
        assert_eq!(
            resolved(&state, &[identity("A", 0, false), identity("B", 30, false)]),
            [(1, 40)]
        );
        // Gone: dropped (there is nothing to match it by).
        assert!(resolved(&state, &[identity("A", 0, false)]).is_empty());
    }

    #[test]
    fn two_entries_never_land_on_the_same_track() {
        // A hand-edited block with the same number twice: the first one wins.
        let guitar = identity("Guitar", 25, false);
        let state = state_of(vec![
            entry(0, 10, Some(guitar.clone())),
            entry(0, 20, Some(guitar.clone())),
        ])
        .unwrap();
        assert_eq!(resolved(&state, &[guitar]), [(0, 10)]);
    }

    #[test]
    fn an_identity_survives_the_block() {
        let state = state_of(vec![entry(
            0,
            5,
            Some(TrackIdentity {
                name: "Grüße & Saiten".to_owned(),
                program: 30,
                drums: false,
            }),
        )])
        .unwrap();
        let notes = encode([], Some(&state));
        let (_, back) = split(notes);
        assert_eq!(back, Some(state));
    }
}
