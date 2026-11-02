//! The edit palette (the left panel): note lengths, dots and tuplets, rests and ties, the
//! voice, the measure's tempo, signatures and section marker, and the effects matrix.
//!
//! Changes are ordinary commands (or one `batch` command for several beats), so every
//! palette click is undoable and recorded in the recovery journal.

use gscore_edit::{Command, Cursor, Selection};
use gscore_model::{
    BeatIndex, BeatKind, BeatPosition, KeySignature, MeasureIndex, NoteValue, Section, Song,
    TimeSignature, TrackColor, TrackIndex, VoiceIndex,
};

use super::GuitarScoreApp;
use super::edit_effects::{BeatState, EffectsState, NoteState};

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
        for at in &beats {
            if let Some(beat) = song.beat(*at) {
                for note in beat.notes() {
                    if let Ok(string) = note.string() {
                        note_targets.push((*at, string));
                    }
                }
            }
        }
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
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.heading("Edit palette");
                note_length(ui, &mut self.edit_palette, &snapshot, &mut commands);
                beat_buttons(ui, &mut self.edit_palette, &snapshot, &mut commands);
                ui.separator();
                header_editors(ui, &mut self.edit_palette, &snapshot, &mut commands);
                ui.separator();
                self.edit_palette
                    .effects
                    .panel(ui, &snapshot, &mut commands);
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
    /// beat. `None` without a selection.
    pub(super) fn selection_start(&self) -> Option<BeatPosition> {
        let document = self.document.as_ref()?;
        let cursor = document.editor().cursor();
        let song = document.song();
        match document.editor().selection()? {
            Selection::Beats {
                track,
                measure,
                voice,
                anchor,
                focus,
            } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                let mut fallback = None;
                for index in first..=last {
                    let at = BeatPosition {
                        track,
                        measure,
                        voice,
                        beat: BeatIndex::new(index),
                    };
                    if song
                        .beat(at)
                        .is_some_and(|beat| beat.notes().next().is_some())
                    {
                        return Some(at);
                    }
                    fallback.get_or_insert(at);
                }
                fallback
            }
            Selection::Measures { anchor, focus } => Some(BeatPosition {
                track: cursor.track,
                measure: anchor.min(focus),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(0),
            }),
        }
    }
}

