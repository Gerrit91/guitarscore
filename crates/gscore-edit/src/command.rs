//! The command enum: what an edit is, whether it can run, and how it applies.

use gscore_model::{
    BeatKind, BeatPosition, BeatView, Change, Fret, MAX_TEMPO, MeasureIndex, ModelError, NoteValue,
    Pitch, Song, SongTextField, StringNumber, TrackColor, TrackIndex,
};

use crate::info::{
    CLEAR_NOTE, CommandInfo, DELETE_BEAT, DELETE_MEASURE, DELETE_TRACK, DUPLICATE_TRACK,
    INSERT_BEAT, INSERT_MEASURE, INSERT_TRACK, MOVE_TRACK, REDO, RENAME_TRACK, SET_DURATION,
    SET_FRET, SET_SONG_TEXT, SET_TEMPO, SET_TRACK_CAPO, SET_TRACK_COLOR, SET_TRACK_DRUMS,
    SET_TRACK_MUTE, SET_TRACK_PAN, SET_TRACK_PROGRAM, SET_TRACK_SOLO, SET_TRACK_STRING_PITCH,
    SET_TRACK_STRINGS, SET_TRACK_VOLUME, UNDO,
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
    /// Set the track's pan (0 left, 64 middle, 127 right).
    SetTrackPan {
        /// The track.
        track: TrackIndex,
        /// The pan.
        pan: u8,
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
            Command::SetTrackMute { .. } => &SET_TRACK_MUTE,
            Command::SetTrackSolo { .. } => &SET_TRACK_SOLO,
            Command::SetTrackProgram { .. } => &SET_TRACK_PROGRAM,
            Command::SetTrackDrums { .. } => &SET_TRACK_DRUMS,
            Command::SetTrackCapo { .. } => &SET_TRACK_CAPO,
            Command::SetTrackStrings { .. } => &SET_TRACK_STRINGS,
            Command::SetTrackStringPitch { .. } => &SET_TRACK_STRING_PITCH,
            Command::SetSongText { .. } => &SET_SONG_TEXT,
            Command::SetTempo { .. } => &SET_TEMPO,
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

    /// True for `undo` and `redo`, which the editor handles instead of the model.
    pub fn is_history(&self) -> bool {
        matches!(self, Command::Undo | Command::Redo)
    }

    /// True for commands that add, remove, duplicate or move tracks, so every track index
    /// (and every per-track cache keyed by one) may change.
    pub fn changes_tracks(&self) -> bool {
        matches!(
            self,
            Command::InsertTrack { .. }
                | Command::DeleteTrack { .. }
                | Command::DuplicateTrack { .. }
                | Command::MoveTrack { .. }
        )
    }

    /// True for commands that change a track's sound (volume, pan, program). Playback sends
    /// these to the player at once, without rebuilding the sequence.
    pub fn changes_sound(&self) -> bool {
        matches!(
            self,
            Command::SetTrackVolume { .. }
                | Command::SetTrackPan { .. }
                | Command::SetTrackProgram { .. }
        )
    }

    /// True for commands that change which notes play, how they are mapped to a channel or
    /// what pitch they sound (mute, solo, drum kit, capo, string count, tuning). Playback
    /// must rebuild the sequence for these, and can do it without stopping.
    pub fn rebuilds_playback(&self) -> bool {
        matches!(
            self,
            Command::SetTrackMute { .. }
                | Command::SetTrackSolo { .. }
                | Command::SetTrackDrums { .. }
                | Command::SetTrackCapo { .. }
                | Command::SetTrackStrings { .. }
                | Command::SetTrackStringPitch { .. }
                | Command::SetTempo { .. }
        )
    }

    /// True for commands that change only how the song or a track is named or shown:
    /// playback goes on.
    pub fn changes_display_only(&self) -> bool {
        matches!(
            self,
            Command::RenameTrack { .. }
                | Command::SetTrackColor { .. }
                | Command::SetSongText { .. }
        )
    }

    /// The track a sound command changes.
    pub fn mix_track(&self) -> Option<TrackIndex> {
        match self {
            Command::SetTrackVolume { track, .. }
            | Command::SetTrackPan { track, .. }
            | Command::SetTrackProgram { track, .. } => Some(*track),
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
                Command::SetTrackColor { track: first, .. },
                Command::SetTrackColor { track: second, .. },
            ) => first == second,
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
                if measure.voices().len() == 0 {
                    at.voice.get() == 0 && at.beat.get() == 0
                } else {
                    measure
                        .voice(at.voice)
                        .is_some_and(|voice| at.beat.get() <= voice.beats().len())
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
            | Command::SetTrackMute { track, .. }
            | Command::SetTrackSolo { track, .. }
            | Command::SetTrackDrums { track, .. } => track.get() < song.track_count(),
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
