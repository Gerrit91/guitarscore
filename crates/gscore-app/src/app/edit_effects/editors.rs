//! The effect editor windows (bend, whammy bar, harmonic): opening them on the cursor's
//! note or beat, showing them, and turning OK, Remove and Listen into commands.

use gscore_edit::Command;
use gscore_model::{BeatEffect, BendKind, BendShape, NoteEffect, WhammyShape};

use super::super::edit_palette::PaletteSnapshot;
use super::super::harmonic_editor::HarmonicEditor;
use super::super::shape_editor::{Outcome, ShapeEditor, ShapeTarget};
use super::{EffectsState, apply_beat, apply_note, default_bend, default_whammy};
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

    /// Whether the bend or whammy editor is open.
    pub(in crate::app) fn is_editing(&self) -> bool {
        self.bend.is_some() || self.whammy.is_some() || self.harmonic.is_some()
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
