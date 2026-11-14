//! Interactive actions: what a key press (or a menu entry) asks the editor to do. Unlike a
//! [`crate::Command`], an action has no explicit position; it works at the cursor. One action
//! may run one model command, move the cursor, or ask the application for something
//! (play, save) that the editor itself cannot do.
//!
//! The ids are the ones the keymap file binds keys to (AGENTS.md section 7).

use std::fmt;

/// What an action id stands for. `EnterFret` takes its digit from the key that was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionId {
    /// Move to the previous beat (into the previous measure at its start).
    CursorLeft,
    /// Move to the next beat; at the end of an unfinished measure, add a rest beat; at the
    /// end of the last measure, add a measure.
    CursorRight,
    /// Move to the next higher string (towards string 1).
    CursorUpString,
    /// Move to the next lower string.
    CursorDownString,
    /// Move to the first beat of the previous measure.
    PreviousMeasure,
    /// Move to the first beat of the next measure.
    NextMeasure,
    /// Move to the first beat of the track (the start of the song).
    CursorHome,
    /// Move to the last beat of the track's last measure (its start when it is empty).
    CursorEnd,
    /// Extend the beat selection to the first beat of the track.
    SelectHome,
    /// Extend the beat selection to the last beat of the track.
    SelectEnd,
    /// Type a fret digit on the cursor's string; a second digit soon after makes two-digit
    /// frets.
    EnterFret,
    /// Remove the note on the cursor's string.
    ClearNote,
    /// Halve the beat's note value (a quarter becomes an eighth).
    DurationShorter,
    /// Double the beat's note value (a quarter becomes a half).
    DurationLonger,
    /// Insert a rest beat before the cursor.
    InsertBeat,
    /// Delete what the cursor is on: the note on its string, or the whole beat when that
    /// string has none.
    Delete,
    /// Remove the beat at the cursor.
    DeleteBeat,
    /// Insert a measure before the cursor's measure, in every track.
    InsertMeasure,
    /// Remove the cursor's measure from every track.
    DeleteMeasure,
    /// Copy the selection.
    Copy,
    /// Cut the selection.
    Cut,
    /// Paste at the cursor.
    Paste,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Start or pause playback.
    PlayPause,
    /// Stop playback.
    Stop,
    /// Save the file.
    Save,
    /// Extend the beat selection one beat to the left (the cursor moves with it).
    SelectLeft,
    /// Extend the beat selection one beat to the right (the cursor moves with it).
    SelectRight,
    /// Select the cursor's whole beat (every string).
    SelectBeat,
    /// Extend the beat selection to the line above: the beat at the same place on the
    /// system before (the page's layout decides; see [`crate::Request::SelectLine`]).
    SelectLineUp,
    /// Extend the beat selection to the line below.
    SelectLineDown,
    /// Palette: Whole note.
    NoteWhole,
    /// Palette: Half note.
    NoteHalf,
    /// Palette: Quarter note.
    NoteQuarter,
    /// Palette: Eighth note.
    NoteEighth,
    /// Palette: 16th note.
    NoteSixteenth,
    /// Palette: 32nd note.
    NoteThirtySecond,
    /// Palette: 64th note.
    NoteSixtyFourth,
    /// Palette: Dotted.
    ToggleDot,
    /// Palette: Double dotted.
    ToggleDoubleDot,
    /// Palette: Triplet.
    TupletThree,
    /// Palette: Quintuplet (5:4).
    TupletFive,
    /// Palette: Septuplet (7:4).
    TupletSeven,
    /// Palette: Sextuplet (6:4).
    TupletSix,
    /// Palette: any tuplet (opens the tuplet dialog).
    TupletOther,
    /// Palette: Rest.
    MakeRest,
    /// Palette: Tie.
    ToggleTie,
    /// Palette: tie the whole beat to the previous one.
    TieBeat,
    /// Palette: text written above the beat (opens the beat text dialog).
    EditBeatText,
    /// Palette: lyrics (opens the lyrics dialog).
    EditLyrics,
    /// Palette: chord diagram (opens the chord diagram dialog).
    EditChordDiagram,
    /// Palette: left-hand fingering (opens the fingering dialog).
    EditLeftFinger,
    /// Palette: right-hand fingering (opens the fingering dialog).
    EditRightFinger,
    /// Palette: Dead note.
    ToggleDeadNote,
    /// Palette: Ghost note.
    ToggleGhostNote,
    /// Palette: Hammer-on / pull-off.
    ToggleHammer,
    /// Palette: Shift slide.
    ToggleShiftSlide,
    /// Palette: Legato slide.
    ToggleLegatoSlide,
    /// Palette: Slide in from below.
    ToggleSlideInBelow,
    /// Palette: Slide in from above.
    ToggleSlideInAbove,
    /// Palette: Slide out downwards.
    ToggleSlideOutDown,
    /// Palette: Slide out upwards.
    ToggleSlideOutUp,
    /// Palette: Bend.
    ToggleBend,
    /// Palette: Vibrato.
    ToggleVibrato,
    /// Palette: Wide vibrato.
    ToggleWideVibrato,
    /// Palette: Accent.
    ToggleAccent,
    /// Palette: Heavy accent.
    ToggleHeavyAccent,
    /// Palette: Staccato.
    ToggleStaccato,
    /// Palette: Palm mute.
    TogglePalmMute,
    /// Palette: Let ring.
    ToggleLetRing,
    /// Palette: Trill.
    ToggleTrill,
    /// Palette: Tremolo picking.
    ToggleTremoloPicking,
    /// Palette: Grace note (opens the grace note dialog).
    GraceNote,
    /// Palette: Natural harmonic.
    ToggleNaturalHarmonic,
    /// Palette: Artificial harmonic.
    ToggleArtificialHarmonic,
    /// Palette: Tapped harmonic.
    ToggleTappedHarmonic,
    /// Palette: Pinch harmonic.
    TogglePinchHarmonic,
    /// Palette: Semi harmonic.
    ToggleSemiHarmonic,
    /// Palette: Pick downstroke.
    TogglePickDown,
    /// Palette: Pick upstroke.
    TogglePickUp,
    /// Palette: Brush (strum) down.
    ToggleBrushDown,
    /// Palette: Brush (strum) up.
    ToggleBrushUp,
    /// Palette: Tapping.
    ToggleTapping,
    /// Palette: Slap.
    ToggleSlapping,
    /// Palette: Pop.
    TogglePopping,
    /// Palette: Fade in.
    ToggleFadeIn,
    /// Palette: Fade out.
    ToggleFadeOut,
    /// Palette: Volume swell (in and out again).
    ToggleVolumeSwell,
    /// Palette: 8va (an octave higher).
    ToggleOttava,
    /// Palette: 8vb (an octave lower).
    ToggleOttavaBassa,
    /// Palette: 15ma (two octaves higher).
    ToggleQuindicesima,
    /// Palette: 15mb (two octaves lower).
    ToggleQuindicesimaBassa,
    /// Palette: Whammy bar.
    ToggleWhammy,
    /// Palette: Legato (beat slur).
    ToggleLegato,
    /// Palette: Beat vibrato.
    ToggleBeatVibrato,
    /// Palette: Arpeggio (rolled chord) down.
    ToggleArpeggioDown,
    /// Palette: Arpeggio (rolled chord) up.
    ToggleArpeggioUp,
    /// Palette: Dead slapped beat.
    ToggleDeadSlapped,
    /// Palette: Rasgueado.
    ToggleRasgueado,
    /// Palette: Left-hand tap.
    ToggleLeftHandTap,
    /// Palette: Circled string number.
    ToggleStringNumber,
    /// Palette: Pick slide down.
    TogglePickSlideDown,
    /// Palette: Pick slide up.
    TogglePickSlideUp,
    /// Palette: Trill with a fret and speed (opens the trill dialog).
    TrillOther,
    /// Palette: Tremolo picking at a chosen speed (opens the tremolo dialog).
    TremoloOther,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Back to actual size.
    ZoomReset,
    /// Switch the window to full screen and back.
    ToggleFullscreen,
    /// Show the mixer in the bottom panel instead of the measure grid, and back.
    ToggleMixer,
    /// Play the song a semitone higher (the written notes stay).
    TransposeUpSemitone,
    /// Play the song a semitone lower (the written notes stay).
    TransposeDownSemitone,
    /// Play the song an octave higher (the written notes stay).
    TransposeUpOctave,
    /// Play the song an octave lower (the written notes stay).
    TransposeDownOctave,
    /// Switch the song's transposition on or off (a listening aid).
    ToggleTransposition,
    /// Show every track of the song together, or only the cursor's track again.
    ToggleMultitrack,
    /// Open the edit history window.
    ShowHistory,
    /// Start a new song.
    NewSong,
    /// Open a file.
    OpenFile,
    /// Save under another name.
    SaveAs,
    /// Open the settings window.
    OpenSettings,
    /// Quit the app (asking about unsaved changes first).
    Quit,
}

