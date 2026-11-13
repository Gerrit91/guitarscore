//! The edit palette (the left panel): note lengths, dots and tuplets, rests and ties, the
//! voice, dynamics, the effects matrix, and the measure's tempo, signatures and section
//! marker.
//!
//! Changes are ordinary commands (or one `batch` command for several beats), so every
//! palette click is undoable and recorded in the recovery journal.

use super::shape_editor::Outcome;
use super::tuplet_dialog::TupletDialog;
use gscore_edit::{ActionId, Command, Cursor, Selection};
use gscore_model::{
    BeatIndex, BeatPosition, Feel, Fermata, Finger, Hand, MeasureIndex, NoteValue, Repeat, Song,
    SystemBreak, VoiceIndex,
};

use super::GuitarScoreApp;
use super::edit_dynamics::{self, DynamicsSnapshot};
use super::edit_effects::{BeatState, EffectsState, LyricsPreview, NoteState};
use super::edit_staff::{self, StaffState};
use super::style;
use super::tiles::{self, Face};
use super::toolbar::key_hint;

/// Note lengths offered, as the `1/value` of a whole note.
const NOTE_VALUES: [u16; 7] = [1, 2, 4, 8, 16, 32, 64];
/// Tuplets with a tile of their own: "in the time of". Every other one is chosen in the
/// tuplet dialog.
const TUPLETS: [(u8, u8); 2] = [(3, 2), (6, 4)];

/// What the palette keeps between frames.
#[derive(Debug)]
pub(super) struct EditPaletteState {
    /// The chosen note value (`1/value`).
    value: u16,
    /// Augmentation dots (0 to 2).
    dots: u8,
    /// The chosen tuplet, if any.
    tuplet: Option<(u8, u8)>,
    /// The tuplet dialog, while it is open.
    tuplet_dialog: Option<TupletDialog>,
    /// The staff tiles and the dialog one of them opened.
    pub(super) staff: StaffState,
    /// A voice chosen this frame, for the caller to move the cursor.
    pending_voice: Option<usize>,
    /// The bend and whammy editors and the effects shown.
    effects: EffectsState,
}

impl Default for EditPaletteState {
    fn default() -> Self {
        EditPaletteState {
            value: 4,
            dots: 0,
            tuplet: None,
            tuplet_dialog: None,
            staff: StaffState::default(),
            pending_voice: None,
            effects: EffectsState::default(),
        }
    }
}

impl EditPaletteState {
    fn duration(&self) -> Option<NoteValue> {
        NoteValue::new(self.value, self.dots, self.tuplet).ok()
    }

    /// The hand whose fingering window is open, if one is.
    pub(super) fn finger_hand(&self) -> Option<Hand> {
        self.effects.finger_hand()
    }

    /// A finger chosen with a key while a fingering window is open.
    pub(super) fn choose_finger(&mut self, finger: Option<Finger>) {
        self.effects.choose_finger(finger);
    }

    /// Whether an open editor takes the keys (the fingering windows do not).
    pub(super) fn takes_keys(&self) -> bool {
        self.effects.takes_keys()
    }
}

