//! The edit palette (the left panel): note lengths, dots and tuplets, rests and ties, the
//! voice, dynamics, the effects matrix, and the measure's tempo, signatures and section
//! marker.
//!
//! Changes are ordinary commands (or one `batch` command for several beats), so every
//! palette click is undoable and recorded in the recovery journal.

use gscore_edit::{ActionId, Command, Cursor, Selection};
use gscore_model::{
    BeatIndex, BeatPosition, KeySignature, MeasureIndex, NoteValue, Section, Song, TimeSignature,
    TrackColor, VoiceIndex,
};

use super::GuitarScoreApp;
use super::edit_dynamics::{self, DynamicsSnapshot};
use super::edit_effects::{BeatState, EffectsState, NoteState};
use super::style;
use super::tiles::{self, Face};
use super::toolbar::key_hint;

/// Note lengths offered, as the `1/value` of a whole note.
const NOTE_VALUES: [u16; 7] = [1, 2, 4, 8, 16, 32, 64];
/// Tuplets offered: "in the time of".
const TUPLETS: [(u8, u8); 3] = [(3, 2), (5, 4), (7, 4)];

/// What the palette keeps between frames.
#[derive(Debug)]
pub(super) struct EditPaletteState {
    /// The chosen note value (`1/value`).
    value: u16,
    /// Augmentation dots (0 to 2).
    dots: u8,
    /// The chosen tuplet, if any.
    tuplet: Option<(u8, u8)>,
    /// The measure and song revision the header fields below were read at; they are read
    /// again when either changes (another measure, an edit, an undo), so they never show a
    /// stale value. A draft typed into them lasts until the next edit.
    fields_from: Option<(MeasureIndex, u64)>,
    tempo: u32,
    numerator: u8,
    denominator: u16,
    key: i8,
    minor: bool,
    section_title: String,
    section_color: [u8; 3],
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
            fields_from: None,
            tempo: 120,
            numerator: 4,
            denominator: 4,
            key: 0,
            minor: false,
            section_title: String::new(),
            section_color: [255, 0, 0],
            pending_voice: None,
            effects: EffectsState::default(),
        }
    }
}

impl EditPaletteState {
    /// Reads the header fields from the song again on the next frame (another document).
    pub(super) fn forget_fields(&mut self) {
        self.fields_from = None;
    }

    fn duration(&self) -> Option<NoteValue> {
        NoteValue::new(self.value, self.dots, self.tuplet).ok()
    }
}

/// The song data the palette shows, taken before the widgets run.
pub(super) struct PaletteSnapshot {
    pub(super) cursor: Cursor,
    pub(super) beats: Vec<BeatPosition>,
    pub(super) measure: MeasureIndex,
    pub(super) tempo: Option<u32>,
    pub(super) time_signature: Option<TimeSignature>,
    pub(super) key: Option<KeySignature>,
    pub(super) section: Option<Section>,
    /// The note on the cursor's string: whether it exists and whether it is a tie.
    pub(super) note_exists: bool,
    pub(super) note_tie: bool,
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
    fn of(song: &Song, cursor: Cursor, selection: Option<Selection>) -> Self {
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
        let dynamics = DynamicsSnapshot::of(song, &beats, cursor.position());
        PaletteSnapshot {
            cursor,
            beats,
            measure,
            tempo: song
                .tempo_change(measure)
                .or_else(|| (measure.get() == 0).then(|| song.tempo())),
            time_signature,
            key: song.key_signature(measure),
            section: song.section(measure),
            note_exists: note.is_some(),
            note_tie: note
                .as_ref()
                .is_some_and(|note| note.kind() == gscore_model::NoteKind::Tie),
            note_state,
            beat_state,
            note_targets,
            target_states,
            beat_states,
            dynamics,
        }
    }
}

