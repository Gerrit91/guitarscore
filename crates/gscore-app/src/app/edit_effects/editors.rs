//! The effect editor windows (bend, whammy bar, harmonic, grace note): opening them on the
//! cursor's
//! note or beat, showing them, and turning OK, Remove and Listen into commands.

use gscore_edit::Command;
use gscore_model::{
    BeatEffect, BendKind, BendShape, Finger, Hand, NoteEffect, StrokeDirection, WhammyShape,
};

use super::super::beat_text_editor::BeatTextEditor;
use super::super::chord_editor::ChordEditor;
use super::super::edit_palette::PaletteSnapshot;
use super::super::grace_editor::GraceEditor;
use super::super::harmonic_editor::HarmonicEditor;
use super::super::lyrics_editor::LyricsEditor;
use super::super::shape_editor::{Outcome, ShapeEditor, ShapeTarget};
use super::dialogs::{
    FingerEditor, FingerOutcome, StrokeEditor, StrokeEditorKind, TremoloEditor, TrillEditor,
};
use super::{EffectsState, LyricsPreview, apply_beat, apply_note, default_bend, default_whammy};
use crate::settings::{Settings, ShapePreset};

impl EffectsState {
    /// Opens the bend editor on the cursor note's bend, or on a full-step bend when the
    /// note has none. Nothing changes until OK.
    pub(in crate::app) fn open_bend(&mut self, snapshot: &PaletteSnapshot) {
        if snapshot.note_targets.is_empty() {
            return;
        }
        let bend = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
            .and_then(|note| note.bend.clone());
        self.bend = Some(match bend {
            Some(bend) => ShapeEditor::new(ShapeTarget::Bend, bend.kind, bend.points),
            None => ShapeEditor::new(ShapeTarget::Bend, BendKind::Up, default_bend()),
        });
    }

    /// Opens the whammy bar editor on the cursor beat's shape, or on a dip when the beat
    /// has none. Nothing changes until OK.
    pub(in crate::app) fn open_whammy(&mut self, snapshot: &PaletteSnapshot) {
        if snapshot.beats.is_empty() {
            return;
        }
        let shape = snapshot
            .beat_state
            .as_ref()
            .or(snapshot.beat_states.first())
            .and_then(|beat| beat.whammy.clone());
        let points = shape.map_or_else(default_whammy, |shape| shape.points);
        self.whammy = Some(ShapeEditor::new(ShapeTarget::Whammy, BendKind::Up, points));
    }

    /// Whether an open editor takes the keys: every one but the fingering windows, which
    /// let the cursor move through the score while they are open.
    pub(in crate::app) fn takes_keys(&self) -> bool {
        self.bend.is_some()
            || self.whammy.is_some()
            || self.harmonic.is_some()
            || self.grace.is_some()
            || self.trill.is_some()
            || self.tremolo.is_some()
            || self.beat_text.is_some()
            || self.chord.is_some()
            || self.lyrics.is_some()
            || self.stroke.is_some()
    }

    /// Whether the bend or whammy editor is open.
    pub(in crate::app) fn is_editing(&self) -> bool {
        self.bend.is_some()
            || self.whammy.is_some()
            || self.harmonic.is_some()
            || self.grace.is_some()
            || self.trill.is_some()
            || self.tremolo.is_some()
            || self.beat_text.is_some()
            || self.chord.is_some()
            || self.lyrics.is_some()
            || self.finger_left.is_some()
            || self.finger_right.is_some()
            || self.stroke.is_some()
    }

    /// Shows the open bend or whammy editor; OK and Remove turn into commands for the
    /// selected notes or beats.
    pub(in crate::app) fn show_editors(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        song: &gscore_model::Song,
        settings: &mut Settings,
        commands: &mut Vec<Command>,
        trial: &mut Vec<Command>,
    ) {
        self.show_bend(ctx, snapshot, &mut settings.bend_presets, commands, trial);
        self.show_whammy(ctx, snapshot, &mut settings.whammy_presets, commands, trial);
        self.show_harmonic(ctx, snapshot, song, commands, trial);
        self.show_grace(ctx, snapshot, commands, trial);
        self.show_trill(ctx, snapshot, commands);
        self.show_tremolo(ctx, snapshot, commands);
        self.show_beat_text(ctx, snapshot, commands);
        self.show_chord(ctx, snapshot, commands);
        self.show_lyrics(ctx, snapshot, commands);
        self.show_finger(ctx, snapshot, commands, Hand::Left);
        self.show_finger(ctx, snapshot, commands, Hand::Right);
        self.show_stroke(ctx, snapshot, commands);
    }