/// The song data the palette shows, taken before the widgets run.
pub(super) struct PaletteSnapshot {
    pub(super) cursor: Cursor,
    pub(super) beats: Vec<BeatPosition>,
    /// Each beat's written note value, in the order of `beats` (`None` for a missing beat).
    /// Dot and tuplet choices change this value instead of the palette's, so toggling a
    /// tuplet on and off leaves the notes as they were.
    pub(super) beat_values: Vec<Option<NoteValue>>,
    /// The selected beats without notes (rests and empty beats).
    pub(super) silent_beats: Vec<BeatPosition>,
    /// The target beats that are rests.
    pub(super) rest_beats: Vec<BeatPosition>,
    /// Whether the last edit made the target beats rests (from the rest tile), so another
    /// click on it undoes that and brings their notes back.
    pub(super) rest_was_last: bool,
    pub(super) measure: MeasureIndex,
    pub(super) tempo: Option<u32>,
    pub(super) time_signature: Option<gscore_model::TimeSignature>,
    pub(super) key: Option<gscore_model::KeySignature>,
    /// The clef of the cursor's measure (treble when the file stores none).
    pub(super) clef: gscore_model::Clef,
    /// Whether the cursor's measure is in free time.
    pub(super) free_time: bool,
    /// Every measure's bar-repetition (simile) mark in the cursor's track, so a pair is
    /// set and cleared whole.
    pub(super) similes: Vec<Option<gscore_model::SimileMark>>,
    pub(super) section: Option<gscore_model::Section>,
    /// The measure's repeat signs, double barline, break and bars-per-line limit.
    pub(super) repeat: Repeat,
    pub(super) double_bar: bool,
    pub(super) system_break: SystemBreak,
    pub(super) bars_per_system: Option<u8>,
    /// The measure's feel and fermata.
    pub(super) feel: Feel,
    pub(super) fermata: Option<Fermata>,
    /// The note on the cursor's string: whether it exists and whether it is a tie.
    pub(super) note_exists: bool,
    pub(super) note_tie: bool,
    /// Whether every note of the cursor beat that continues the previous beat is tied;
    /// `None` when the beat has no note to tie.
    pub(super) beat_tie: Option<bool>,
    /// The cursor beat's text, if it has one.
    pub(super) beat_text: String,
    /// The cursor beat's chord diagram, if it has one.
    pub(super) chord: Option<gscore_model::ChordDiagram>,
    /// The track's string count, and the cursor beat's frets as a chord shape (lowest
    /// string first, -1 where it has no note), for the chord diagram dialog.
    pub(super) string_count: usize,
    pub(super) beat_shape: Vec<i8>,
    /// The chord the cursor beat's notes make ("Am"), if they make one.
    pub(super) beat_chord: Option<String>,
    /// The lyrics lines of the song, for the lyrics dialog.
    pub(super) lyric_lines: Vec<gscore_model::LyricLine>,
    /// How many measures the song has (for the lyrics dialog's start).
    pub(super) measure_count: usize,
    /// The cursor note's effects, and the cursor beat's.
    pub(super) note_state: Option<NoteState>,
    pub(super) beat_state: Option<BeatState>,
    /// Every note in the selected beats: where it is and which string.
    pub(super) note_targets: Vec<(BeatPosition, gscore_model::StringNumber)>,
    /// The effects of each of those notes, in the same order.
    pub(super) target_states: Vec<NoteState>,
    /// The effects of each selected beat, in the order of `beats`.
    pub(super) beat_states: Vec<BeatState>,
    /// The selected beats' dynamics and the one in effect at the cursor.
    pub(super) dynamics: DynamicsSnapshot,
}

impl PaletteSnapshot {
    pub(super) fn of(song: &Song, cursor: Cursor, selection: Option<Selection>) -> Self {
        let beats = selected_beats(song, cursor, selection);
        let measure = cursor.measure;
        let track = song.track(cursor.track);
        let time_signature = track
            .and_then(|track| track.measure(measure))
            .and_then(|measure| measure.time_signature().ok());
        let beat = song.beat(cursor.position());
        let note = beat.as_ref().and_then(|beat| {
            beat.notes()
                .find(|note| note.string().ok().map(|s| s.get()) == Some(cursor.string))
        });
        let note_state = note.as_ref().map(NoteState::of);
        let beat_state = beat.as_ref().map(BeatState::of);
        let beat_tie = song.beat_tie_state(cursor.position());
        let beat_text = beat
            .as_ref()
            .map(|beat| beat.text().to_owned())
            .unwrap_or_default();
        let chord = beat
            .as_ref()
            .and_then(gscore_model::BeatView::chord_diagram);
        let string_count = track.map_or(6, |track| track.string_count()).max(1);
        let mut beat_shape = vec![-1_i8; string_count];
        if let Some(beat) = &beat {
            for note in beat.notes() {
                let (Ok(string), Ok(fret)) = (note.string(), note.fret()) else {
                    continue;
                };
                // String 1 is the highest; a diagram starts at the lowest.
                let index = string_count.checked_sub(usize::from(string.get()));
                if let (Some(slot), Ok(fret)) = (
                    index.and_then(|index| beat_shape.get_mut(index)),
                    i8::try_from(fret.get()),
                ) {
                    *slot = fret;
                }
            }
        }
        let beat_chord = beat.as_ref().zip(track).and_then(|(beat, track)| {
            let pitches: Vec<_> = beat
                .notes()
                .filter_map(|note| track.sounding_pitch(&note).ok())
                .collect();
            gscore_model::chord_name(&pitches)
        });
        let lyric_lines = song.lyric_lines();
        let measure_count = song.measure_count();
        let mut note_targets = Vec::new();
        let mut target_states = Vec::new();
        let mut beat_states = Vec::new();
        for at in &beats {
            if let Some(beat) = song.beat(*at) {
                beat_states.push(BeatState::of(&beat));
                for note in beat.notes() {
                    if let Ok(string) = note.string() {
                        note_targets.push((*at, string));
                        target_states.push(NoteState::of(&note));
                    }
                }
            }
        }
        let dynamics = DynamicsSnapshot::of(song, &beats);
        let beat_values = beats
            .iter()
            .map(|at| song.beat(*at).and_then(|beat| beat.note_value().ok()))
            .collect();
        let measure_view = track.and_then(|track| track.measure(measure));
        let silent_beats = beats
            .iter()
            .copied()
            .filter(|at| {
                song.beat(*at)
                    .is_some_and(|beat| beat.notes().next().is_none())
            })
            .collect();
        let rest_beats = beats
            .iter()
            .copied()
            .filter(|at| {
                song.beat(*at)
                    .is_some_and(|beat| beat.kind() == gscore_model::BeatKind::Rest)
            })
            .collect();
        PaletteSnapshot {
            cursor,
            beats,
            beat_values,
            silent_beats,
            rest_beats,
            rest_was_last: false,
            measure,
            tempo: song
                .tempo_change(measure)
                .or_else(|| (measure.get() == 0).then(|| song.tempo())),
            time_signature,
            key: song.key_signature(measure),
            clef: track
                .and_then(|track| track.measure(measure))
                .and_then(|measure| measure.clef())
                .unwrap_or(gscore_model::Clef::Treble),
            free_time: song.free_time(measure).unwrap_or(false),
            similes: track.map_or_else(Vec::new, |track| {
                track
                    .measures()
                    .map(|measure| measure.simile_mark())
                    .collect()
            }),
            section: song.section(measure),
            repeat: song.repeat(measure).unwrap_or_default(),
            double_bar: song.double_bar(measure).unwrap_or(false),
            system_break: measure_view.map_or(SystemBreak::None, |view| view.system_break()),
            bars_per_system: track.and_then(|track| track.bars_per_system()),
            feel: song.feel(measure).unwrap_or_default(),
            fermata: song.fermata(measure),
            note_exists: note.is_some(),
            note_tie: note
                .as_ref()
                .is_some_and(|note| note.kind() == gscore_model::NoteKind::Tie),
            beat_tie,
            beat_text,
            chord,
            string_count,
            beat_shape,
            beat_chord,
            lyric_lines,
            measure_count,
            note_state,
            beat_state,
            note_targets,
            target_states,
            beat_states,
            dynamics,
        }
    }
}