impl GuitarScoreApp {
    /// The left panel: the edit palette.
    pub(super) fn edit_palette(&mut self, ui: &mut egui::Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let cursor = document.editor().cursor();
        let selection = document.editor().selection();
        let snapshot = PaletteSnapshot::of(document.song(), cursor, selection);

        let fields_from = (snapshot.measure, document.editor().revision());
        if self.edit_palette.fields_from != Some(fields_from) {
            self.edit_palette.fields_from = Some(fields_from);
            self.edit_palette.tempo = snapshot.tempo.unwrap_or(120);
            if let Some(signature) = snapshot.time_signature {
                self.edit_palette.numerator = signature.numerator();
                self.edit_palette.denominator = signature.denominator();
            }
            if let Some(key) = snapshot.key {
                self.edit_palette.key = key.key;
                self.edit_palette.minor = key.minor;
            }
            self.edit_palette.section_title = snapshot
                .section
                .as_ref()
                .map_or_else(String::new, |section| section.title.clone());
            if let Some(section) = &snapshot.section {
                let (red, green, blue) = section.color.parts();
                self.edit_palette.section_color = [red, green, blue];
            }
        }

        let mut commands: Vec<Command> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let keymap = &self.keymap;
                let hint = |action: ActionId| key_hint(keymap, action);
                style::section(ui, "note_input", "Note input", |ui| {
                    note_length(ui, &mut self.edit_palette, &snapshot, &hint, &mut commands);
                    beat_buttons(ui, &mut self.edit_palette, &snapshot, &hint, &mut commands);
                });
                edit_dynamics::section(ui, &snapshot.dynamics, &mut commands);
                self.edit_palette
                    .effects
                    .panel(ui, &snapshot, &hint, &mut commands);
                // The measure's headers come last, after the techniques and strokes.
                style::section(ui, "measure", "Measure", |ui| {
                    header_editors(ui, &mut self.edit_palette, &snapshot, &mut commands);
                });
            });
        self.apply_commands(commands);
        // New beats past the cursor take the palette's length.
        if let Some(duration) = self.edit_palette.duration()
            && let Some(document) = &mut self.document
        {
            document.editor_mut().set_entry_value(duration);
        }

        // A voice choice is a cursor move, which no command expresses.
        if let Some(voice) = self.edit_palette.pending_voice.take()
            && let Some(document) = &mut self.document
        {
            let mut cursor = document.editor().cursor();
            cursor.voice = VoiceIndex::new(voice);
            document.editor_mut().set_cursor(cursor);
        }
    }

    /// Runs a palette action from a key, exactly like a click on its tile.
    pub(super) fn run_palette_action(&mut self, action: ActionId) {
        let Some(document) = &self.document else {
            return;
        };
        let snapshot = PaletteSnapshot::of(
            document.song(),
            document.editor().cursor(),
            document.editor().selection(),
        );
        let state = &mut self.edit_palette;
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
                true
            }
            (None, ActionId::ToggleDot) => {
                toggle_dots(state, 1);
                true
            }
            (None, ActionId::ToggleDoubleDot) => {
                toggle_dots(state, 2);
                true
            }
            (None, ActionId::TupletThree) => {
                toggle_tuplet(state, (3, 2));
                true
            }
            (None, ActionId::TupletFive) => {
                toggle_tuplet(state, (5, 4));
                true
            }
            (None, ActionId::TupletSeven) => {
                toggle_tuplet(state, (7, 4));
                true
            }
            _ => false,
        };
        let mut commands = Vec::new();
        if length {
            apply_duration(state, &snapshot, &mut commands);
        } else if action == ActionId::MakeRest {
            commands = rest_commands(&snapshot);
        } else if action == ActionId::ToggleTie {
            commands = tie_commands(&snapshot);
        } else if let Some(item) = super::edit_effects::fx_item(action) {
            commands = super::edit_effects::fx_commands(item.fx, &snapshot);
        }
        self.apply_commands(commands);
        if let Some(duration) = self.edit_palette.duration()
            && let Some(document) = &mut self.document
        {
            document.editor_mut().set_entry_value(duration);
        }
    }

    /// Applies one command, or a batch for several beats (one undo step).
    pub(super) fn apply_commands(&mut self, mut commands: Vec<Command>) {
        match commands.len() {
            0 => {}
            1 => {
                if let Some(command) = commands.pop() {
                    self.apply_command(command, false);
                }
            }
            _ => self.apply_command(Command::Batch { commands }, false),
        }
    }

    /// The start of a selection for playback: its first beat with a note, else its first
    /// beat (for a measure selection, in the shown track). `None` without a selection.
    pub(super) fn selection_start(&self) -> Option<BeatPosition> {
        let document = self.document.as_ref()?;
        let cursor = document.editor().cursor();
        let song = document.song();
        let selection = document.editor().selection()?;
        if let Selection::Measures { .. } = selection {
            return Some(BeatPosition {
                track: cursor.track,
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
            let glyph = if dots == 1 {
                "\u{E1E7}"
            } else {
                "\u{E1E7}\u{E1E7}"
            };
            let face = Face::Glyph {
                text: glyph,
                size: 26.0,
                baseline: 4.0,
            };
            let tip = format!("{tip}{}", hint(action));
            if tiles::tile(ui, face, state.dots == dots, true, label, &tip).clicked() {
                toggle_dots(state, dots);
                apply_duration(state, snapshot, commands);
            }
        }
        ui.add_space(6.0);
        for (enters, times) in TUPLETS {
            let text = if times == 2 && enters == 3 {
                "3".to_owned()
            } else {
                format!("{enters}:{times}")
            };
            let label = format!("{enters}:{times}");
            let tip = format!(
                "Tuplet: {enters} in the time of {times}{}",
                hint(tuplet_action(enters))
            );
            let on = state.tuplet == Some((enters, times));
            if tiles::tile(ui, Face::Text(&text), on, true, &label, &tip).clicked() {
                toggle_tuplet(state, (enters, times));
                apply_duration(state, snapshot, commands);
            }
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

/// The palette action of a tuplet, by how many notes it has.
fn tuplet_action(enters: u8) -> ActionId {
    match enters {
        3 => ActionId::TupletThree,
        5 => ActionId::TupletFive,
        _ => ActionId::TupletSeven,
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
fn rest_commands(snapshot: &PaletteSnapshot) -> Vec<Command> {
    snapshot
        .beats
        .iter()
        .map(|at| Command::SetBeatRest { at: *at })
        .collect()
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
        _ => ("\u{E1DD}", 11.0),
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
        let rest = Face::Glyph {
            text: "\u{E4E5}",
            size: 22.0,
            baseline: 1.0,
        };
        let tip = format!(
            "Rest: the selected beats become rests{}",
            hint(ActionId::MakeRest)
        );
        if tiles::tile(ui, rest, false, true, "Rest", &tip).clicked() {
            commands.extend(rest_commands(snapshot));
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
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Voice").color(style::tokens(ui).text_weak));
        let chosen = snapshot.cursor.voice.get();
        if let Some(voice) = tiles::segments(
            ui,
            &[("1", "Voice 1"), ("2", "Voice 2")],
            chosen,
            "Enter notes in this voice",
        ) {
            state.pending_voice = Some(voice);
        }
    });
}

/// Header fields at the cursor's measure: tempo, signatures and section marker, as a
/// property sheet (labels in one column, values and their Set buttons in the other).
fn header_editors(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    commands: &mut Vec<Command>,
) {
    let weak = style::tokens(ui).text_weak;
    let label = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).color(weak));
    };
    egui::Grid::new("measure_properties")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            label(ui, "Tempo");
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut state.tempo)
                        .range(1..=1000)
                        .suffix(" BPM"),
                );
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Measure tempo"));
                if ui.button("Set tempo").clicked() {
                    commands.push(Command::SetMeasureTempo {
                        measure: snapshot.measure,
                        tempo: state.tempo,
                    });
                }
            });
            ui.end_row();

            label(ui, "Time");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let response = ui.add(egui::DragValue::new(&mut state.numerator).range(1..=64));
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Time numerator"));
                ui.label("/");
                let response = ui.add(
                    egui::DragValue::new(&mut state.denominator)
                        .range(1..=128)
                        .speed(1),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Time denominator");
                });
                ui.add_space(4.0);
                if ui.button("Set time").clicked() {
                    commands.push(Command::SetTimeSignature {
                        measure: snapshot.measure,
                        numerator: state.numerator,
                        denominator: state.denominator,
                    });
                }
            });
            ui.end_row();

            label(ui, "Key");
            ui.horizontal(|ui| {
                let response = ui.add(egui::DragValue::new(&mut state.key).range(-7..=7));
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Key signature"));
                ui.checkbox(&mut state.minor, "minor");
                if ui.button("Set key").clicked() {
                    commands.push(Command::SetKeySignature {
                        measure: snapshot.measure,
                        key: state.key,
                        minor: state.minor,
                    });
                }
            });
            ui.end_row();

            label(ui, "Section");
            ui.horizontal(|ui| {
                let response = ui.color_edit_button_srgb(&mut state.section_color);
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Section color"));
                response.on_hover_text("The color is applied with the title (Enter)");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.section_title)
                        .hint_text("Section title, Enter to apply")
                        .desired_width(110.0),
                );
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Section title"));
                let escape =
                    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape));
                if escape {
                    state.section_title = snapshot
                        .section
                        .as_ref()
                        .map_or_else(String::new, |section| section.title.clone());
                    if let Some(section) = &snapshot.section {
                        let (red, green, blue) = section.color.parts();
                        state.section_color = [red, green, blue];
                    }
                }
                let submitted = (response.has_focus() || response.lost_focus())
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if submitted {
                    let [red, green, blue] = state.section_color;
                    let rgb =
                        u32::from(red) * 0x1_0000 + u32::from(green) * 0x100 + u32::from(blue);
                    let old_title = snapshot
                        .section
                        .as_ref()
                        .map_or("", |section| section.title.as_str());
                    let old_rgb = snapshot.section.as_ref().map(|section| section.color.rgb());
                    let title = state.section_title.trim();
                    if title.is_empty() {
                        if snapshot.section.is_some() {
                            commands.push(Command::ClearSection {
                                measure: snapshot.measure,
                            });
                        }
                    } else if title != old_title || old_rgb != Some(rgb) {
                        commands.push(Command::SetSection {
                            measure: snapshot.measure,
                            title: title.to_owned(),
                            color: TrackColor::new(rgb),
                        });
                    }
                }
            });
            ui.end_row();
        });
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