    /// Opens the brush or arpeggio dialog on the cursor beat's stroke (or on `kind`'s
    /// defaults in `direction`). Nothing changes until OK.
    pub(in crate::app) fn open_stroke(
        &mut self,
        snapshot: &PaletteSnapshot,
        kind: StrokeEditorKind,
        direction: StrokeDirection,
    ) {
        let current = snapshot
            .beat_state
            .as_ref()
            .or(snapshot.beat_states.first())
            .and_then(|beat| match kind {
                StrokeEditorKind::Brush => beat.brush,
                StrokeEditorKind::Arpeggio => beat.arpeggio,
            });
        self.stroke = Some(StrokeEditor::new(kind, current, direction));
    }

    fn show_stroke(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.stroke else {
            return;
        };
        let kind = editor.kind();
        let effect = match editor.show(ctx) {
            Outcome::Open => return,
            Outcome::Apply => Some(match kind {
                StrokeEditorKind::Brush => BeatEffect::BrushStroke(Some(editor.stroke())),
                StrokeEditorKind::Arpeggio => BeatEffect::Arpeggio(Some(editor.stroke())),
            }),
            Outcome::Remove => Some(match kind {
                StrokeEditorKind::Brush => BeatEffect::BrushStroke(None),
                StrokeEditorKind::Arpeggio => BeatEffect::Arpeggio(None),
            }),
            Outcome::Cancel | Outcome::Listen => None,
        };
        if let Some(effect) = effect {
            apply_beat(commands, &snapshot.beats, effect);
        }
        self.stroke = None;
    }

    /// Opens the fingering window for `hand` (closing the other hand's: one at a time, so
    /// the keys are clear).
    pub(in crate::app) fn open_finger(&mut self, _snapshot: &PaletteSnapshot, hand: Hand) {
        let editor = FingerEditor::new(hand);
        match hand {
            Hand::Left => {
                self.finger_right = None;
                self.finger_left = Some(editor);
            }
            Hand::Right => {
                self.finger_left = None;
                self.finger_right = Some(editor);
            }
        }
    }

    /// The hand whose fingering window is open, if one is.
    pub(in crate::app) fn finger_hand(&self) -> Option<Hand> {
        self.finger_left
            .as_ref()
            .or(self.finger_right.as_ref())
            .map(FingerEditor::hand)
    }

    /// A finger chosen with a key while the fingering window is open.
    pub(in crate::app) fn choose_finger(&mut self, finger: Option<Finger>) {
        self.finger_key = Some(finger);
    }

    fn show_finger(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
        hand: Hand,
    ) {
        let editor = match hand {
            Hand::Left => &mut self.finger_left,
            Hand::Right => &mut self.finger_right,
        };
        let Some(editor) = editor else {
            return;
        };
        // Always the cursor note's finger: the window follows the cursor.
        let current = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
            .and_then(|note| match hand {
                Hand::Left => note.finger_left,
                Hand::Right => note.finger_right,
            });
        let has_note = !snapshot.note_targets.is_empty();
        let outcome = match (editor.show(ctx, current, has_note), self.finger_key.take()) {
            (FingerOutcome::Open, Some(finger)) => FingerOutcome::Set(finger),
            (outcome, _) => outcome,
        };
        match outcome {
            FingerOutcome::Open => {}
            FingerOutcome::Set(finger) => {
                if has_note {
                    apply_note(
                        commands,
                        &snapshot.note_targets,
                        NoteEffect::Finger(hand, finger),
                    );
                }
            }
            FingerOutcome::Close => match hand {
                Hand::Left => self.finger_left = None,
                Hand::Right => self.finger_right = None,
            },
        }
    }

    /// Opens the chord diagram dialog on the cursor beat's diagram, or on an empty one.
    pub(in crate::app) fn open_chord(&mut self, snapshot: &PaletteSnapshot) {
        if snapshot.beats.is_empty() {
            return;
        }
        self.chord = Some(ChordEditor::new(
            snapshot.chord.as_ref(),
            snapshot.string_count,
            &snapshot.beat_shape,
        ));
    }

    fn show_chord(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.chord else {
            return;
        };
        let outcome = editor.show(ctx, snapshot.chord.is_some());
        match outcome {
            Outcome::Open => return,
            Outcome::Apply => commands.push(Command::SetChordDiagram {
                at: snapshot.cursor.position(),
                diagram: Some(editor.diagram()),
            }),
            Outcome::Remove => commands.push(Command::SetChordDiagram {
                at: snapshot.cursor.position(),
                diagram: None,
            }),
            Outcome::Cancel | Outcome::Listen => {}
        }
        self.chord = None;
    }

    /// Opens the lyrics dialog on the cursor's measure.
    pub(in crate::app) fn open_lyrics(&mut self, snapshot: &PaletteSnapshot) {
        self.lyrics = Some(LyricsEditor::new(
            snapshot.lyric_lines.clone(),
            snapshot.measure.get(),
            snapshot.measure_count,
        ));
    }