/// The cursor and selection the palette shows: the playing cursor is ignored, so the
/// palette stays on the note chosen before playback started.
fn palette_position(
    playback: &crate::playback::Playback,
    tab: &super::tab::Tab,
) -> (Cursor, Option<Selection>) {
    let current = || {
        (
            tab.document.editor().cursor(),
            tab.document.editor().selection(),
        )
    };
    if playback.state() == gscore_play::PlayState::Playing {
        tab.palette_anchor.unwrap_or_else(current)
    } else {
        current()
    }
}

impl GuitarScoreApp {
    /// The left panel: the edit palette.
    pub(super) fn edit_palette(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let several = tab.shown_tracks().len() > 1;
        let document = &tab.document;
        let (cursor, selection) = palette_position(&self.playback, tab);
        let mut snapshot = PaletteSnapshot::of(document.song(), cursor, selection);
        snapshot.rest_was_last = rest_was_last(&snapshot, document.editor().last_command());
        // With several tracks shown, the line breaks are the multitrack layout's.
        snapshot.system_break =
            super::score_view::shown_break(document.song(), cursor.track, cursor.measure, several);

        let mut commands: Vec<Command> = Vec::new();
        // A beat to play although nothing changed (its dynamic chosen again).
        let mut heard = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let keymap = &self.keymap;
                let hint = |action: ActionId| key_hint(keymap, action);
                let beat_text = edit_staff::section(
                    ui,
                    &mut tab.edit_palette.staff,
                    &snapshot,
                    &hint,
                    &mut commands,
                );
                if beat_text == Some(ActionId::EditBeatText) {
                    tab.edit_palette.effects.open_beat_text(&snapshot);
                } else if beat_text == Some(ActionId::EditLyrics) {
                    tab.edit_palette.effects.open_lyrics(&snapshot);
                }
                style::section(ui, "note_input", "Note input", |ui| {
                    note_length(ui, &mut tab.edit_palette, &snapshot, &hint, &mut commands);
                    beat_buttons(ui, &mut tab.edit_palette, &snapshot, &hint, &mut commands);
                });
                edit_dynamics::section(ui, &snapshot, &hint, &mut commands, &mut heard);
                tab.edit_palette
                    .effects
                    .panel(ui, &snapshot, &hint, &mut commands);
            });
        // New beats past the cursor take the palette's length; a voice choice is a cursor
        // move, which no command expresses. Both are read before the commands run.
        let duration = tab.edit_palette.duration();
        let pending_voice = tab.edit_palette.pending_voice.take();
        self.apply_commands(commands);
        if let Some(at) = heard {
            self.preview_beat(at);
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if let Some(duration) = duration {
                tab.document.editor_mut().set_entry_value(duration);
            }
            if let Some(voice) = pending_voice {
                let mut cursor = tab.document.editor().cursor();
                cursor.voice = VoiceIndex::new(voice);
                tab.document.editor_mut().set_cursor(cursor);
            }
        }
    }

    /// Runs a palette action from a key, exactly like a click on its tile.
    pub(super) fn run_palette_action(&mut self, action: ActionId) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &tab.document;
        let (cursor, selection) = palette_position(&self.playback, tab);
        let mut snapshot = PaletteSnapshot::of(document.song(), cursor, selection);
        snapshot.rest_was_last = rest_was_last(&snapshot, document.editor().last_command());
        let state = &mut tab.edit_palette;
        let value = match action {
            ActionId::NoteWhole => Some(1),
            ActionId::NoteHalf => Some(2),
            ActionId::NoteQuarter => Some(4),
            ActionId::NoteEighth => Some(8),
            ActionId::NoteSixteenth => Some(16),
            ActionId::NoteThirtySecond => Some(32),
            ActionId::NoteSixtyFourth => Some(64),
            _ => None,
        };
        let length = match (value, action) {
            (Some(value), _) => {
                state.value = value;
                Some(LengthChange::Value)
            }
            (None, ActionId::ToggleDot) => {
                toggle_dots(state, 1);
                Some(LengthChange::Dots)
            }
            (None, ActionId::ToggleDoubleDot) => {
                toggle_dots(state, 2);
                Some(LengthChange::Dots)
            }
            (None, ActionId::TupletThree) => {
                toggle_tuplet(state, (3, 2));
                Some(LengthChange::Tuplet)
            }
            (None, ActionId::TupletFive) => {
                toggle_tuplet(state, (5, 4));
                Some(LengthChange::Tuplet)
            }
            (None, ActionId::TupletSeven) => {
                toggle_tuplet(state, (7, 4));
                Some(LengthChange::Tuplet)
            }
            (None, ActionId::TupletSix) => {
                toggle_tuplet(state, (6, 4));
                Some(LengthChange::Tuplet)
            }
            (None, ActionId::TupletOther) => {
                state.tuplet_dialog = Some(TupletDialog::new(state.tuplet));
                None
            }
            _ => None,
        };
        let mut commands = Vec::new();
        if let Some(change) = length {
            match change {
                LengthChange::Value => apply_duration(state, &snapshot, &mut commands),
                LengthChange::Dots => apply_dots(state, &snapshot, &mut commands),
                LengthChange::Tuplet => apply_tuplet(state, &snapshot, &mut commands),
            }
        } else if action == ActionId::MakeRest {
            commands = rest_commands(state, &snapshot);
        } else if action == ActionId::ToggleTie {
            commands = tie_commands(&snapshot);
        } else if action == ActionId::TieBeat {
            commands = tie_beat_commands(&snapshot);
        } else if action == ActionId::EditBeatText {
            state.effects.open_beat_text(&snapshot);
        } else if action == ActionId::EditLyrics {
            state.effects.open_lyrics(&snapshot);
        } else if action == ActionId::EditChordDiagram {
            state.effects.open_chord(&snapshot);
        } else if action == ActionId::EditLeftFinger {
            state.effects.open_finger(&snapshot, Hand::Left);
        } else if action == ActionId::EditRightFinger {
            state.effects.open_finger(&snapshot, Hand::Right);
        } else if action == ActionId::ToggleBend {
            // Like a click on the bend or whammy tile: the editor opens.
            state.effects.open_bend(&snapshot);
        } else if action == ActionId::ToggleWhammy {
            state.effects.open_whammy(&snapshot);
        } else if action == ActionId::GraceNote {
            state.effects.open_grace(&snapshot);
        } else if action == ActionId::TrillOther {
            state.effects.open_trill(&snapshot);
        } else if action == ActionId::TremoloOther {
            state.effects.open_tremolo(&snapshot);
        } else if let Some((kind, direction)) =
            super::edit_effects::fx_item(action).and_then(|item| item.fx.stroke_target())
        {
            state.effects.open_stroke(&snapshot, kind, direction);
        } else if let Some(item) = super::edit_effects::fx_item(action) {
            commands = super::edit_effects::fx_commands(item.fx, &snapshot);
        }
        let duration = state.duration();
        self.apply_commands(commands);
        if let Some(duration) = duration
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.document.editor_mut().set_entry_value(duration);
        }
    }

    /// The bend or whammy editor, while one is open. It floats over the app, so it shows
    /// even when the palette is hidden or its section folded.
    pub(super) fn show_shape_editors(&mut self, ctx: &egui::Context) {
        if !self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.edit_palette.effects.is_editing())
        {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &tab.document;
        let (cursor, selection) = palette_position(&self.playback, tab);
        let snapshot = PaletteSnapshot::of(document.song(), cursor, selection);
        let mut commands = Vec::new();
        let mut trial = Vec::new();
        tab.edit_palette.effects.show_editors(
            ctx,
            &snapshot,
            document.song(),
            &mut self.settings,
            &mut commands,
            &mut trial,
        );
        self.apply_commands(commands);
        self.listen_to(&trial);
        self.show_lyrics_preview();
    }

    /// Shows the lyrics dialog's text in the score as it is typed, one undo step per line;
    /// Cancel takes the dialog's changes back.
    fn show_lyrics_preview(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(preview) = tab.edit_palette.effects.take_lyrics_preview() else {
            return;
        };
        match preview {
            LyricsPreview::Show(command) => {
                let depth = tab.document.editor().undo_depth();
                tab.edit_palette.effects.note_lyrics_undo_start(depth);
                let closing = !tab.edit_palette.effects.lyrics_open();
                self.apply_command(command, true);
                if closing && let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.document.editor_mut().break_merge();
                }
            }
            LyricsPreview::Done => tab.document.editor_mut().break_merge(),
            LyricsPreview::Revert(depth) => {
                // Only the dialog's own steps, newest first: an edit made meanwhile stays.
                while self.tabs.get(self.active).is_some_and(|tab| {
                    tab.document.editor().undo_depth() > depth
                        && matches!(
                            tab.document.editor().last_command(),
                            Some(Command::SetLyricLine { .. })
                        )
                }) {
                    self.apply_command(Command::Undo, false);
                }
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.document.editor_mut().break_merge();
                }
            }
        }
    }

    /// The tuplet dialog, while it is open. OK sets the palette's tuplet and the length of
    /// the selected beats; Remove takes the tuplet off them.
    pub(super) fn show_tuplet_dialog(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let (cursor, selection) = palette_position(&self.playback, tab);
        let state = &mut tab.edit_palette;
        let Some(dialog) = &mut state.tuplet_dialog else {
            return;
        };
        let tuplet = match dialog.show(ctx) {
            Outcome::Open | Outcome::Listen => return,
            Outcome::Cancel => {
                state.tuplet_dialog = None;
                return;
            }
            Outcome::Apply => dialog.tuplet(),
            Outcome::Remove => None,
        };
        state.tuplet_dialog = None;
        state.tuplet = tuplet;
        let document = &tab.document;
        let snapshot = PaletteSnapshot::of(document.song(), cursor, selection);
        let mut commands = Vec::new();
        apply_tuplet(state, &snapshot, &mut commands);
        let duration = state.duration();
        self.apply_commands(commands);
        if let Some(duration) = duration
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.document.editor_mut().set_entry_value(duration);
        }
    }

    /// The staff dialog, while one is open. It floats over the app, so it shows even when
    /// the palette is hidden or its section folded.
    pub(super) fn show_staff_dialogs(&mut self, ctx: &egui::Context) {
        if !self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.edit_palette.staff.is_open())
        {
            return;
        }
        let mut commands = Vec::new();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.edit_palette.staff.show_dialog(ctx, &mut commands);
        }
        self.apply_commands(commands);
    }

    /// Applies one command, or a batch for several beats (one undo step).
    pub(super) fn apply_commands(&mut self, mut commands: Vec<Command>) {
        let heard = commands.iter().find_map(super::editing::preview_target);
        self.apply_commands_quietly(&mut commands);
        // An effect or dynamic chosen in the palette is heard on its beat.
        if let Some(at) = heard {
            self.preview_beat(at);
        }
    }

    /// Applies the commands without playing anything.
    fn apply_commands_quietly(&mut self, commands: &mut Vec<Command>) {
        match commands.len() {
            0 => {}
            1 => {
                if let Some(command) = commands.pop() {
                    self.apply_command(command, false);
                }
            }
            _ => self.apply_command(
                Command::Batch {
                    commands: std::mem::take(commands),
                },
                false,
            ),
        }
    }

    /// The start of a selection for playback: its first beat with a note, else its first
    /// beat (for a measure selection, in the shown track). `None` without a selection.
    pub(super) fn selection_start(&self) -> Option<BeatPosition> {
        let document = self.tabs.get(self.active).map(|tab| &tab.document)?;
        let cursor = document.editor().cursor();
        let song = document.song();
        let selection = document.editor().selection()?;
        if let Selection::Measures { track, .. } = selection {
            return Some(BeatPosition {
                staff: gscore_model::StaffIndex::new(0),
                track: track.unwrap_or(cursor.track),
                measure: selection.first_measure(),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(0),
            });
        }
        let positions = selection.positions(song);
        positions
            .iter()
            .copied()
            .find(|at| {
                song.beat(*at)
                    .is_some_and(|beat| beat.notes().next().is_some())
            })
            .or_else(|| positions.first().copied())
    }
}