impl ActionId {
    /// True for the edit palette's actions (note values, rests, ties, effects). The app runs
    /// them like a click on their palette tile; the editor itself does nothing for them.
    pub fn is_palette(self) -> bool {
        matches!(
            self,
            ActionId::NoteWhole
                | ActionId::NoteHalf
                | ActionId::NoteQuarter
                | ActionId::NoteEighth
                | ActionId::NoteSixteenth
                | ActionId::NoteThirtySecond
                | ActionId::NoteSixtyFourth
                | ActionId::ToggleDot
                | ActionId::ToggleDoubleDot
                | ActionId::TupletThree
                | ActionId::TupletFive
                | ActionId::TupletSeven
                | ActionId::TupletSix
                | ActionId::TupletOther
                | ActionId::MakeRest
                | ActionId::ToggleTie
                | ActionId::TieBeat
                | ActionId::EditBeatText
                | ActionId::EditLyrics
                | ActionId::EditChordDiagram
                | ActionId::EditLeftFinger
                | ActionId::EditRightFinger
                | ActionId::ToggleDeadNote
                | ActionId::ToggleGhostNote
                | ActionId::ToggleHammer
                | ActionId::ToggleShiftSlide
                | ActionId::ToggleLegatoSlide
                | ActionId::ToggleSlideInBelow
                | ActionId::ToggleSlideInAbove
                | ActionId::ToggleSlideOutDown
                | ActionId::ToggleSlideOutUp
                | ActionId::ToggleBend
                | ActionId::ToggleVibrato
                | ActionId::ToggleWideVibrato
                | ActionId::ToggleAccent
                | ActionId::ToggleHeavyAccent
                | ActionId::ToggleStaccato
                | ActionId::TogglePalmMute
                | ActionId::ToggleLetRing
                | ActionId::ToggleTrill
                | ActionId::ToggleTremoloPicking
                | ActionId::GraceNote
                | ActionId::ToggleNaturalHarmonic
                | ActionId::ToggleArtificialHarmonic
                | ActionId::ToggleTappedHarmonic
                | ActionId::TogglePinchHarmonic
                | ActionId::ToggleSemiHarmonic
                | ActionId::TogglePickDown
                | ActionId::TogglePickUp
                | ActionId::ToggleBrushDown
                | ActionId::ToggleBrushUp
                | ActionId::ToggleTapping
                | ActionId::ToggleSlapping
                | ActionId::TogglePopping
                | ActionId::ToggleFadeIn
                | ActionId::ToggleFadeOut
                | ActionId::ToggleVolumeSwell
                | ActionId::ToggleOttava
                | ActionId::ToggleOttavaBassa
                | ActionId::ToggleQuindicesima
                | ActionId::ToggleQuindicesimaBassa
                | ActionId::ToggleWhammy
                | ActionId::ToggleLegato
                | ActionId::ToggleBeatVibrato
                | ActionId::ToggleArpeggioDown
                | ActionId::ToggleArpeggioUp
                | ActionId::ToggleDeadSlapped
                | ActionId::ToggleRasgueado
                | ActionId::ToggleLeftHandTap
                | ActionId::ToggleStringNumber
                | ActionId::TogglePickSlideDown
                | ActionId::TogglePickSlideUp
                | ActionId::TrillOther
                | ActionId::TremoloOther
        )
    }
}

