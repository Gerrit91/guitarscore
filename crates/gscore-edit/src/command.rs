//! The command enum: what an edit is, whether it can run, and how it applies.

use gscore_model::{
    BeatEffect, BeatKind, BeatPosition, BeatView, Change, Fret, MAX_SECTION_TITLE_CHARS, MAX_TEMPO,
    MeasureIndex, ModelError, NoteEffect, NoteValue, Pitch, Song, SongTextField, StringNumber,
    TimeSignature, TrackColor, TrackIndex, TrackStaves,
};

use crate::info::{
    BATCH, CLEAR_NOTE, CLEAR_SECTION, CommandInfo, DELETE_BEAT, DELETE_MEASURE, DELETE_TRACK,
    DUPLICATE_TRACK, INSERT_BEAT, INSERT_MEASURE, INSERT_TRACK, MOVE_TRACK, REDO, RENAME_TRACK,
    SET_BEAT_EFFECT, SET_BEAT_REST, SET_DEAD_NOTE, SET_DURATION, SET_FRET, SET_KEY_SIGNATURE,
    SET_MASTER_PAN, SET_MASTER_VOLUME, SET_MEASURE_TEMPO, SET_NOTE_EFFECT, SET_SECTION,
    SET_SONG_TEXT, SET_TEMPO, SET_TIE, SET_TIME_SIGNATURE, SET_TRACK_CAPO, SET_TRACK_CHORUS,
    SET_TRACK_COLOR, SET_TRACK_DRUMS, SET_TRACK_MUTE, SET_TRACK_PAN, SET_TRACK_PROGRAM,
    SET_TRACK_REVERB, SET_TRACK_SOLO, SET_TRACK_STAVES, SET_TRACK_STRING_PITCH, SET_TRACK_STRINGS,
    SET_TRACK_VOLUME, UNDO,
};