/// Note lengths, dots and tuplets as tiles; choosing one sets the length of the selected
/// beats and of the notes entered next.
fn note_length(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    hint: &dyn Fn(ActionId) -> String,
    commands: &mut Vec<Command>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 3.0);
        for value in NOTE_VALUES {
            let label = note_value_label(value);
            let tip = format!(
                "{} note{}",
                note_value_name(value),
                hint(value_action(value))
            );
            let chosen = state.value == value;
            if tiles::tile(ui, note_face(value), chosen, true, &label, &tip).clicked() {
                state.value = value;
                apply_duration(state, snapshot, commands);
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 3.0);
        for (dots, label, tip, action) in [
            (
                1_u8,
                "Dot",
                "Dotted: half as long again",
                ActionId::ToggleDot,
            ),
            (2, "Double dot", "Double dotted", ActionId::ToggleDoubleDot),
        ] {
            let face = dot_face(dots);
            let tip = format!("{tip}{}", hint(action));
            if tiles::tile(ui, face, state.dots == dots, true, label, &tip).clicked() {
                toggle_dots(state, dots);
                apply_dots(state, snapshot, commands);
            }
        }
        ui.add_space(6.0);
        for (enters, times) in TUPLETS {
            let text = enters.to_string();
            let label = tuplet_name(enters, times);
            let tip = format!(
                "{label}: {enters} in the time of {times}{}",
                hint(tuplet_action(enters))
            );
            let on = state.tuplet == Some((enters, times));
            let face = Face::TupletBracket {
                text: &text,
                letters: false,
                more: false,
            };
            if tiles::wide_tile(ui, face, on, true, &label, &tip).clicked() {
                toggle_tuplet(state, (enters, times));
                apply_tuplet(state, snapshot, commands);
            }
        }
        // Any other tuplet: the tile shows it when one is chosen, else "n:m".
        let other = state.tuplet.filter(|tuplet| !TUPLETS.contains(tuplet));
        let text = other.map_or_else(|| "n:m".to_owned(), |(n, m)| format!("{n}:{m}"));
        let face = Face::TupletBracket {
            text: &text,
            letters: other.is_none(),
            more: true,
        };
        let tip = format!(
            "Tuplet…: any number of notes in the time of another{}",
            hint(ActionId::TupletOther)
        );
        if tiles::wide_tile(ui, face, other.is_some(), true, "Tuplet…", &tip).clicked() {
            state.tuplet_dialog = Some(TupletDialog::new(state.tuplet));
        }
    });
}