/// Note length, dots and tuplets; applying one sets the length of the selected beats.
fn note_length(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    commands: &mut Vec<Command>,
) {
    ui.strong("Note length");
    ui.horizontal_wrapped(|ui| {
        for value in NOTE_VALUES {
            let label = note_value_label(value);
            if ui.selectable_label(state.value == value, label).clicked() {
                state.value = value;
                apply_duration(state, snapshot, commands);
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        for dots in 0..=2 {
            let label = if dots == 0 {
                "no dots".to_owned()
            } else {
                ".".repeat(usize::from(dots))
            };
            if ui.selectable_label(state.dots == dots, label).clicked() {
                state.dots = dots;
                apply_duration(state, snapshot, commands);
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(state.tuplet.is_none(), "no tuplet")
            .clicked()
        {
            state.tuplet = None;
            apply_duration(state, snapshot, commands);
        }
        for (enters, times) in TUPLETS {
            if ui
                .selectable_label(
                    state.tuplet == Some((enters, times)),
                    format!("{enters}:{times}"),
                )
                .clicked()
            {
                state.tuplet = Some((enters, times));
                apply_duration(state, snapshot, commands);
            }
        }
    });
}

/// Rest and tie buttons and the voice selector.
fn beat_buttons(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    commands: &mut Vec<Command>,
) {
    ui.horizontal(|ui| {
        if ui
            .button("Rest")
            .on_hover_text("Clear the selected beats")
            .clicked()
        {
            for at in &snapshot.beats {
                commands.push(Command::SetBeatRest { at: *at });
            }
        }
        if ui
            .add_enabled(
                snapshot.note_exists,
                egui::Button::new("Tie").selected(snapshot.note_tie),
            )
            .on_hover_text("Tie the note to the previous one on its string")
            .clicked()
            && let Ok(string) = gscore_model::StringNumber::new(snapshot.cursor.string)
        {
            commands.push(Command::SetTie {
                at: snapshot.cursor.position(),
                string,
                tie: !snapshot.note_tie,
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label("Voice");
        for voice in 0..2 {
            if ui
                .selectable_label(
                    snapshot.cursor.voice.get() == voice,
                    format!("{}", voice.saturating_add(1)),
                )
                .on_hover_text("Enter notes in this voice")
                .clicked()
            {
                state.pending_voice = Some(voice);
            }
        }
    });
}

/// Header fields at the cursor's measure: tempo, signatures and section marker.
fn header_editors(
    ui: &mut egui::Ui,
    state: &mut EditPaletteState,
    snapshot: &PaletteSnapshot,
    commands: &mut Vec<Command>,
) {
    ui.strong("Measure");
    ui.horizontal(|ui| {
        ui.label("Tempo");
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
    ui.horizontal(|ui| {
        ui.label("Time");
        let response = ui.add(egui::DragValue::new(&mut state.numerator).range(1..=64));
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Time numerator"));
        ui.label("/");
        let response = ui.add(
            egui::DragValue::new(&mut state.denominator)
                .range(1..=128)
                .speed(1),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Time denominator"));
        if ui.button("Set time").clicked() {
            commands.push(Command::SetTimeSignature {
                measure: snapshot.measure,
                numerator: state.numerator,
                denominator: state.denominator,
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label("Key");
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
    ui.horizontal(|ui| {
        ui.label("Section");
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.section_title)
                .hint_text("Section title")
                .desired_width(110.0),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Section title"));
    });
    ui.horizontal(|ui| {
        let response = ui.color_edit_button_srgb(&mut state.section_color);
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Section color"));
        if ui.button("Set section").clicked() {
            let [red, green, blue] = state.section_color;
            let color = TrackColor::new(
                u32::from(red) * 0x1_0000 + u32::from(green) * 0x100 + u32::from(blue),
            );
            commands.push(Command::SetSection {
                measure: snapshot.measure,
                title: state.section_title.clone(),
                color,
            });
        }
        if ui.button("Clear").clicked() {
            commands.push(Command::ClearSection {
                measure: snapshot.measure,
            });
        }
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
    let mut beats = Vec::new();
    match selection {
        Some(Selection::Beats {
            track,
            measure,
            voice,
            anchor,
            focus,
        }) => {
            let first = anchor.min(focus).get();
            let last = anchor.max(focus).get();
            for index in first..=last {
                let at = BeatPosition {
                    track,
                    measure,
                    voice,
                    beat: BeatIndex::new(index),
                };
                if song.beat(at).is_some() {
                    beats.push(at);
                }
            }
        }
        Some(Selection::Measures { anchor, focus }) => {
            let first = anchor.min(focus).get();
            let last = anchor.max(focus).get();
            for track in 0..song.track_count() {
                for index in first..=last {
                    let measure = MeasureIndex::new(index);
                    let Some(view) = song
                        .track(TrackIndex::new(track))
                        .and_then(|t| t.measure(measure))
                    else {
                        continue;
                    };
                    for (voice_index, voice) in view.voices().enumerate() {
                        for (beat_index, beat) in voice.beats().enumerate() {
                            if beat.kind() != BeatKind::Empty {
                                beats.push(BeatPosition {
                                    track: TrackIndex::new(track),
                                    measure,
                                    voice: VoiceIndex::new(voice_index),
                                    beat: BeatIndex::new(beat_index),
                                });
                            }
                        }
                    }
                }
            }
        }
        None => {
            if song.beat(cursor.position()).is_some() {
                beats.push(cursor.position());
            }
        }
    }
    beats
}

/// "1", "1/2", ... for a note value.
fn note_value_label(value: u16) -> String {
    if value == 1 {
        "1".to_owned()
    } else {
        format!("1/{value}")
    }
}