/// One edit, with every parameter already checked for syntax and range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Put a note on a string of one beat.
    SetFret {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
        /// The fret.
        fret: Fret,
    },
    /// Remove the note on one string of a beat.
    ClearNote {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
    },
    /// Set a beat's note value, dots and tuplet.
    SetDuration {
        /// The beat.
        at: BeatPosition,
        /// The new length.
        duration: NoteValue,
    },
    /// Insert a rest beat before `at.beat`.
    InsertBeat {
        /// Where to insert.
        at: BeatPosition,
        /// The rest's length.
        duration: NoteValue,
    },
    /// Remove the beat at `at.beat`.
    DeleteBeat {
        /// The beat.
        at: BeatPosition,
    },
    /// Insert a rest-filled measure in every track before `measure`.
    InsertMeasure {
        /// Where to insert.
        measure: MeasureIndex,
    },
    /// Remove the measure in every track at `measure`.
    DeleteMeasure {
        /// The measure.
        measure: MeasureIndex,
    },
    /// Insert a new, rest-filled track before `at` (one past the last track appends).
    InsertTrack {
        /// Where to insert.
        at: TrackIndex,
    },
    /// Remove the track at `track`.
    DeleteTrack {
        /// The track.
        track: TrackIndex,
    },
    /// Copy the track at `track` and insert the copy after it.
    DuplicateTrack {
        /// The track.
        track: TrackIndex,
    },
    /// Move the track at `from` to index `to` in the resulting order.
    MoveTrack {
        /// The track to move.
        from: TrackIndex,
        /// Where it ends up.
        to: TrackIndex,
    },
    /// Rename the track at `track`.
    RenameTrack {
        /// The track.
        track: TrackIndex,
        /// The new name.
        name: String,
    },
    /// Set the track's color.
    SetTrackColor {
        /// The track.
        track: TrackIndex,
        /// The new color.
        color: TrackColor,
    },
    /// Set the track's volume (0-127).
    SetTrackVolume {
        /// The track.
        track: TrackIndex,
        /// The volume.
        volume: u8,
    },
    /// Choose which staves the track's page shows.
    SetTrackStaves {
        /// The track.
        track: TrackIndex,
        /// Notation and tab, tab only, or notation only.
        staves: TrackStaves,
    },
    /// Set the master volume (0-127; 97, Guitar Pro's default, plays every track at its own
    /// volume).
    SetMasterVolume {
        /// The volume.
        volume: u8,
    },
    /// Set the master pan (0 left, 64 leaves every track's pan, 127 right).
    SetMasterPan {
        /// The pan.
        pan: u8,
    },
    /// Set the track's pan (0 left, 64 middle, 127 right).
    SetTrackPan {
        /// The track.
        track: TrackIndex,
        /// The pan.
        pan: u8,
    },
    /// Set the track's reverb send (0-127).
    SetTrackReverb {
        /// The track.
        track: TrackIndex,
        /// The send.
        reverb: u8,
    },
    /// Set the track's chorus send (0-127).
    SetTrackChorus {
        /// The track.
        track: TrackIndex,
        /// The send.
        chorus: u8,
    },
    /// Mute or unmute the track.
    SetTrackMute {
        /// The track.
        track: TrackIndex,
        /// Whether it is muted.
        mute: bool,
    },
    /// Solo or unsolo the track.
    SetTrackSolo {
        /// The track.
        track: TrackIndex,
        /// Whether it is soloed.
        solo: bool,
    },
    /// Set the track's General MIDI program (the SoundFont preset it plays).
    SetTrackProgram {
        /// The track.
        track: TrackIndex,
        /// The program, 0 to 127.
        program: u8,
    },
    /// Make the track a drum kit, or an ordinary instrument again.
    SetTrackDrums {
        /// The track.
        track: TrackIndex,
        /// Whether the track is a drum kit.
        drums: bool,
    },
    /// Set the track's capo fret (0 for none).
    SetTrackCapo {
        /// The track.
        track: TrackIndex,
        /// The capo fret.
        capo: u8,
    },
    /// Change how many strings the track has.
    SetTrackStrings {
        /// The track.
        track: TrackIndex,
        /// The number of strings.
        strings: usize,
    },
    /// Set one string's open pitch.
    SetTrackStringPitch {
        /// The track.
        track: TrackIndex,
        /// The string (1 = highest).
        string: StringNumber,
        /// The open-string pitch.
        pitch: Pitch,
    },
    /// Set one text field of the song.
    SetSongText {
        /// Which field.
        field: SongTextField,
        /// The new text.
        text: String,
    },
    /// Set the song's initial tempo.
    SetTempo {
        /// Quarter notes per minute.
        tempo: u32,
    },
    /// Remove a beat's notes, leaving a rest.
    SetBeatRest {
        /// The beat.
        at: BeatPosition,
    },
    /// Make a note dead (muted) or normal again.
    SetDeadNote {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
        /// Whether it is dead.
        dead: bool,
    },
    /// Tie a note to the previous one, or remove the tie.
    SetTie {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
        /// Whether it is tied.
        tie: bool,
    },
    /// Set the tempo a measure starts at.
    SetMeasureTempo {
        /// The measure.
        measure: MeasureIndex,
        /// Quarter notes per minute.
        tempo: u32,
    },
    /// Set a measure's time signature.
    SetTimeSignature {
        /// The measure.
        measure: MeasureIndex,
        /// Beats per measure.
        numerator: u8,
        /// Note value of one beat.
        denominator: u16,
    },
    /// Set a measure's key signature.
    SetKeySignature {
        /// The measure.
        measure: MeasureIndex,
        /// Sharps (positive) or flats (negative), -7 to 7.
        key: i8,
        /// Whether the key is minor.
        minor: bool,
    },
    /// Set (or replace) a measure's section marker.
    SetSection {
        /// The measure.
        measure: MeasureIndex,
        /// The section's title.
        title: String,
        /// The marker's color.
        color: TrackColor,
    },
    /// Remove a measure's section marker.
    ClearSection {
        /// The measure.
        measure: MeasureIndex,
    },
    /// Set or clear one effect of a note.
    SetNoteEffect {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
        /// The effect and its new state.
        effect: NoteEffect,
    },
    /// Set or clear one effect of a whole beat.
    SetBeatEffect {
        /// The beat.
        at: BeatPosition,
        /// The effect and its new state.
        effect: BeatEffect,
    },
    /// Several edits applied and undone as one step.
    Batch {
        /// The edits, in order (no `undo`/`redo`).
        commands: Vec<Command>,
    },
    /// Undo the last edit (handled by [`crate::Editor`]).
    Undo,
    /// Redo the last undone edit (handled by [`crate::Editor`]).
    Redo,
}