/// The palette action of a note value.
fn value_action(value: u16) -> ActionId {
    match value {
        1 => ActionId::NoteWhole,
        2 => ActionId::NoteHalf,
        4 => ActionId::NoteQuarter,
        8 => ActionId::NoteEighth,
        16 => ActionId::NoteSixteenth,
        32 => ActionId::NoteThirtySecond,
        _ => ActionId::NoteSixtyFourth,
    }
}

/// The palette action of a tuplet tile, by how many notes it has.
fn tuplet_action(enters: u8) -> ActionId {
    match enters {
        3 => ActionId::TupletThree,
        _ => ActionId::TupletSix,
    }
}

/// The name of a tuplet tile.
fn tuplet_name(enters: u8, times: u8) -> String {
    match (enters, times) {
        (3, 2) => "Triplet".to_owned(),
        (6, 4) => "Sextuplet".to_owned(),
        _ => format!("{enters}:{times}"),
    }
}

/// A dot choice is a toggle: choosing the chosen one again removes the dots.
fn toggle_dots(state: &mut EditPaletteState, dots: u8) {
    state.dots = if state.dots == dots { 0 } else { dots };
}

/// A tuplet choice is a toggle too.
fn toggle_tuplet(state: &mut EditPaletteState, tuplet: (u8, u8)) {
    state.tuplet = if state.tuplet == Some(tuplet) {
        None
    } else {
        Some(tuplet)
    };
}