/// An action id with its text form and a label for menus and help.
#[derive(Debug, Clone, Copy)]
pub struct ActionInfo {
    /// The action.
    pub action: ActionId,
    /// The id the keymap file uses.
    pub id: &'static str,
    /// A short, imperative label.
    pub label: &'static str,
}

/// Every action, in the order help lists them.
pub const ACTIONS: &[ActionInfo] = &[
    info(ActionId::CursorLeft, "cursor_left", "Previous beat"),
    info(ActionId::CursorRight, "cursor_right", "Next beat"),
    info(
        ActionId::CursorUpString,
        "cursor_up_string",
        "Higher string",
    ),
    info(
        ActionId::CursorDownString,
        "cursor_down_string",
        "Lower string",
    ),
    info(
        ActionId::PreviousMeasure,
        "previous_measure",
        "Previous measure",
    ),
    info(ActionId::NextMeasure, "next_measure", "Next measure"),
    info(ActionId::CursorHome, "cursor_home", "Start of track"),
    info(ActionId::CursorEnd, "cursor_end", "End of track"),
    info(ActionId::SelectHome, "select_home", "Select to start"),
    info(ActionId::SelectEnd, "select_end", "Select to end"),
    info(ActionId::EnterFret, "enter_fret", "Enter fret"),
    info(ActionId::ClearNote, "clear_note", "Clear note"),
    info(
        ActionId::DurationShorter,
        "duration_shorter",
        "Shorter note",
    ),
    info(ActionId::DurationLonger, "duration_longer", "Longer note"),
    info(ActionId::InsertBeat, "insert_beat", "Insert beat"),
    info(ActionId::Delete, "delete", "Delete note or beat"),
    info(ActionId::DeleteBeat, "delete_beat", "Delete beat"),
    info(ActionId::InsertMeasure, "insert_measure", "Insert measure"),
    info(ActionId::DeleteMeasure, "delete_measure", "Delete measure"),
    info(ActionId::Copy, "copy", "Copy"),
    info(ActionId::Cut, "cut", "Cut"),
    info(ActionId::Paste, "paste", "Paste"),
    info(ActionId::Undo, "undo", "Undo"),
    info(ActionId::Redo, "redo", "Redo"),
    info(ActionId::PlayPause, "play_pause", "Play/pause"),
    info(ActionId::Stop, "stop", "Stop"),
    info(ActionId::Save, "save", "Save"),
    info(ActionId::SelectLeft, "select_left", "Extend selection left"),
    info(
        ActionId::SelectRight,
        "select_right",
        "Extend selection right",
    ),
    info(ActionId::SelectBeat, "select_beat", "Select whole beat"),
    info(
        ActionId::SelectLineUp,
        "select_line_up",
        "Extend selection a line up",
    ),
    info(
        ActionId::SelectLineDown,
        "select_line_down",
        "Extend selection a line down",
    ),
    info(ActionId::NoteWhole, "note_whole", "Whole note"),
    info(ActionId::NoteHalf, "note_half", "Half note"),
    info(ActionId::NoteQuarter, "note_quarter", "Quarter note"),
    info(ActionId::NoteEighth, "note_eighth", "Eighth note"),
    info(ActionId::NoteSixteenth, "note_sixteenth", "16th note"),
    info(
        ActionId::NoteThirtySecond,
        "note_thirty_second",
        "32nd note",
    ),
    info(ActionId::NoteSixtyFourth, "note_sixty_fourth", "64th note"),
    info(ActionId::ToggleDot, "toggle_dot", "Dotted"),
    info(
        ActionId::ToggleDoubleDot,
        "toggle_double_dot",
        "Double dotted",
    ),
    info(ActionId::TupletThree, "tuplet_three", "Triplet"),
    info(ActionId::TupletFive, "tuplet_five", "Quintuplet (5:4)"),
    info(ActionId::TupletSeven, "tuplet_seven", "Septuplet (7:4)"),
    info(ActionId::TupletSix, "tuplet_six", "Sextuplet (6:4)"),
    info(ActionId::TupletOther, "tuplet_other", "Tuplet…"),
    info(ActionId::MakeRest, "make_rest", "Rest"),
    info(ActionId::ToggleTie, "toggle_tie", "Tie"),
    info(ActionId::TieBeat, "tie_beat", "Tie beat"),
    info(ActionId::EditBeatText, "edit_beat_text", "Beat text…"),
    info(ActionId::EditLyrics, "edit_lyrics", "Lyrics…"),
    info(
        ActionId::EditChordDiagram,
        "edit_chord_diagram",
        "Chord diagram…",
    ),
    info(
        ActionId::EditLeftFinger,
        "edit_left_finger",
        "Left-hand fingering…",
    ),
    info(
        ActionId::EditRightFinger,
        "edit_right_finger",
        "Right-hand fingering…",
    ),
    info(ActionId::ToggleDeadNote, "toggle_dead_note", "Dead note"),
    info(ActionId::ToggleGhostNote, "toggle_ghost_note", "Ghost note"),
    info(
        ActionId::ToggleHammer,
        "toggle_hammer",
        "Hammer-on / pull-off",
    ),
    info(
        ActionId::ToggleShiftSlide,
        "toggle_shift_slide",
        "Shift slide",
    ),
    info(
        ActionId::ToggleLegatoSlide,
        "toggle_legato_slide",
        "Legato slide",
    ),
    info(
        ActionId::ToggleSlideInBelow,
        "toggle_slide_in_below",
        "Slide in from below",
    ),
    info(
        ActionId::ToggleSlideInAbove,
        "toggle_slide_in_above",
        "Slide in from above",
    ),
    info(
        ActionId::ToggleSlideOutDown,
        "toggle_slide_out_down",
        "Slide out downwards",
    ),
    info(
        ActionId::ToggleSlideOutUp,
        "toggle_slide_out_up",
        "Slide out upwards",
    ),
    info(ActionId::ToggleBend, "toggle_bend", "Bend"),
    info(ActionId::ToggleVibrato, "toggle_vibrato", "Vibrato"),
    info(
        ActionId::ToggleWideVibrato,
        "toggle_wide_vibrato",
        "Wide vibrato",
    ),
    info(ActionId::ToggleAccent, "toggle_accent", "Accent"),
    info(
        ActionId::ToggleHeavyAccent,
        "toggle_heavy_accent",
        "Heavy accent",
    ),
    info(ActionId::ToggleStaccato, "toggle_staccato", "Staccato"),
    info(ActionId::TogglePalmMute, "toggle_palm_mute", "Palm mute"),
    info(ActionId::ToggleLetRing, "toggle_let_ring", "Let ring"),
    info(ActionId::ToggleTrill, "toggle_trill", "Trill"),
    info(
        ActionId::ToggleTremoloPicking,
        "toggle_tremolo_picking",
        "Tremolo picking",
    ),
    info(ActionId::GraceNote, "grace_note", "Grace note"),
    info(
        ActionId::ToggleNaturalHarmonic,
        "toggle_natural_harmonic",
        "Natural harmonic",
    ),
    info(
        ActionId::ToggleArtificialHarmonic,
        "toggle_artificial_harmonic",
        "Artificial harmonic",
    ),
    info(
        ActionId::ToggleTappedHarmonic,
        "toggle_tapped_harmonic",
        "Tapped harmonic",
    ),
    info(
        ActionId::TogglePinchHarmonic,
        "toggle_pinch_harmonic",
        "Pinch harmonic",
    ),
    info(
        ActionId::ToggleSemiHarmonic,
        "toggle_semi_harmonic",
        "Semi harmonic",
    ),
    info(
        ActionId::TogglePickDown,
        "toggle_pick_down",
        "Pick downstroke",
    ),
    info(ActionId::TogglePickUp, "toggle_pick_up", "Pick upstroke"),
    info(
        ActionId::ToggleBrushDown,
        "toggle_brush_down",
        "Brush (strum) down",
    ),
    info(
        ActionId::ToggleBrushUp,
        "toggle_brush_up",
        "Brush (strum) up",
    ),
    info(ActionId::ToggleTapping, "toggle_tapping", "Tapping"),
    info(ActionId::ToggleSlapping, "toggle_slapping", "Slap"),
    info(ActionId::TogglePopping, "toggle_popping", "Pop"),
    info(ActionId::ToggleFadeIn, "toggle_fade_in", "Fade in"),
    info(ActionId::ToggleFadeOut, "toggle_fade_out", "Fade out"),
    info(
        ActionId::ToggleVolumeSwell,
        "toggle_volume_swell",
        "Volume swell",
    ),
    info(ActionId::ToggleOttava, "toggle_8va", "8va (an octave up)"),
    info(
        ActionId::ToggleOttavaBassa,
        "toggle_8vb",
        "8vb (an octave down)",
    ),
    info(
        ActionId::ToggleQuindicesima,
        "toggle_15ma",
        "15ma (two octaves up)",
    ),
    info(
        ActionId::ToggleQuindicesimaBassa,
        "toggle_15mb",
        "15mb (two octaves down)",
    ),
    info(ActionId::ToggleWhammy, "toggle_whammy", "Whammy bar"),
    info(
        ActionId::ToggleLegato,
        "toggle_legato",
        "Legato (beat slur)",
    ),
    info(
        ActionId::ToggleBeatVibrato,
        "toggle_beat_vibrato",
        "Beat vibrato",
    ),
    info(
        ActionId::ToggleArpeggioDown,
        "toggle_arpeggio_down",
        "Arpeggio down",
    ),
    info(
        ActionId::ToggleArpeggioUp,
        "toggle_arpeggio_up",
        "Arpeggio up",
    ),
    info(
        ActionId::ToggleDeadSlapped,
        "toggle_dead_slapped",
        "Dead slapped",
    ),
    info(ActionId::ToggleRasgueado, "toggle_rasgueado", "Rasgueado"),
    info(
        ActionId::ToggleLeftHandTap,
        "toggle_left_hand_tap",
        "Left-hand tap",
    ),
    info(
        ActionId::ToggleStringNumber,
        "toggle_string_number",
        "String number",
    ),
    info(
        ActionId::TogglePickSlideDown,
        "toggle_pick_slide_down",
        "Pick slide down",
    ),
    info(
        ActionId::TogglePickSlideUp,
        "toggle_pick_slide_up",
        "Pick slide up",
    ),
    info(ActionId::TrillOther, "trill_other", "Trill…"),
    info(ActionId::TremoloOther, "tremolo_other", "Tremolo…"),
    info(ActionId::ZoomIn, "zoom_in", "Zoom in"),
    info(ActionId::ZoomOut, "zoom_out", "Zoom out"),
    info(ActionId::ZoomReset, "zoom_reset", "Actual size"),
    info(
        ActionId::ToggleFullscreen,
        "toggle_fullscreen",
        "Full screen",
    ),
    info(ActionId::ToggleMixer, "toggle_mixer", "Mixer"),
    info(
        ActionId::TransposeUpSemitone,
        "transpose_up_semitone",
        "Transpose up a semitone",
    ),
    info(
        ActionId::TransposeDownSemitone,
        "transpose_down_semitone",
        "Transpose down a semitone",
    ),
    info(
        ActionId::TransposeUpOctave,
        "transpose_up_octave",
        "Transpose up an octave",
    ),
    info(
        ActionId::TransposeDownOctave,
        "transpose_down_octave",
        "Transpose down an octave",
    ),
    info(
        ActionId::ToggleTransposition,
        "toggle_transposition",
        "Transposition on/off",
    ),
    info(
        ActionId::ToggleMultitrack,
        "toggle_multitrack",
        "All tracks",
    ),
    info(ActionId::ShowHistory, "show_history", "History…"),
    info(ActionId::NewSong, "new_song", "New"),
    info(ActionId::OpenFile, "open_file", "Open…"),
    info(ActionId::SaveAs, "save_as", "Save As…"),
    info(ActionId::OpenSettings, "open_settings", "Settings…"),
    info(ActionId::Quit, "quit", "Quit"),
];