    fn show_lyrics(
        &mut self,
        ctx: &egui::Context,
        _snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.lyrics else {
            return;
        };
        let outcome = editor.show(ctx);
        let show = |editor: &LyricsEditor| Command::SetLyricLine {
            line: editor.line(),
            start: gscore_model::MeasureIndex::new(editor.start()),
            text: editor.text().to_owned(),
        };
        match outcome {
            Outcome::Open => {
                // What is typed shows in the score at once.
                if editor.take_change() {
                    self.lyrics_preview = Some(LyricsPreview::Show(show(editor)));
                }
                return;
            }
            Outcome::Apply => {
                if editor.take_change() {
                    self.lyrics_preview = Some(LyricsPreview::Show(show(editor)));
                } else {
                    self.lyrics_preview = Some(LyricsPreview::Done);
                }
            }
            Outcome::Remove => commands.push(Command::SetLyricLine {
                line: editor.line(),
                start: gscore_model::MeasureIndex::new(editor.start()),
                text: String::new(),
            }),
            Outcome::Cancel | Outcome::Listen => {
                if let Some(depth) = editor.undo_start() {
                    self.lyrics_preview = Some(LyricsPreview::Revert(depth));
                }
            }
        }
        self.lyrics = None;
    }

    /// What the lyrics dialog asks of the score this frame, if anything.
    pub(in crate::app) fn take_lyrics_preview(&mut self) -> Option<LyricsPreview> {
        self.lyrics_preview.take()
    }

    /// Whether the lyrics dialog is open.
    pub(in crate::app) fn lyrics_open(&self) -> bool {
        self.lyrics.is_some()
    }

    /// Remembers the undo depth before the lyrics dialog's first change in the score.
    pub(in crate::app) fn note_lyrics_undo_start(&mut self, depth: usize) {
        if let Some(editor) = &mut self.lyrics {
            editor.note_undo_start(depth);
        }
    }

    /// Opens the beat text dialog on the cursor beat's text.
    pub(in crate::app) fn open_beat_text(&mut self, snapshot: &PaletteSnapshot) {
        self.beat_text = Some(BeatTextEditor::new(&snapshot.beat_text));
    }

    fn show_beat_text(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.beat_text else {
            return;
        };
        let outcome = editor.show(ctx);
        match outcome {
            Outcome::Open => return,
            Outcome::Apply => commands.push(Command::SetBeatText {
                at: snapshot.cursor.position(),
                text: editor.text().to_owned(),
            }),
            Outcome::Remove => commands.push(Command::SetBeatText {
                at: snapshot.cursor.position(),
                text: String::new(),
            }),
            Outcome::Cancel | Outcome::Listen => {}
        }
        self.beat_text = None;
    }

    /// Opens the trill dialog on the cursor note's trill (or the first selected note's),
    /// or on two frets above the note.
    pub(in crate::app) fn open_trill(&mut self, snapshot: &PaletteSnapshot) {
        let Some(note) = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
        else {
            return;
        };
        self.trill = Some(TrillEditor::new(note.trill, note.fret));
    }

    fn show_trill(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.trill else {
            return;
        };
        let effect = match editor.show(ctx) {
            Outcome::Open => return,
            Outcome::Apply => Some(NoteEffect::Trill(Some(editor.trill()))),
            Outcome::Remove => Some(NoteEffect::Trill(None)),
            Outcome::Cancel | Outcome::Listen => None,
        };
        if let Some(effect) = effect {
            apply_note(commands, &snapshot.note_targets, effect);
        }
        self.trill = None;
    }

    /// Opens the tremolo dialog on the cursor note's speed (or the first selected note's).
    pub(in crate::app) fn open_tremolo(&mut self, snapshot: &PaletteSnapshot) {
        let Some(note) = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
        else {
            return;
        };
        self.tremolo = Some(TremoloEditor::new(note.tremolo_picking));
    }

    fn show_tremolo(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.tremolo else {
            return;
        };
        let effect = match editor.show(ctx) {
            Outcome::Open => return,
            Outcome::Apply => Some(NoteEffect::TremoloPicking(Some(editor.speed()))),
            Outcome::Remove => Some(NoteEffect::TremoloPicking(None)),
            Outcome::Cancel | Outcome::Listen => None,
        };
        if let Some(effect) = effect {
            apply_note(commands, &snapshot.note_targets, effect);
        }
        self.tremolo = None;
    }

    /// Opens the grace note dialog on the cursor note's grace note (or the first selected
    /// note's), or on a new one two frets below the note.
    pub(in crate::app) fn open_grace(&mut self, snapshot: &PaletteSnapshot) {
        let Some(note) = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
        else {
            return;
        };
        self.grace = Some(GraceEditor::new(note.grace, note.fret));
    }