/// The commands of the rest tile: every selected beat becomes a rest.
///
/// A beat without notes (a rest, such as the whole rest of a new bar, or an empty beat)
/// becomes a rest of the palette's length; beats with notes keep theirs. Where the cursor
/// has no beat yet (an empty GP7 bar), a rest of the palette's length is entered.
fn rest_commands(state: &EditPaletteState, snapshot: &PaletteSnapshot) -> Vec<Command> {
    let duration = state.duration();
    if snapshot.beats.is_empty() {
        return duration
            .map(|duration| Command::InsertBeat {
                at: snapshot.cursor.position(),
                duration,
            })
            .into_iter()
            .collect();
    }
    // A second click on rests the tile just made brings their notes back.
    if snapshot.rest_was_last && snapshot.rest_beats.len() == snapshot.beats.len() {
        return vec![Command::Undo];
    }
    // Every beat becomes a rest; an empty one (a rest too) takes the chosen length.
    let mut batch = Vec::new();
    for &at in &snapshot.beats {
        batch.push(Command::SetBeatRest { at });
        if snapshot.silent_beats.contains(&at)
            && let Some(duration) = duration
        {
            batch.push(Command::SetDuration { at, duration });
        }
    }
    vec![Command::Batch { commands: batch }]
}

/// Whether `last`, the editor's last command, is the rest tile's batch for exactly the
/// snapshot's beats, and they are all rests now (it was not undone).
pub(super) fn rest_was_last(snapshot: &PaletteSnapshot, last: Option<&Command>) -> bool {
    let Some(Command::Batch { commands }) = last else {
        return false;
    };
    let rested: Vec<BeatPosition> = commands
        .iter()
        .filter_map(|command| match command {
            Command::SetBeatRest { at } => Some(*at),
            _ => None,
        })
        .collect();
    let only_rests = commands.iter().all(|command| {
        matches!(
            command,
            Command::SetBeatRest { .. } | Command::SetDuration { .. }
        )
    });
    only_rests
        && !snapshot.beats.is_empty()
        && rested == snapshot.beats
        && snapshot.rest_beats.len() == snapshot.beats.len()
}