const fn info(action: ActionId, id: &'static str, label: &'static str) -> ActionInfo {
    ActionInfo { action, id, label }
}

impl ActionId {
    /// The action with keymap id `id`, if there is one.
    pub fn from_id(id: &str) -> Option<ActionId> {
        ACTIONS.iter().find(|a| a.id == id).map(|a| a.action)
    }

    /// This action's metadata.
    pub fn info(self) -> &'static ActionInfo {
        // Every variant has an entry (tested); the fallback is never used.
        static UNKNOWN: ActionInfo = info(ActionId::EnterFret, "?", "?");
        ACTIONS
            .iter()
            .find(|a| a.action == self)
            .unwrap_or(&UNKNOWN)
    }

    /// The keymap id.
    pub fn id(self) -> &'static str {
        self.info().id
    }

    /// The label.
    pub fn label(self) -> &'static str {
        self.info().label
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// An action ready to run: the id plus what the key supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Any action without a parameter.
    Plain(ActionId),
    /// A fret digit (0-9).
    Digit(u8),
}

impl Action {
    /// The action's id.
    pub fn id(self) -> ActionId {
        match self {
            Action::Plain(id) => id,
            Action::Digit(_) => ActionId::EnterFret,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_round_trip() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert_eq!(ActionId::from_id(a.id), Some(a.action));
            assert_eq!(a.action.id(), a.id);
            assert!(
                ACTIONS
                    .iter()
                    .skip(i + 1)
                    .all(|b| b.id != a.id && b.action != a.action)
            );
        }
        assert_eq!(ActionId::from_id("no_such_action"), None);
        assert_eq!(ACTIONS.len(), 123, "one entry per ActionId variant");
    }
}