impl Command {
    /// The command as one line of the command language, which [`crate::parse_line`] reads
    /// back as the same command (positions 1-based). The recovery journal stores commands
    /// this way.
    pub fn to_line(&self) -> String {
        let one = |index: usize| index.saturating_add(1);
        let at = |at: &BeatPosition| {
            format!(
                "track={} measure={} voice={} beat={}",
                one(at.track.get()),
                one(at.measure.get()),
                one(at.voice.get()),
                one(at.beat.get())
            )
        };
        let id = self.info().id;
        match self {
            Command::SetFret {
                at: p,
                string,
                fret,
            } => format!("{id} {} string={} fret={}", at(p), string.get(), fret.get()),
            Command::ClearNote { at: p, string } => {
                format!("{id} {} string={}", at(p), string.get())
            }
            Command::SetDuration { at: p, duration } | Command::InsertBeat { at: p, duration } => {
                format!("{id} {} duration={duration}", at(p))
            }
            Command::DeleteBeat { at: p } => format!("{id} {}", at(p)),
            Command::InsertMeasure { measure } | Command::DeleteMeasure { measure } => {
                format!("{id} measure={}", one(measure.get()))
            }
            Command::InsertTrack { at: track }
            | Command::DeleteTrack { track }
            | Command::DuplicateTrack { track } => format!("{id} track={}", one(track.get())),
            Command::MoveTrack { from, to } => {
                format!("{id} from={} to={}", one(from.get()), one(to.get()))
            }
            Command::RenameTrack { track, name } => format!(
                "{id} track={} name={}",
                one(track.get()),
                crate::parse::encode_text(name)
            ),
            Command::SetTrackColor { track, color } => {
                format!("{id} track={} color={:06x}", one(track.get()), color.rgb())
            }
            Command::SetTrackVolume { track, volume } => {
                format!("{id} track={} volume={volume}", one(track.get()))
            }
            Command::SetTrackPan { track, pan } => {
                format!("{id} track={} pan={pan}", one(track.get()))
            }
            Command::SetTrackReverb { track, reverb } => {
                format!("{id} track={} reverb={reverb}", one(track.get()))
            }
            Command::SetTrackChorus { track, chorus } => {
                format!("{id} track={} chorus={chorus}", one(track.get()))
            }
            Command::SetTrackStaves { track, staves } => {
                format!("{id} track={} staves={}", one(track.get()), staves.id())
            }
            Command::SetMasterVolume { volume } => format!("{id} volume={volume}"),
            Command::SetMasterPan { pan } => format!("{id} pan={pan}"),
            Command::SetTrackMute { track, mute } => {
                format!("{id} track={} mute={mute}", one(track.get()))
            }
            Command::SetTrackSolo { track, solo } => {
                format!("{id} track={} solo={solo}", one(track.get()))
            }
            Command::SetTrackProgram { track, program } => {
                format!("{id} track={} program={program}", one(track.get()))
            }
            Command::SetTrackDrums { track, drums } => {
                format!("{id} track={} drums={drums}", one(track.get()))
            }
            Command::SetTrackCapo { track, capo } => {
                format!("{id} track={} capo={capo}", one(track.get()))
            }
            Command::SetTrackStrings { track, strings } => {
                format!("{id} track={} strings={strings}", one(track.get()))
            }
            Command::SetTrackStringPitch {
                track,
                string,
                pitch,
            } => format!(
                "{id} track={} string={} pitch={pitch}",
                one(track.get()),
                string.get()
            ),
            Command::SetSongText { field, text } => {
                format!(
                    "{id} field={} text={}",
                    field.id(),
                    crate::parse::encode_text(text)
                )
            }
            Command::SetTempo { tempo } => format!("{id} tempo={tempo}"),
            Command::SetBeatRest { at: p } => format!("{id} {}", at(p)),
            Command::SetTie { at: p, string, tie } => {
                format!("{id} {} string={} tie={tie}", at(p), string.get())
            }
            Command::SetDeadNote {
                at: p,
                string,
                dead,
            } => {
                format!("{id} {} string={} dead={dead}", at(p), string.get())
            }
            Command::SetMeasureTempo { measure, tempo } => {
                format!("{id} measure={} tempo={tempo}", one(measure.get()))
            }
            Command::SetTimeSignature {
                measure,
                numerator,
                denominator,
            } => format!(
                "{id} measure={} numerator={numerator} denominator={denominator}",
                one(measure.get())
            ),
            Command::SetKeySignature {
                measure,
                key,
                minor,
            } => format!(
                "{id} measure={} key={key} minor={minor}",
                one(measure.get())
            ),
            Command::SetSection {
                measure,
                title,
                color,
            } => format!(
                "{id} measure={} title={} color={:06x}",
                one(measure.get()),
                crate::parse::encode_text(title),
                color.rgb()
            ),
            Command::ClearSection { measure } => {
                format!("{id} measure={}", one(measure.get()))
            }
            Command::SetNoteEffect {
                at: p,
                string,
                effect,
            } => {
                let (effect_id, value) = crate::effect_text::note_effect_text(effect);
                format!(
                    "{id} {} string={} effect={effect_id} value={}",
                    at(p),
                    string.get(),
                    crate::parse::encode_text(&value)
                )
            }
            Command::SetBeatEffect { at: p, effect } => {
                let (effect_id, value) = crate::effect_text::beat_effect_text(effect);
                format!(
                    "{id} {} effect={effect_id} value={}",
                    at(p),
                    crate::parse::encode_text(&value)
                )
            }
            Command::Batch { commands } => {
                let mut line = id.to_owned();
                for command in commands {
                    line.push(' ');
                    line.push_str(&crate::parse::encode_text(&command.to_line()));
                }
                line
            }
            Command::Undo | Command::Redo => id.to_owned(),
        }
    }