/// The commands of the tie-beat tile: every note of the cursor beat is tied to the
/// previous beat, or those ties are removed.
fn tie_beat_commands(snapshot: &PaletteSnapshot) -> Vec<Command> {
    let Some(all_tied) = snapshot.beat_tie else {
        return Vec::new();
    };
    vec![Command::SetBeatTie {
        at: snapshot.cursor.position(),
        tie: !all_tied,
    }]
}

/// The commands of the tie tile: the cursor's note is tied, or untied.
fn tie_commands(snapshot: &PaletteSnapshot) -> Vec<Command> {
    match gscore_model::StringNumber::new(snapshot.cursor.string) {
        Ok(string) if snapshot.note_exists => vec![Command::SetTie {
            at: snapshot.cursor.position(),
            string,
            tie: !snapshot.note_tie,
        }],
        _ => Vec::new(),
    }
}

/// The tile face of one or two augmentation dots.
fn dot_face(dots: u8) -> Face<'static> {
    let text = if dots == 1 {
        "\u{E1E7}"
    } else {
        "\u{E1E7}\u{E1E7}"
    };
    Face::Glyph {
        text,
        size: 38.0,
        baseline: 2.0,
    }
}

/// The rest tile: a quarter rest.
const REST_FACE: Face<'static> = Face::Glyph {
    text: "\u{E4E5}",
    size: 22.0,
    baseline: 1.0,
};

/// Every note-input tile's face, for the test that they fit their tiles: the regular
/// tiles, then the wide (tuplet) ones.
#[cfg(test)]
pub(super) fn faces() -> (Vec<Face<'static>>, Vec<Face<'static>>) {
    let mut regular: Vec<_> = [1, 2, 4, 8, 16, 32, 64]
        .into_iter()
        .map(note_face)
        .collect();
    regular.extend([
        dot_face(1),
        dot_face(2),
        REST_FACE,
        Face::Tie,
        Face::BeatTie,
    ]);
    let wide = vec![
        Face::TupletBracket {
            text: "3",
            letters: false,
            more: false,
        },
        Face::TupletBracket {
            text: "6",
            letters: false,
            more: false,
        },
        Face::TupletBracket {
            text: "n:m",
            letters: true,
            more: true,
        },
        Face::TupletBracket {
            text: "12:8",
            letters: false,
            more: true,
        },
    ];
    (regular, wide)
}

/// The tile face of a note value: the note from the music font.
fn note_face(value: u16) -> Face<'static> {
    // SMuFL "Individual notes", stems up. Longer stems (32nd, 64th) sit a little higher.
    let (text, baseline) = match value {
        1 => ("\u{E1D2}", 2.0),
        2 => ("\u{E1D3}", 9.0),
        4 => ("\u{E1D5}", 9.0),
        8 => ("\u{E1D7}", 9.0),
        16 => ("\u{E1D9}", 9.0),
        32 => ("\u{E1DB}", 10.0),
        _ => ("\u{E1DD}", 12.0),
    };
    Face::Glyph {
        text,
        size: 22.0,
        baseline,
    }
}

/// "Whole", "Half", "Quarter", "8th", ...
fn note_value_name(value: u16) -> String {
    match value {
        1 => "Whole".to_owned(),
        2 => "Half".to_owned(),
        4 => "Quarter".to_owned(),
        _ => format!("{value}th"),
    }
}

