//! The command enum: what an edit is, whether it can run, and how it applies.

use gscore_model::{
    BeatEffect, BeatKind, BeatPosition, BeatView, Change, ChordDiagram, Clef, Dynamic, Feel,
    Fermata, Fret, InstrumentSymbol, MAX_BARS_PER_SYSTEM, MAX_SECTION_TITLE_CHARS, MAX_TEMPO,
    MeasureIndex, ModelError, NoteEffect, NoteValue, PageSetup, Pitch, Repeat, SimileMark, Song,
    SongTextField, StringNumber, SystemBreak, TimeSignature, TrackColor, TrackIndex, TrackStaves,
};

use crate::info::{
    BATCH, CLEAR_NOTE, CLEAR_SECTION, CommandInfo, DELETE_BEAT, DELETE_MEASURE, DELETE_TRACK,
    DUPLICATE_TRACK, INSERT_BEAT, INSERT_MEASURE, INSERT_TRACK, MOVE_TRACK, REDO, RENAME_TRACK,
    SET_BEAT_EFFECT, SET_BEAT_REST, SET_BEAT_TEXT, SET_BEAT_TIE, SET_CHORD_DIAGRAM, SET_CLEF,
    SET_DEAD_NOTE, SET_DOUBLE_BAR, SET_DURATION, SET_DYNAMIC, SET_FEEL, SET_FERMATA, SET_FREE_TIME,
    SET_FRET, SET_KEY_SIGNATURE, SET_LYRIC_LINE, SET_MASTER_PAN, SET_MASTER_VOLUME,
    SET_MEASURE_TEMPO, SET_NOTE_EFFECT, SET_REPEAT, SET_SECTION, SET_SIMILE_MARK, SET_SONG_SOUND,
    SET_SONG_TEXT, SET_TEMPO, SET_TIE, SET_TIME_SIGNATURE, SET_TRACK_CAPO, SET_TRACK_CHORUS,
    SET_TRACK_COLOR, SET_TRACK_DRUMS, SET_TRACK_MUTE, SET_TRACK_PAN, SET_TRACK_PROGRAM,
    SET_TRACK_REVERB, SET_TRACK_SOLO, SET_TRACK_SOUND, SET_TRACK_STAVES, SET_TRACK_STRING_PITCH,
    SET_TRACK_STRINGS, SET_TRACK_SYMBOL, SET_TRACK_VOLUME, UNDO,
};
use crate::info::{SET_BARS_PER_SYSTEM, SET_PAGE_SETUP, SET_SCORE_LINE_BREAK, SET_SYSTEM_BREAK};

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
    /// Set the symbol the app draws for the track (an instrument icon).
    SetTrackSymbol {
        /// The track.
        track: TrackIndex,
        /// The instrument symbol.
        symbol: InstrumentSymbol,
    },
    /// Set GuitarScore's own sound state for a track: the app's opaque payload (what
    /// plays the track, its tone and chain, its sends, ...), or `None` for its defaults.
    /// The Guitar Pro format has no field for it; the app saves it in the song's notes.
    SetTrackSound {
        /// The track.
        track: TrackIndex,
        /// The app's state, or `None` to clear it.
        state: Option<String>,
    },
    /// Set GuitarScore's own song-wide sound state: the app's opaque payload (the mixer's
    /// reverb and compressor buses, ...), or `None` for the program's own settings.
    SetSongSound {
        /// The app's state, or `None` to clear it.
        state: Option<String>,
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
    /// Set how the song is printed: paper, margins, score size, header and footer.
    SetPageSetup {
        /// The new page setup.
        setup: PageSetup,
    },
    /// Make a measure of a track start a new line or page, or remove its break.
    SetSystemBreak {
        /// The track.
        track: TrackIndex,
        /// The measure (not the first).
        measure: MeasureIndex,
        /// The break.
        kind: SystemBreak,
    },
    /// Start a new line at a measure when all tracks are shown together (Guitar Pro's
    /// multitrack layout), or remove that break.
    SetScoreLineBreak {
        /// The measure (not the first, for a break).
        measure: MeasureIndex,
        /// Whether a line starts there.
        on: bool,
    },
    /// Limit how many bars a line of a track holds (`None`: as many as fit).
    SetBarsPerSystem {
        /// The track.
        track: TrackIndex,
        /// The limit, 1 to 32.
        bars: Option<u8>,
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
    /// Set how loud a beat's notes are played.
    SetDynamic {
        /// The beat.
        at: BeatPosition,
        /// The dynamic mark.
        dynamic: Dynamic,
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
    /// Tie every note of a beat to the note on its string in the previous beat, or remove
    /// those ties.
    SetBeatTie {
        /// The beat.
        at: BeatPosition,
        /// Whether the notes are tied.
        tie: bool,
    },
    /// Set the text written above a beat; an empty text removes it.
    SetBeatText {
        /// The beat.
        at: BeatPosition,
        /// The new text.
        text: String,
    },
    /// Set one lyric line: where its text starts and the text (an empty text removes it).
    SetLyricLine {
        /// The line, 0 to 4 (the first is 1 in the command line).
        line: u8,
        /// The measure the first syllable starts at.
        start: MeasureIndex,
        /// The syllables, separated by spaces.
        text: String,
    },
    /// Set the chord diagram shown at a beat; `None` removes it.
    SetChordDiagram {
        /// The beat.
        at: BeatPosition,
        /// The diagram, or `None` to remove it.
        diagram: Option<ChordDiagram>,
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
    /// Set a measure's repeat signs and alternative endings.
    SetRepeat {
        /// The measure.
        measure: MeasureIndex,
        /// The repeat signs and endings.
        repeat: Repeat,
    },
    /// Show a double barline at the end of a measure, or remove it.
    SetDoubleBar {
        /// The measure.
        measure: MeasureIndex,
        /// Whether the barline is doubled.
        double_bar: bool,
    },
    /// Set a measure's feel (triplet feel).
    SetFeel {
        /// The measure.
        measure: MeasureIndex,
        /// The feel.
        feel: Feel,
    },
    /// Set or clear a measure's fermata.
    SetFermata {
        /// The measure.
        measure: MeasureIndex,
        /// The fermata, or `None` to clear it.
        fermata: Option<Fermata>,
    },
    /// Set the clef of a measure's standard-notation staff in one track.
    SetClef {
        /// The track.
        track: TrackIndex,
        /// The measure.
        measure: MeasureIndex,
        /// The clef.
        clef: Clef,
    },
    /// Set or clear a measure's free time (no metronome).
    SetFreeTime {
        /// The measure.
        measure: MeasureIndex,
        /// Whether the measure is in free time.
        free_time: bool,
    },
    /// Set or clear a measure's bar-repetition (simile) mark.
    SetSimileMark {
        /// The track.
        track: TrackIndex,
        /// The measure.
        measure: MeasureIndex,
        /// The mark, or `None` to clear it.
        mark: Option<SimileMark>,
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
            // The staff is written only when it is not the track's first: a line without
            // one (from a journal written before staves were editable) means staff 1.
            let staff = match at.staff.get() {
                0 => String::new(),
                index => format!(" staff={}", one(index)),
            };
            format!(
                "track={}{staff} measure={} voice={} beat={}",
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
            Command::SetTrackSymbol { track, symbol } => {
                format!("{id} track={} symbol={}", one(track.get()), symbol.id())
            }
            Command::SetTrackSound { track, state } => format!(
                "{id} track={} state={}",
                one(track.get()),
                state
                    .as_deref()
                    .map_or_else(|| "none".to_owned(), crate::parse::encode_text)
            ),
            Command::SetSongSound { state } => format!(
                "{id} state={}",
                state
                    .as_deref()
                    .map_or_else(|| "none".to_owned(), crate::parse::encode_text)
            ),
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
            Command::SetPageSetup { setup } => format!(
                "{id} width={} height={} top={} right={} bottom={} left={} scale={} \
                 page_numbers={} copyright={} header={}",
                setup.width_mm,
                setup.height_mm,
                setup.margins.top,
                setup.margins.right,
                setup.margins.bottom,
                setup.margins.left,
                setup.scale_percent,
                setup.page_numbers,
                setup.copyright,
                setup.running_header
            ),
            Command::SetSystemBreak {
                track,
                measure,
                kind,
            } => format!(
                "{id} track={} measure={} break={}",
                one(track.get()),
                one(measure.get()),
                crate::parse::system_break_id(*kind)
            ),
            Command::SetScoreLineBreak { measure, on } => {
                format!("{id} measure={} on={on}", one(measure.get()))
            }
            Command::SetBarsPerSystem { track, bars } => format!(
                "{id} track={} bars={}",
                one(track.get()),
                bars.map_or_else(|| "auto".to_owned(), |bars| bars.to_string())
            ),
            Command::SetTempo { tempo } => format!("{id} tempo={tempo}"),
            Command::SetBeatRest { at: p } => format!("{id} {}", at(p)),
            Command::SetTie { at: p, string, tie } => {
                format!("{id} {} string={} tie={tie}", at(p), string.get())
            }
            Command::SetBeatTie { at: p, tie } => format!("{id} {} tie={tie}", at(p)),
            Command::SetBeatText { at: p, text } => {
                format!("{id} {} text={}", at(p), crate::parse::encode_text(text))
            }
            Command::SetLyricLine { line, start, text } => format!(
                "{id} line={} measure={} text={}",
                line.saturating_add(1),
                one(start.get()),
                crate::parse::encode_text(text)
            ),
            Command::SetChordDiagram { at: p, diagram } => match diagram {
                Some(diagram) => format!(
                    "{id} {} name={} base={} frets={}",
                    at(p),
                    crate::parse::encode_text(&diagram.name),
                    diagram.base_fret,
                    diagram
                        .frets
                        .iter()
                        .map(|(_, fret)| fret.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                None => format!("{id} {}", at(p)),
            },
            Command::SetDeadNote {
                at: p,
                string,
                dead,
            } => {
                format!("{id} {} string={} dead={dead}", at(p), string.get())
            }
            Command::SetDynamic { at: p, dynamic } => {
                format!("{id} {} dynamic={}", at(p), dynamic.id())
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
            Command::SetRepeat { measure, repeat } => format!(
                "{id} measure={} start={} end={} endings={}",
                one(measure.get()),
                repeat.start,
                repeat.end_repeats.unwrap_or(0),
                repeat.endings
            ),
            Command::SetDoubleBar {
                measure,
                double_bar,
            } => format!(
                "{id} measure={} double_bar={double_bar}",
                one(measure.get())
            ),
            Command::SetFeel { measure, feel } => {
                format!("{id} measure={} feel={}", one(measure.get()), feel.id())
            }
            Command::SetFermata { measure, fermata } => format!(
                "{id} measure={} fermata={}",
                one(measure.get()),
                fermata.map_or("none", Fermata::id)
            ),
            Command::SetClef {
                track,
                measure,
                clef,
            } => format!(
                "{id} track={} measure={} clef={}",
                one(track.get()),
                one(measure.get()),
                clef.id()
            ),
            Command::SetFreeTime { measure, free_time } => {
                format!("{id} measure={} free_time={free_time}", one(measure.get()))
            }
            Command::SetSimileMark {
                track,
                measure,
                mark,
            } => format!(
                "{id} track={} measure={} mark={}",
                one(track.get()),
                one(measure.get()),
                mark.map_or("none", SimileMark::id)
            ),
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
            Command::SetTrackSymbol { .. } => &SET_TRACK_SYMBOL,
            Command::SetTrackSound { .. } => &SET_TRACK_SOUND,
            Command::SetSongSound { .. } => &SET_SONG_SOUND,
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
            Command::SetPageSetup { .. } => &SET_PAGE_SETUP,
            Command::SetSystemBreak { .. } => &SET_SYSTEM_BREAK,
            Command::SetBarsPerSystem { .. } => &SET_BARS_PER_SYSTEM,
            Command::SetScoreLineBreak { .. } => &SET_SCORE_LINE_BREAK,
            Command::SetTempo { .. } => &SET_TEMPO,
            Command::SetBeatRest { .. } => &SET_BEAT_REST,
            Command::SetTie { .. } => &SET_TIE,
            Command::SetBeatTie { .. } => &SET_BEAT_TIE,
            Command::SetBeatText { .. } => &SET_BEAT_TEXT,
            Command::SetLyricLine { .. } => &SET_LYRIC_LINE,
            Command::SetChordDiagram { .. } => &SET_CHORD_DIAGRAM,
            Command::SetDeadNote { .. } => &SET_DEAD_NOTE,
            Command::SetDynamic { .. } => &SET_DYNAMIC,
            Command::SetMeasureTempo { .. } => &SET_MEASURE_TEMPO,
            Command::SetTimeSignature { .. } => &SET_TIME_SIGNATURE,
            Command::SetKeySignature { .. } => &SET_KEY_SIGNATURE,
            Command::SetSection { .. } => &SET_SECTION,
            Command::ClearSection { .. } => &CLEAR_SECTION,
            Command::SetRepeat { .. } => &SET_REPEAT,
            Command::SetDoubleBar { .. } => &SET_DOUBLE_BAR,
            Command::SetFeel { .. } => &SET_FEEL,
            Command::SetFermata { .. } => &SET_FERMATA,
            Command::SetClef { .. } => &SET_CLEF,
            Command::SetFreeTime { .. } => &SET_FREE_TIME,
            Command::SetSimileMark { .. } => &SET_SIMILE_MARK,
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
        if let Command::SetTrackSound { track, state } = self {
            let track = track.get().saturating_add(1);
            return match state {
                Some(state) => format!("track {track}, sound state ({} bytes)", state.len()),
                None => format!("track {track}, sound state cleared"),
            };
        }
        if let Command::SetSongSound { state } = self {
            return match state {
                Some(state) => format!("song sound state ({} bytes)", state.len()),
                None => "song sound state cleared".to_owned(),
            };
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
            | Command::ClearSection { .. }
            | Command::SetRepeat { .. }
            | Command::SetDoubleBar { .. }
            | Command::SetFeel { .. }
            | Command::SetFermata { .. }
            | Command::SetClef { .. }
            | Command::SetFreeTime { .. }
            | Command::SetSimileMark { .. }
            | Command::SetLyricLine { .. }
            | Command::SetChordDiagram { .. } => true,
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
            | Command::SetDeadNote { .. }
            | Command::SetBeatTie { .. }
            | Command::SetDynamic { .. }
            | Command::SetRepeat { .. }
            | Command::SetFeel { .. }
            | Command::SetFermata { .. }
            | Command::SetFreeTime { .. } => true,
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
            | Command::SetTrackSymbol { .. }
            | Command::SetSongText { .. }
            | Command::SetPageSetup { .. }
            | Command::SetSystemBreak { .. }
            | Command::SetBarsPerSystem { .. }
            | Command::SetScoreLineBreak { .. }
            | Command::SetTrackStaves { .. }
            | Command::SetSection { .. }
            | Command::ClearSection { .. }
            | Command::SetDoubleBar { .. }
            | Command::SetKeySignature { .. }
            | Command::SetClef { .. }
            | Command::SetSimileMark { .. } => true,
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
            // A track's sound state changes continuously while a knob or slider is dragged.
            (
                Command::SetTrackSound { track: first, .. },
                Command::SetTrackSound { track: second, .. },
            ) => first == second,
            // So does the song's own (a bus's level or a knob in its window).
            (Command::SetSongSound { .. }, Command::SetSongSound { .. }) => true,
            (Command::SetMasterVolume { .. }, Command::SetMasterVolume { .. })
            | (Command::SetMasterPan { .. }, Command::SetMasterPan { .. }) => true,
            // One Apply in the settings window is one undo step for the fields it changed.
            (
                Command::SetSongText { .. } | Command::SetTempo { .. },
                Command::SetSongText { .. } | Command::SetTempo { .. },
            ) => true,
            // Typing in the lyrics dialog (shown in the score as it is typed) is one step
            // per line.
            (
                Command::SetLyricLine { line: first, .. },
                Command::SetLyricLine { line: second, .. },
            ) => first == second,
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
                .is_some_and(|beat| beat.kind() != BeatKind::Empty && !beat.is_grace()),
            Command::InsertBeat { at, .. } => {
                // `apply` creates voice 1 of a measure that has no voices (GP7 empty
                // bars import that way), so the first beat of such a measure is editable.
                let Some(measure) = song.measure_at(*at) else {
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
            | Command::SetTrackSymbol { track, .. }
            | Command::SetTrackSound { track, .. }
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
            Command::SetSongSound { .. } => true,
            Command::SetPageSetup { setup } => setup.check().is_ok(),
            Command::SetSystemBreak {
                track,
                measure,
                kind,
            } => {
                track.get() < song.track_count()
                    && measure.get() < song.measure_count()
                    && (measure.get() > 0 || *kind == SystemBreak::None)
            }
            Command::SetScoreLineBreak { measure, on } => {
                measure.get() < song.measure_count() && (measure.get() > 0 || !*on)
            }
            Command::SetBarsPerSystem { track, bars } => {
                track.get() < song.track_count()
                    && bars.is_none_or(|bars| (1..=MAX_BARS_PER_SYSTEM).contains(&bars))
            }
            Command::SetTempo { tempo } => (1..=MAX_TEMPO).contains(tempo),
            Command::SetBeatRest { at } => song.beat(*at).is_some(),
            Command::SetDynamic { at, .. } => song
                .beat(*at)
                .is_some_and(|beat| beat.notes().next().is_some()),
            Command::SetTie { at, string, .. } | Command::SetDeadNote { at, string, .. } => song
                .beat(*at)
                .is_some_and(|beat| beat.notes().any(|note| note.string().ok() == Some(*string))),
            Command::SetBeatTie { at, .. } => song.beat_tie_state(*at).is_some(),
            Command::SetBeatText { at, text } => {
                song.beat(*at).is_some()
                    && text.chars().count() <= gscore_model::MAX_BEAT_TEXT_CHARS
            }
            Command::SetLyricLine { line, start, text } => {
                *line < gscore_model::LYRIC_LINES
                    && text.chars().count() <= gscore_model::MAX_LYRIC_LINE_CHARS
                    && (text.is_empty() || start.get() < song.measure_count())
            }
            Command::SetChordDiagram { at, diagram } => {
                song.beat(*at).is_some() && diagram.as_ref().is_none_or(|d| d.validate().is_ok())
            }
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
            Command::SetRepeat { measure, repeat } => {
                measure.get() < song.measure_count()
                    && repeat
                        .end_repeats
                        .is_none_or(|repeats| (1..=127).contains(&repeats))
            }
            Command::SetDoubleBar { measure, .. } => measure.get() < song.measure_count(),
            Command::SetFeel { measure, .. }
            | Command::SetFermata { measure, .. }
            | Command::SetFreeTime { measure, .. } => measure.get() < song.measure_count(),
            Command::SetClef {
                track,
                measure,
                clef: _,
            }
            | Command::SetSimileMark {
                track,
                measure,
                mark: _,
            } => song.track(*track).is_some() && measure.get() < song.measure_count(),
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
            Command::SetTrackSymbol { track, symbol } => {
                song.set_track_symbol(*track, Some(*symbol))
            }
            Command::SetTrackSound { track, state } => {
                song.set_track_sound_state(*track, state.clone())
            }
            Command::SetSongSound { state } => song.set_sound_state(state.clone()),
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
            Command::SetPageSetup { setup } => song.set_page_setup(*setup),
            Command::SetSystemBreak {
                track,
                measure,
                kind,
            } => song.set_system_break(*track, *measure, *kind),
            Command::SetBarsPerSystem { track, bars } => song.set_bars_per_system(*track, *bars),
            Command::SetScoreLineBreak { measure, on } => song.set_score_line_break(*measure, *on),
            Command::SetTempo { tempo } => song.set_tempo(*tempo),
            Command::SetBeatRest { at } => song.set_beat_rest(*at),
            Command::SetTie { at, string, tie } => song.set_note_tie(*at, *string, *tie),
            Command::SetBeatTie { at, tie } => song.set_beat_tie(*at, *tie),
            Command::SetBeatText { at, text } => song.set_beat_text(*at, text),
            Command::SetLyricLine { line, start, text } => song.set_lyric_line(*line, *start, text),
            Command::SetChordDiagram { at, diagram } => {
                song.set_chord_diagram(*at, diagram.as_ref())
            }
            Command::SetDeadNote { at, string, dead } => song.set_note_dead(*at, *string, *dead),
            Command::SetDynamic { at, dynamic } => song.set_dynamic(*at, *dynamic),
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
            Command::SetRepeat { measure, repeat } => song.set_repeat(*measure, *repeat),
            Command::SetDoubleBar {
                measure,
                double_bar,
            } => song.set_double_bar(*measure, *double_bar),
            Command::SetFeel { measure, feel } => song.set_feel(*measure, *feel),
            Command::SetFermata { measure, fermata } => song.set_fermata(*measure, *fermata),
            Command::SetClef {
                track,
                measure,
                clef,
            } => song.set_clef(*track, *measure, *clef),
            Command::SetFreeTime { measure, free_time } => song.set_free_time(*measure, *free_time),
            Command::SetSimileMark {
                track,
                measure,
                mark,
            } => song.set_simile_mark(*track, *measure, *mark),
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
    let track = song.staff(at.track, at.staff)?;
    if usize::from(string.get()) > track.string_count() {
        return None;
    }
    // Grace notes are ornaments, not notes to edit on their own.
    song.beat(at).filter(|beat| !beat.is_grace())
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