    /// The beat the command works on, if it works on one beat.
    pub fn position(&self) -> Option<BeatPosition> {
        match self {
            Command::SetFret { at, .. }
            | Command::ClearNote { at, .. }
            | Command::SetDuration { at, .. }
            | Command::InsertBeat { at, .. }
            | Command::DeleteBeat { at } => Some(*at),
            _ => None,
        }
    }

    /// The measure a measure command works on.
    pub fn measure(&self) -> Option<MeasureIndex> {
        match self {
            Command::InsertMeasure { measure } | Command::DeleteMeasure { measure } => {
                Some(*measure)
            }
            _ => None,
        }
    }

    /// This command's metadata.
    pub fn info(&self) -> &'static CommandInfo {
        match self {
            Command::SetFret { .. } => &SET_FRET,
            Command::ClearNote { .. } => &CLEAR_NOTE,
            Command::SetDuration { .. } => &SET_DURATION,
            Command::InsertBeat { .. } => &INSERT_BEAT,
            Command::DeleteBeat { .. } => &DELETE_BEAT,
            Command::InsertMeasure { .. } => &INSERT_MEASURE,
            Command::DeleteMeasure { .. } => &DELETE_MEASURE,
            Command::InsertTrack { .. } => &INSERT_TRACK,
            Command::DeleteTrack { .. } => &DELETE_TRACK,
            Command::DuplicateTrack { .. } => &DUPLICATE_TRACK,
            Command::MoveTrack { .. } => &MOVE_TRACK,
            Command::RenameTrack { .. } => &RENAME_TRACK,
            Command::SetTrackColor { .. } => &SET_TRACK_COLOR,
            Command::SetTrackVolume { .. } => &SET_TRACK_VOLUME,
            Command::SetTrackPan { .. } => &SET_TRACK_PAN,
            Command::SetTrackReverb { .. } => &SET_TRACK_REVERB,
            Command::SetTrackChorus { .. } => &SET_TRACK_CHORUS,
            Command::SetTrackStaves { .. } => &SET_TRACK_STAVES,
            Command::SetMasterVolume { .. } => &SET_MASTER_VOLUME,
            Command::SetMasterPan { .. } => &SET_MASTER_PAN,
            Command::SetTrackMute { .. } => &SET_TRACK_MUTE,
            Command::SetTrackSolo { .. } => &SET_TRACK_SOLO,
            Command::SetTrackProgram { .. } => &SET_TRACK_PROGRAM,
            Command::SetTrackDrums { .. } => &SET_TRACK_DRUMS,
            Command::SetTrackCapo { .. } => &SET_TRACK_CAPO,
            Command::SetTrackStrings { .. } => &SET_TRACK_STRINGS,
            Command::SetTrackStringPitch { .. } => &SET_TRACK_STRING_PITCH,
            Command::SetSongText { .. } => &SET_SONG_TEXT,
            Command::SetTempo { .. } => &SET_TEMPO,
            Command::SetBeatRest { .. } => &SET_BEAT_REST,
            Command::SetTie { .. } => &SET_TIE,
            Command::SetDeadNote { .. } => &SET_DEAD_NOTE,
            Command::SetMeasureTempo { .. } => &SET_MEASURE_TEMPO,
            Command::SetTimeSignature { .. } => &SET_TIME_SIGNATURE,
            Command::SetKeySignature { .. } => &SET_KEY_SIGNATURE,
            Command::SetSection { .. } => &SET_SECTION,
            Command::ClearSection { .. } => &CLEAR_SECTION,
            Command::SetNoteEffect { .. } => &SET_NOTE_EFFECT,
            Command::SetBeatEffect { .. } => &SET_BEAT_EFFECT,
            Command::Batch { .. } => &BATCH,
            Command::Undo => &UNDO,
            Command::Redo => &REDO,
        }
    }

    /// The command id (the canonical one, not an alias).
    pub fn id(&self) -> &'static str {
        self.info().id
    }

    /// The short, imperative label.
    pub fn label(&self) -> &'static str {
        self.info().label
    }

    /// The command's arguments as one human-readable line, for example
    /// `track 1, measure 2, beat 3, string 4, fret 5`. Empty for undo/redo; a batch lists
    /// how many changes it holds.
    pub fn details(&self) -> String {
        if let Command::Batch { commands } = self {
            let labels: Vec<&str> = commands.iter().map(Command::label).collect();
            return format!("{} change(s): {}", commands.len(), labels.join(", "));
        }
        if self.is_history() {
            return String::new();
        }
        let line = self.to_line();
        let Some((_, rest)) = line.split_once(' ') else {
            return String::new();
        };
        rest.split(' ')
            .filter(|token| !token.is_empty())
            .map(|token| match token.split_once('=') {
                Some((key, value)) => {
                    let value =
                        crate::parse::decode_text(value).unwrap_or_else(|| value.to_owned());
                    format!("{key} {value}")
                }
                None => token.to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// True for `undo` and `redo`, which the editor handles instead of the model.
    pub fn is_history(&self) -> bool {
        matches!(self, Command::Undo | Command::Redo)
    }

    /// True for commands that add, remove, duplicate or move tracks, so every track index
    /// (and every per-track cache keyed by one) may change.
    pub fn changes_tracks(&self) -> bool {
        match self {
            Command::InsertTrack { .. }
            | Command::DeleteTrack { .. }
            | Command::DuplicateTrack { .. }
            | Command::MoveTrack { .. } => true,
            Command::Batch { commands } => commands.iter().any(Command::changes_tracks),
            _ => false,
        }
    }

    /// True for commands that may change how every measure is laid out (time and key
    /// signatures, section markers). The app then drops the layout cache.
    pub fn changes_layout(&self) -> bool {
        match self {
            Command::SetTrackStaves { .. }
            | Command::SetTimeSignature { .. }
            | Command::SetKeySignature { .. }
            | Command::SetSection { .. }
            | Command::ClearSection { .. } => true,
            Command::Batch { commands } => commands.iter().any(Command::changes_layout),
            _ => false,
        }
    }

    /// True for the master volume and pan, which change how every track sounds. Playback
    /// sends every track's new sound at once, like [`Command::changes_sound`] for one.
    pub fn changes_master(&self) -> bool {
        match self {
            Command::SetMasterVolume { .. } | Command::SetMasterPan { .. } => true,
            Command::Batch { commands } => {
                !commands.is_empty() && commands.iter().all(Command::changes_master)
            }
            _ => false,
        }
    }

    /// True for commands that change a track's sound (volume, pan, reverb, chorus,
    /// program). Playback sends these to the player at once, without rebuilding the
    /// sequence.
    pub fn changes_sound(&self) -> bool {
        matches!(
            self,
            Command::SetTrackVolume { .. }
                | Command::SetTrackPan { .. }
                | Command::SetTrackReverb { .. }
                | Command::SetTrackChorus { .. }
                | Command::SetTrackProgram { .. }
        )
    }

    /// True for commands that change which notes play, how they are mapped to a channel or
    /// what pitch they sound (mute, solo, drum kit, capo, string count, tuning). Playback
    /// must rebuild the sequence for these, and can do it without stopping.
    pub fn rebuilds_playback(&self) -> bool {
        match self {
            Command::SetTrackMute { .. }
            | Command::SetTrackSolo { .. }
            | Command::SetTrackDrums { .. }
            | Command::SetTrackCapo { .. }
            | Command::SetTrackStrings { .. }
            | Command::SetTrackStringPitch { .. }
            | Command::SetTempo { .. }
            | Command::SetMeasureTempo { .. }
            | Command::SetTimeSignature { .. }
            | Command::SetNoteEffect { .. }
            | Command::SetBeatEffect { .. }
            | Command::SetDeadNote { .. } => true,
            Command::Batch { commands } => commands.iter().any(Command::rebuilds_playback),
            _ => false,
        }
    }

    /// True for commands that change only how the song or a track is named or shown:
    /// playback goes on.
    pub fn changes_display_only(&self) -> bool {
        match self {
            Command::RenameTrack { .. }
            | Command::SetTrackColor { .. }
            | Command::SetSongText { .. }
            | Command::SetTrackStaves { .. }
            | Command::SetSection { .. }
            | Command::ClearSection { .. }
            | Command::SetKeySignature { .. } => true,
            Command::Batch { commands } => commands.iter().all(Command::changes_display_only),
            _ => false,
        }
    }

    /// The track a sound command changes.
    pub fn mix_track(&self) -> Option<TrackIndex> {
        match self {
            Command::SetTrackVolume { track, .. }
            | Command::SetTrackPan { track, .. }
            | Command::SetTrackReverb { track, .. }
            | Command::SetTrackChorus { track, .. }
            | Command::SetTrackProgram { track, .. } => Some(*track),
            // Only a batch of nothing but sound changes on one track can be sent live.
            Command::Batch { commands } => {
                let mut track = None;
                for command in commands {
                    let candidate = command.mix_track()?;
                    if track.is_some_and(|track| track != candidate) {
                        return None;
                    }
                    track = Some(candidate);
                }
                track
            }
            _ => None,
        }
    }

    /// Whether `self` continues `previous` in the undo history: consecutive volume, pan or
    /// color changes on the same track (one slider or color-picker drag) become a single
    /// undo step.
    pub fn continues(&self, previous: &Command) -> bool {
        match (previous, self) {
            (
                Command::SetTrackVolume { track: first, .. },
                Command::SetTrackVolume { track: second, .. },
            )
            | (
                Command::SetTrackPan { track: first, .. },
                Command::SetTrackPan { track: second, .. },
            )
            | (
                Command::SetTrackReverb { track: first, .. },
                Command::SetTrackReverb { track: second, .. },
            )
            | (
                Command::SetTrackChorus { track: first, .. },
                Command::SetTrackChorus { track: second, .. },
            )
            | (
                Command::SetTrackColor { track: first, .. },
                Command::SetTrackColor { track: second, .. },
            ) => first == second,
            (Command::SetMasterVolume { .. }, Command::SetMasterVolume { .. })
            | (Command::SetMasterPan { .. }, Command::SetMasterPan { .. }) => true,
            // One Apply in the settings window is one undo step for the fields it changed.
            (
                Command::SetSongText { .. } | Command::SetTempo { .. },
                Command::SetSongText { .. } | Command::SetTempo { .. },
            ) => true,
            _ => false,
        }
    }

    /// Whether the command can run on `song` right now. A command that is not enabled fails
    /// when applied; this is the cheap check for menus and key bindings.
    pub fn enabled(&self, song: &Song) -> bool {
        match self {
            Command::SetFret { at, string, .. } => note_target(song, *at, *string).is_some(),
            Command::ClearNote { at, string } => note_target(song, *at, *string)
                .is_some_and(|beat| beat.notes().any(|note| note.string().ok() == Some(*string))),
            Command::SetDuration { at, .. } => song
                .beat(*at)
                .is_some_and(|beat| beat.kind() != BeatKind::Empty),
            Command::InsertBeat { at, .. } => {
                // `apply` creates voice 1 of a measure that has no voices (GP7 empty
                // bars import that way), so the first beat of such a measure is editable.
                let Some(measure) = song.track(at.track).and_then(|t| t.measure(at.measure)) else {
                    return false;
                };
                let voices = measure.voices().len();
                if voices == 0 {
                    at.voice.get() == 0 && at.beat.get() == 0
                } else if let Some(voice) = measure.voice(at.voice) {
                    at.beat.get() <= voice.beats().len()
                } else {
                    // The next voice (2) may be created at its first beat.
                    at.voice.get() == voices && voices < 2 && at.beat.get() == 0
                }
            }
            Command::DeleteBeat { at } => song.beat(*at).is_some(),
            Command::InsertMeasure { measure } => {
                song.track_count() > 0 && measure.get() <= song.measure_count()
            }
            Command::DeleteMeasure { measure } => {
                song.track_count() > 0
                    && song.measure_count() > 1
                    && measure.get() < song.measure_count()
            }
            Command::InsertTrack { at } => {
                song.track_count() < gscore_model::MAX_TRACKS && at.get() <= song.track_count()
            }
            Command::DeleteTrack { track } => {
                song.track_count() > 1 && track.get() < song.track_count()
            }
            Command::DuplicateTrack { track } => {
                song.track_count() < gscore_model::MAX_TRACKS && track.get() < song.track_count()
            }
            Command::MoveTrack { from, to } => {
                from.get() < song.track_count() && to.get() < song.track_count() && from != to
            }
            Command::RenameTrack { track, name } => {
                track.get() < song.track_count()
                    && name.chars().count() <= gscore_model::MAX_TRACK_NAME_CHARS
            }
            Command::SetTrackColor { track, .. }
            | Command::SetTrackVolume { track, .. }
            | Command::SetTrackPan { track, .. }
            | Command::SetTrackReverb { track, .. }
            | Command::SetTrackChorus { track, .. }
            | Command::SetTrackStaves { track, .. }
            | Command::SetTrackMute { track, .. }
            | Command::SetTrackSolo { track, .. }
            | Command::SetTrackDrums { track, .. } => track.get() < song.track_count(),
            Command::SetMasterVolume { volume: value } | Command::SetMasterPan { pan: value } => {
                *value <= 127
            }
            Command::SetTrackProgram { track, program } => {
                *program <= 127 && track.get() < song.track_count()
            }
            Command::SetTrackCapo { track, capo } => {
                *capo <= 127 && track.get() < song.track_count()
            }
            Command::SetTrackStrings { track, strings } => {
                song.check_track_strings(*track, *strings).is_ok()
            }
            Command::SetTrackStringPitch { track, string, .. } => song
                .track(*track)
                .is_some_and(|view| usize::from(string.get()) <= view.string_count()),
            Command::SetSongText { field, text } => field.check(text).is_ok(),
            Command::SetTempo { tempo } => (1..=MAX_TEMPO).contains(tempo),
            Command::SetBeatRest { at } => song.beat(*at).is_some(),
            Command::SetTie { at, string, .. } | Command::SetDeadNote { at, string, .. } => song
                .beat(*at)
                .is_some_and(|beat| beat.notes().any(|note| note.string().ok() == Some(*string))),
            Command::SetMeasureTempo { measure, tempo } => {
                measure.get() < song.measure_count() && (1..=MAX_TEMPO).contains(tempo)
            }
            Command::SetTimeSignature {
                measure,
                numerator,
                denominator,
            } => {
                measure.get() < song.measure_count()
                    && TimeSignature::new(*numerator, *denominator).is_ok()
            }
            Command::SetKeySignature { measure, key, .. } => {
                measure.get() < song.measure_count() && (-7..=7).contains(key)
            }
            Command::SetSection { measure, title, .. } => {
                measure.get() < song.measure_count()
                    && title.chars().count() <= MAX_SECTION_TITLE_CHARS
            }
            Command::ClearSection { measure } => measure.get() < song.measure_count(),
            Command::SetNoteEffect { at, string, effect } => {
                effect.validate().is_ok()
                    && song.beat(*at).is_some_and(|beat| {
                        beat.notes().any(|note| note.string().ok() == Some(*string))
                    })
            }
            Command::SetBeatEffect { at, effect } => {
                effect.validate().is_ok() && song.beat(*at).is_some()
            }
            // A batch is enabled when all of its parts apply, one after another.
            Command::Batch { commands } => {
                let mut edited = song.clone();
                commands.iter().all(|command| {
                    !command.is_history()
                        && command.enabled(&edited)
                        && command.apply(&mut edited).is_ok()
                })
            }
            // The editor decides; the history stack is not part of the song.
            Command::Undo | Command::Redo => true,
        }
    }

    /// Undoes a change previously returned by [`Command::apply`]. History commands
    /// (`undo`/`redo`) have no model change; the editor handles them.
    pub fn undo(&self, song: &mut Song, change: &Change) -> Result<(), ModelError> {
        song.revert(change)
    }

    /// Applies the command, returning the change to keep for undo. The model applies it to a
    /// copy first, so an error leaves the song unchanged.
    pub fn apply(&self, song: &mut Song) -> Result<Change, ModelError> {
        match self {
            Command::SetFret { at, string, fret } => song.set_note(*at, *string, *fret),
            Command::ClearNote { at, string } => song.clear_note(*at, *string),
            Command::SetDuration { at, duration } => song.set_duration(*at, *duration),
            Command::InsertBeat { at, duration } => song.insert_beat(*at, *duration),
            Command::DeleteBeat { at } => song.delete_beat(*at),
            Command::InsertMeasure { measure } => song.insert_measure(*measure),
            Command::DeleteMeasure { measure } => song.delete_measure(*measure),
            Command::InsertTrack { at } => song.insert_track(*at),
            Command::DeleteTrack { track } => song.delete_track(*track),
            Command::DuplicateTrack { track } => song.duplicate_track(*track),
            Command::MoveTrack { from, to } => song.move_track(*from, *to),
            Command::RenameTrack { track, name } => song.rename_track(*track, name),
            Command::SetTrackColor { track, color } => song.set_track_color(*track, *color),
            Command::SetTrackVolume { track, volume } => song.set_track_volume(*track, *volume),
            Command::SetTrackPan { track, pan } => song.set_track_pan(*track, *pan),
            Command::SetTrackReverb { track, reverb } => song.set_track_reverb(*track, *reverb),
            Command::SetTrackChorus { track, chorus } => song.set_track_chorus(*track, *chorus),
            Command::SetTrackStaves { track, staves } => song.set_track_staves(*track, *staves),
            Command::SetMasterVolume { volume } => song.set_master_volume(*volume),
            Command::SetMasterPan { pan } => song.set_master_pan(*pan),
            Command::SetTrackMute { track, mute } => song.set_track_mute(*track, *mute),
            Command::SetTrackSolo { track, solo } => song.set_track_solo(*track, *solo),
            Command::SetTrackProgram { track, program } => song.set_track_program(*track, *program),
            Command::SetTrackDrums { track, drums } => song.set_track_drums(*track, *drums),
            Command::SetTrackCapo { track, capo } => song.set_track_capo(*track, *capo),
            Command::SetTrackStrings { track, strings } => song.set_track_strings(*track, *strings),
            Command::SetTrackStringPitch {
                track,
                string,
                pitch,
            } => song.set_track_string_pitch(*track, *string, *pitch),
            Command::SetSongText { field, text } => song.set_text(*field, text),
            Command::SetTempo { tempo } => song.set_tempo(*tempo),
            Command::SetBeatRest { at } => song.set_beat_rest(*at),
            Command::SetTie { at, string, tie } => song.set_note_tie(*at, *string, *tie),
            Command::SetDeadNote { at, string, dead } => song.set_note_dead(*at, *string, *dead),
            Command::SetMeasureTempo { measure, tempo } => song.set_measure_tempo(*measure, *tempo),
            Command::SetTimeSignature {
                measure,
                numerator,
                denominator,
            } => song.set_time_signature(*measure, *numerator, *denominator),
            Command::SetKeySignature {
                measure,
                key,
                minor,
            } => song.set_key_signature(*measure, *key, *minor),
            Command::SetSection {
                measure,
                title,
                color,
            } => song.set_section(*measure, title, *color),
            Command::ClearSection { measure } => {
                song.set_section(*measure, "", TrackColor::DEFAULT)
            }
            Command::SetNoteEffect { at, string, effect } => {
                song.set_note_effect(*at, *string, effect.clone())
            }
            Command::SetBeatEffect { at, effect } => song.set_beat_effect(*at, effect.clone()),
            Command::Batch { commands } => {
                let mut changes = Vec::with_capacity(commands.len());
                for command in commands {
                    match command.apply(song) {
                        Ok(change) => changes.push(change),
                        Err(error) => {
                            // Undo the parts already applied, newest first.
                            for change in changes.iter().rev() {
                                let _ = song.revert(change);
                            }
                            return Err(error);
                        }
                    }
                }
                Ok(Change::batch(changes))
            }
            Command::Undo | Command::Redo => Err(ModelError::Invalid(
                "undo and redo are handled by the editor".to_owned(),
            )),
        }
    }
}

/// The beat at `at`, if it exists and the track has `string`. Both must hold before a note
/// can be set or cleared.
fn note_target(song: &Song, at: BeatPosition, string: StringNumber) -> Option<BeatView<'_>> {
    let track = song.track(at.track)?;
    if usize::from(string.get()) > track.string_count() {
        return None;
    }
    song.beat(at)
}

/// One entry of what changed a song, as the recovery journal records it: a command, and
/// whether it was merged into the undo step before it (the next part of a slider drag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// The command (including `undo` and `redo`).
    pub command: Command,
    /// Whether it continued the undo step before it instead of starting its own.
    pub merged: bool,
}