/// Rest and tie tiles and the voice choice.
fn beat_buttons(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    hint: &dyn Fn(ActionId) -> String,
    commands: &mut Vec<Command>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 3.0);
        let rest = REST_FACE;
        let tip = format!(
            "Rest: the selected beats become rests (click again to get their notes back); \
             in an empty bar, a rest of the chosen length is entered{}",
            hint(ActionId::MakeRest)
        );
        let rests = !snapshot.beats.is_empty() && snapshot.rest_beats.len() == snapshot.beats.len();
        if tiles::tile(ui, rest, rests, true, "Rest", &tip).clicked() {
            commands.extend(rest_commands(state, snapshot));
        }
        // A fermata holds the bar's first note: next to the rest, which it often sits on.
        if tiles::dialog_tile(
            ui,
            Face::Fermata,
            snapshot.fermata.is_some(),
            true,
            "Fermata",
            "Put a fermata on this bar",
        )
        .clicked()
        {
            state
                .staff
                .open_dialog(edit_staff::Dialog::Fermata, snapshot);
        }
        let tip = format!(
            "Tie the note to the previous one on its string{}",
            hint(ActionId::ToggleTie)
        );
        let tie = tiles::tile(
            ui,
            Face::Tie,
            snapshot.note_tie,
            snapshot.note_exists,
            "Tie",
            &tip,
        );
        if tie.clicked() {
            commands.extend(tie_commands(snapshot));
        }
        let tip = format!(
            "Tie beat: tie every note of the beat to the previous beat{}",
            hint(ActionId::TieBeat)
        );
        let tie_beat = tiles::tile(
            ui,
            Face::BeatTie,
            snapshot.beat_tie == Some(true),
            snapshot.beat_tie.is_some(),
            "Tie beat",
            &tip,
        );
        if tie_beat.clicked() {
            commands.extend(tie_beat_commands(snapshot));
        }
        // Octave signs change where the notes are written: the last of Note input.
        super::edit_effects::octave_tiles(ui, snapshot, hint, commands);
    });
    ui.add_space(2.0);
    let chosen = snapshot.cursor.voice.get();
    if let Some(voice) = tiles::segments_with_tips(
        ui,
        &[
            ("1", "Voice 1", "First voice"),
            ("2", "Voice 2", "Second voice"),
            (
                "3",
                "Voice 3",
                "Third voice (kept in GP7 files; GP3-5 keep two voices)",
            ),
            (
                "4",
                "Voice 4",
                "Fourth voice (kept in GP7 files; GP3-5 keep two voices)",
            ),
        ],
        chosen,
    ) {
        state.pending_voice = Some(voice);
    }
}

/// How a note-length choice changes the selected beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LengthChange {
    /// The chosen note value replaces every selected beat's value.
    Value,
    /// Dots are added or removed, keeping each beat's value.
    Dots,
    /// A tuplet is added or removed, keeping each beat's value.
    Tuplet,
}

/// Commands that set the chosen length on every selected beat.
fn apply_duration(
    state: &EditPaletteState,
    snapshot: &PaletteSnapshot,
    commands: &mut Vec<Command>,
) {
    let Some(duration) = state.duration() else {
        return;
    };
    for at in &snapshot.beats {
        commands.push(Command::SetDuration { at: *at, duration });
    }
}

/// Commands that change only the dots of the selected beats, keeping every beat's own note
/// value (a dotted quarter stays a quarter when the dot is removed). Beats the palette
/// entered itself fall back to its value.
fn apply_dots(state: &EditPaletteState, snapshot: &PaletteSnapshot, commands: &mut Vec<Command>) {
    let fallback = NoteValue::new(state.value, state.dots, state.tuplet).ok();
    for (index, at) in snapshot.beats.iter().enumerate() {
        let base = snapshot
            .beat_values
            .get(index)
            .copied()
            .flatten()
            .or(fallback);
        let Some(base) = base else {
            continue;
        };
        let Ok(duration) = NoteValue::new(base.value(), state.dots, base.tuplet()) else {
            continue;
        };
        commands.push(Command::SetDuration { at: *at, duration });
    }
}

/// Commands that add or remove the palette's tuplet on the selected beats, keeping every
/// beat's own written value, so taking a triplet off restores the original notes.
fn apply_tuplet(state: &EditPaletteState, snapshot: &PaletteSnapshot, commands: &mut Vec<Command>) {
    let fallback = NoteValue::new(state.value, state.dots, state.tuplet).ok();
    for (index, at) in snapshot.beats.iter().enumerate() {
        let base = snapshot
            .beat_values
            .get(index)
            .copied()
            .flatten()
            .or(fallback);
        let Some(base) = base else {
            continue;
        };
        let Ok(duration) = NoteValue::new(base.value(), base.dots(), state.tuplet) else {
            continue;
        };
        commands.push(Command::SetDuration { at: *at, duration });
    }
}

/// The selected beats: the selection, or the cursor's beat.
fn selected_beats(song: &Song, cursor: Cursor, selection: Option<Selection>) -> Vec<BeatPosition> {
    match selection {
        Some(selection) => selection.positions(song),
        None => song
            .beat(cursor.position())
            .map(|_| cursor.position())
            .into_iter()
            .collect(),
    }
}

/// "1", "1/2", ... for a note value.
fn note_value_label(value: u16) -> String {
    if value == 1 {
        "1".to_owned()
    } else {
        format!("1/{value}")
    }
}