    fn show_grace(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
        trial: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.grace else {
            return;
        };
        let grace = editor.grace();
        match editor.show(ctx) {
            Outcome::Open => return,
            Outcome::Listen => {
                apply_note(
                    trial,
                    &snapshot.note_targets,
                    NoteEffect::Grace(Some(grace)),
                );
                return;
            }
            Outcome::Apply => apply_note(
                commands,
                &snapshot.note_targets,
                NoteEffect::Grace(Some(grace)),
            ),
            Outcome::Remove => {
                apply_note(commands, &snapshot.note_targets, NoteEffect::Grace(None))
            }
            Outcome::Cancel => {}
        }
        self.grace = None;
    }

    /// Opens the harmonic editor on the cursor's note (or the first selected one).
    pub(in crate::app) fn open_harmonic(&mut self, snapshot: &PaletteSnapshot) {
        let Some(note) = snapshot
            .note_state
            .as_ref()
            .or(snapshot.target_states.first())
        else {
            return;
        };
        self.harmonic = Some(HarmonicEditor::new(note.harmonic, note.fret));
    }

    fn show_harmonic(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        song: &gscore_model::Song,
        commands: &mut Vec<Command>,
        trial: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.harmonic else {
            return;
        };
        let draft = |editor: &HarmonicEditor| -> Vec<Command> {
            snapshot
                .note_targets
                .iter()
                .zip(&snapshot.target_states)
                .map(|(&(at, string), state)| Command::SetNoteEffect {
                    at,
                    string,
                    effect: NoteEffect::Harmonic(Some(editor.harmonic(state.fret))),
                })
                .collect()
        };
        editor.sounds = sounding_pitch(song, &draft(editor));
        match editor.show(ctx) {
            Outcome::Open => return,
            Outcome::Listen => {
                trial.extend(draft(editor));
                return;
            }
            Outcome::Apply => commands.extend(draft(editor)),
            Outcome::Remove => {
                apply_note(commands, &snapshot.note_targets, NoteEffect::Harmonic(None))
            }
            Outcome::Cancel => {}
        }
        self.harmonic = None;
    }

    fn show_bend(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        presets: &mut Vec<ShapePreset>,
        commands: &mut Vec<Command>,
        trial: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.bend else {
            return;
        };
        let effect = match editor.show(ctx, presets) {
            Outcome::Open => return,
            Outcome::Listen => {
                match BendShape::new(editor.kind, editor.points.clone()) {
                    Ok(shape) => {
                        apply_note(trial, &snapshot.note_targets, NoteEffect::Bend(Some(shape)))
                    }
                    Err(error) => editor.error = Some(error.to_string()),
                }
                return;
            }
            Outcome::Cancel => None,
            Outcome::Remove => Some(NoteEffect::Bend(None)),
            Outcome::Apply => match BendShape::new(editor.kind, editor.points.clone()) {
                Ok(shape) => Some(NoteEffect::Bend(Some(shape))),
                Err(error) => {
                    editor.error = Some(error.to_string());
                    return;
                }
            },
        };
        if let Some(effect) = effect {
            apply_note(commands, &snapshot.note_targets, effect);
        }
        self.bend = None;
    }

    fn show_whammy(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        presets: &mut Vec<ShapePreset>,
        commands: &mut Vec<Command>,
        trial: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.whammy else {
            return;
        };
        let effect = match editor.show(ctx, presets) {
            Outcome::Open => return,
            Outcome::Listen => {
                match WhammyShape::new(editor.points.clone()) {
                    Ok(shape) => {
                        apply_beat(trial, &snapshot.beats, BeatEffect::Whammy(Some(shape)))
                    }
                    Err(error) => editor.error = Some(error.to_string()),
                }
                return;
            }
            Outcome::Cancel => None,
            Outcome::Remove => Some(BeatEffect::Whammy(None)),
            Outcome::Apply => match WhammyShape::new(editor.points.clone()) {
                Ok(shape) => Some(BeatEffect::Whammy(Some(shape))),
                Err(error) => {
                    editor.error = Some(error.to_string());
                    return;
                }
            },
        };
        if let Some(effect) = effect {
            apply_beat(commands, &snapshot.beats, effect);
        }
        self.whammy = None;
    }
}

/// The pitch the first note of `commands` sounds with them applied (to a copy of `song`),
/// as "E5".
fn sounding_pitch(song: &gscore_model::Song, commands: &[Command]) -> Option<String> {
    let Command::SetNoteEffect { at, string, .. } = commands.first()? else {
        return None;
    };
    let mut draft = song.clone();
    for command in commands {
        let _ = command.apply(&mut draft).ok()?;
    }
    let track = draft.track(at.track)?;
    let beat = draft.beat(*at)?;
    let note = beat
        .notes()
        .find(|note| note.string().ok() == Some(*string))?;
    track
        .sounding_pitch(&note)
        .ok()
        .map(|pitch| pitch.to_string())
}