/// What a merged entry's line starts with.
const MERGED: &str = "merge ";

impl Recorded {
    /// The entry as one line: the command's line, after `merge ` if it was merged.
    pub fn to_line(&self) -> String {
        if self.merged {
            format!("{MERGED}{}", self.command.to_line())
        } else {
            self.command.to_line()
        }
    }

    /// Reads a line written by [`Recorded::to_line`]. `Ok(None)` for a blank line.
    pub fn parse(line: &str) -> Result<Option<Recorded>, crate::ParseError> {
        let (merged, line) = match line.strip_prefix(MERGED) {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        Ok(crate::parse_line(line)?.map(|command| Recorded { command, merged }))
    }
}

impl From<Command> for Recorded {
    fn from(command: Command) -> Self {
        Recorded {
            command,
            merged: false,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn parsed(line: &str) -> Command {
        crate::parse_line(line).unwrap().unwrap()
    }

    #[test]
    fn details_name_the_arguments_in_plain_words() {
        let command = parsed("set_fret track=2 measure=3 voice=1 beat=4 string=5 fret=7");
        assert_eq!(
            command.details(),
            "track 2, measure 3, voice 1, beat 4, string 5, fret 7"
        );
        let text = parsed("set_song_text field=title text=%%New%20Title");
        assert!(
            text.details().contains("text New Title"),
            "{}",
            text.details()
        );
        assert!(Command::Undo.details().is_empty());
    }
}
