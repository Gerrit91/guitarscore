//! The effects half of the edit palette: the note and beat effect matrix, and the bend and
//! whammy shape editors. It shows the state of the note under the cursor and applies a
//! click to every note or beat in the selection as commands.

use egui::vec2;
use gscore_edit::{ActionId, Command};
use gscore_model::{
    AccentKind, BeatEffect, BeatView, BendKind, BendPoint, BendShape, Harmonic, HarmonicKind,
    Legato, NoteEffect, NoteView, SlapKind, SlideKind, StrokeDirection, Vibrato, WhammyShape,
};

use super::edit_palette::PaletteSnapshot;
use super::shape_editor::{Outcome, ShapeEditor, ShapeTarget};
use super::style;
use super::tiles::{self, Face};
use crate::settings::{Settings, ShapePreset};

/// The effects of the note under the cursor.
#[derive(Debug, Clone)]
pub(super) struct NoteState {
    dead: bool,
    hammer: bool,
    ghost: bool,
    palm_mute: bool,
    let_ring: bool,
    staccato: bool,
    trill: bool,
    tremolo_picking: bool,
    accent: Option<AccentKind>,
    vibrato: Option<Vibrato>,
    harmonic: Option<Harmonic>,
    slides: Vec<SlideKind>,
    bend: Option<BendShape>,
}

impl NoteState {
    pub(super) fn of(note: &NoteView<'_>) -> Self {
        NoteState {
            dead: note.kind() == gscore_model::NoteKind::Dead,
            hammer: note.is_hammer(),
            ghost: note.is_ghost(),
            palm_mute: note.is_palm_muted(),
            let_ring: note.is_let_ring(),
            staccato: note.is_staccato(),
            trill: note.has_trill(),
            tremolo_picking: note.has_tremolo_picking(),
            accent: note.accent(),
            vibrato: note.vibrato(),
            harmonic: note.harmonic(),
            slides: note.slides().collect(),
            bend: note.bend_shape(),
        }
    }
}

/// The effects of the beat under the cursor.
#[derive(Debug, Clone)]
pub(super) struct BeatState {
    pick: Option<StrokeDirection>,
    brush: Option<StrokeDirection>,
    slap: Option<SlapKind>,
    fade_in: bool,
    whammy: Option<WhammyShape>,
    legato: Option<Legato>,
    vibrato: Option<Vibrato>,
}

impl BeatState {
    pub(super) fn of(beat: &BeatView<'_>) -> Self {
        BeatState {
            pick: beat.pick_stroke(),
            brush: beat.brush_stroke(),
            slap: beat.slap(),
            fade_in: beat.has_fade_in(),
            whammy: beat.whammy_shape(),
            legato: beat.legato(),
            vibrato: beat.vibrato(),
        }
    }
}

/// The effects matrix and the shape editors.
#[derive(Debug, Default)]
pub(super) struct EffectsState {
    bend: Option<ShapeEditor>,
    whammy: Option<ShapeEditor>,
}

/// What an effect tile (or its key) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fx {
    Dead,
    Ghost,
    Hammer,
    PalmMute,
    LetRing,
    Staccato,
    Trill,
    Tremolo,
    Slide(SlideKind),
    Bend,
    Vibrato(Vibrato),
    Accent(AccentKind),
    Harmonic(HarmonicKind),
    Pick(StrokeDirection),
    Brush(StrokeDirection),
    Slap(SlapKind),
    FadeIn,
    Whammy,
    Legato,
    BeatVibrato(Vibrato),
}

impl Fx {
    /// Whether the effect belongs to a note (else to a beat).
    fn on_note(self) -> bool {
        !matches!(
            self,
            Fx::Pick(_)
                | Fx::Brush(_)
                | Fx::Slap(_)
                | Fx::FadeIn
                | Fx::Whammy
                | Fx::Legato
                | Fx::BeatVibrato(_)
        )
    }
}

/// One effect tile: its action (for the key and the description), face and effect.
#[derive(Debug, Clone, Copy)]
pub(super) struct FxItem {
    pub(super) action: ActionId,
    pub(super) face: Face<'static>,
    pub(super) fx: Fx,
}

const fn glyph(text: &'static str, size: f32, baseline: f32) -> Face<'static> {
    Face::Glyph {
        text,
        size,
        baseline,
    }
}

const fn item(action: ActionId, face: Face<'static>, fx: Fx) -> FxItem {
    FxItem { action, face, fx }
}

/// Note techniques, in palette order (SMuFL glyphs where the font has one).
const NOTE_ITEMS: &[FxItem] = &[
    item(
        ActionId::ToggleDeadNote,
        glyph("\u{E0A9}", 34.0, 0.0),
        Fx::Dead,
    ),
    item(
        ActionId::ToggleGhostNote,
        glyph("\u{E0CE}", 40.0, 0.0),
        Fx::Ghost,
    ),
    item(ActionId::ToggleHammer, Face::Hammer, Fx::Hammer),
    item(
        ActionId::ToggleAccent,
        glyph("\u{E4A0}", 44.0, 4.0),
        Fx::Accent(AccentKind::Normal),
    ),
    item(
        ActionId::ToggleHeavyAccent,
        glyph("\u{E4AC}", 40.0, 5.0),
        Fx::Accent(AccentKind::Heavy),
    ),
    item(
        ActionId::ToggleStaccato,
        glyph("\u{E4A2}", 60.0, 3.0),
        Fx::Staccato,
    ),
    item(ActionId::TogglePalmMute, Face::Text("P.M."), Fx::PalmMute),
    item(
        ActionId::ToggleLetRing,
        Face::Word("let\nring"),
        Fx::LetRing,
    ),
    item(
        ActionId::ToggleVibrato,
        glyph("\u{EAB2}\u{EAB2}\u{EAB2}", 30.0, 3.0),
        Fx::Vibrato(Vibrato::Slight),
    ),
    item(
        ActionId::ToggleWideVibrato,
        glyph("\u{EAB3}\u{EAB3}", 28.0, 4.0),
        Fx::Vibrato(Vibrato::Wide),
    ),
    item(
        ActionId::ToggleTrill,
        glyph("\u{E566}", 30.0, 5.0),
        Fx::Trill,
    ),
    item(
        ActionId::ToggleTremoloPicking,
        glyph("\u{E222}", 28.0, 4.0),
        Fx::Tremolo,
    ),
];

/// Slides.
const SLIDE_ITEMS: &[FxItem] = &[
    item(
        ActionId::ToggleShiftSlide,
        Face::Slide { from: 0, to: 1 },
        Fx::Slide(SlideKind::ShiftSlideTo),
    ),
    item(
        ActionId::ToggleLegatoSlide,
        Face::Slide { from: 0, to: -1 },
        Fx::Slide(SlideKind::LegatoSlideTo),
    ),
    item(
        ActionId::ToggleSlideInBelow,
        Face::Slide { from: -1, to: 0 },
        Fx::Slide(SlideKind::IntoFromBelow),
    ),
    item(
        ActionId::ToggleSlideInAbove,
        Face::Slide { from: 1, to: 0 },
        Fx::Slide(SlideKind::IntoFromAbove),
    ),
    item(
        ActionId::ToggleSlideOutDown,
        Face::Slide { from: 0, to: -1 },
        Fx::Slide(SlideKind::OutDownwards),
    ),
    item(
        ActionId::ToggleSlideOutUp,
        Face::Slide { from: 0, to: 1 },
        Fx::Slide(SlideKind::OutUpwards),
    ),
];

/// The bend, the whammy bar (a beat effect, but its neighbour) and the harmonics.
const PITCH_ITEMS: &[FxItem] = &[
    item(ActionId::ToggleBend, Face::Bend, Fx::Bend),
    item(ActionId::ToggleWhammy, Face::Whammy, Fx::Whammy),
    item(
        ActionId::ToggleNaturalHarmonic,
        glyph("\u{E0DD}", 42.0, 0.0),
        Fx::Harmonic(HarmonicKind::Natural),
    ),
    item(
        ActionId::ToggleArtificialHarmonic,
        Face::Text("A.H."),
        Fx::Harmonic(HarmonicKind::Artificial),
    ),
    item(
        ActionId::ToggleTappedHarmonic,
        Face::Text("T.H."),
        Fx::Harmonic(HarmonicKind::Tapped),
    ),
    item(
        ActionId::TogglePinchHarmonic,
        Face::Text("P.H."),
        Fx::Harmonic(HarmonicKind::Pinch),
    ),
    item(
        ActionId::ToggleSemiHarmonic,
        Face::Text("S.H."),
        Fx::Harmonic(HarmonicKind::Semi),
    ),
];

/// Beat techniques.
const BEAT_ITEMS: &[FxItem] = &[
    item(
        ActionId::TogglePickDown,
        glyph("\u{E610}", 34.0, 4.0),
        Fx::Pick(StrokeDirection::Down),
    ),
    item(
        ActionId::TogglePickUp,
        glyph("\u{E612}", 34.0, 7.0),
        Fx::Pick(StrokeDirection::Up),
    ),
    item(
        ActionId::ToggleBrushDown,
        glyph("\u{E635}", 15.0, 10.0),
        Fx::Brush(StrokeDirection::Down),
    ),
    item(
        ActionId::ToggleBrushUp,
        glyph("\u{E634}", 15.0, 10.0),
        Fx::Brush(StrokeDirection::Up),
    ),
    item(
        ActionId::ToggleTapping,
        Face::Word("tap"),
        Fx::Slap(SlapKind::Tapping),
    ),
    item(
        ActionId::ToggleSlapping,
        Face::Word("slap"),
        Fx::Slap(SlapKind::Slapping),
    ),
    item(
        ActionId::TogglePopping,
        Face::Word("pop"),
        Fx::Slap(SlapKind::Popping),
    ),
    item(ActionId::ToggleFadeIn, Face::Hairpin, Fx::FadeIn),
    item(ActionId::ToggleLegato, Face::Slur, Fx::Legato),
    item(
        ActionId::ToggleBeatVibrato,
        glyph("\u{EAB2}\u{EAB2}\u{EAB2}", 30.0, 3.0),
        Fx::BeatVibrato(Vibrato::Slight),
    ),
];

/// The effect item bound to `action`, for a key press.
pub(super) fn fx_item(action: ActionId) -> Option<&'static FxItem> {
    [NOTE_ITEMS, SLIDE_ITEMS, PITCH_ITEMS, BEAT_ITEMS]
        .into_iter()
        .flatten()
        .find(|item| item.action == action)
}

/// Whether the effect bound to `action` is on for the selected notes or beats, as its
/// palette tile shows it; `None` for an action without a tile.
pub(super) fn action_is_on(action: ActionId, snapshot: &PaletteSnapshot) -> Option<bool> {
    fx_item(action).map(|item| is_on_for_targets(item.fx, snapshot))
}

/// The beat state when the cursor is on no beat.
fn no_beat() -> BeatState {
    BeatState {
        pick: None,
        brush: None,
        slap: None,
        fade_in: false,
        whammy: None,
        legato: None,
        vibrato: None,
    }
}

/// Whether `fx` is on for one note or beat.
fn is_on(fx: Fx, note: Option<&NoteState>, beat: &BeatState) -> bool {
    let note_on = |test: fn(&NoteState) -> bool| note.is_some_and(test);
    match fx {
        Fx::Dead => note_on(|n| n.dead),
        Fx::Ghost => note_on(|n| n.ghost),
        Fx::Hammer => note_on(|n| n.hammer),
        Fx::PalmMute => note_on(|n| n.palm_mute),
        Fx::LetRing => note_on(|n| n.let_ring),
        Fx::Staccato => note_on(|n| n.staccato),
        Fx::Trill => note_on(|n| n.trill),
        Fx::Tremolo => note_on(|n| n.tremolo_picking),
        Fx::Bend => note_on(|n| n.bend.is_some()),
        Fx::Slide(kind) => note.is_some_and(|n| n.slides.contains(&kind)),
        Fx::Vibrato(kind) => note.is_some_and(|n| n.vibrato == Some(kind)),
        Fx::Accent(kind) => note.is_some_and(|n| n.accent == Some(kind)),
        Fx::Harmonic(kind) => note.is_some_and(|n| n.harmonic.is_some_and(|h| h.kind == kind)),
        Fx::Pick(direction) => beat.pick == Some(direction),
        Fx::Brush(direction) => beat.brush == Some(direction),
        Fx::Slap(kind) => beat.slap == Some(kind),
        Fx::FadeIn => beat.fade_in,
        Fx::Whammy => beat.whammy.is_some(),
        Fx::Legato => beat.legato.is_some(),
        Fx::BeatVibrato(kind) => beat.vibrato == Some(kind),
    }
}

/// Whether `fx` is on for the selection: every note it applies to (note effects) or every
/// selected beat (beat effects) has it. Then a click takes it off them all; otherwise a
/// click puts it on them all. Without targets it is off.
fn is_on_for_targets(fx: Fx, snapshot: &PaletteSnapshot) -> bool {
    let none = no_beat();
    if fx.on_note() {
        !snapshot.target_states.is_empty()
            && snapshot
                .target_states
                .iter()
                .all(|note| is_on(fx, Some(note), &none))
    } else {
        !snapshot.beat_states.is_empty()
            && snapshot
                .beat_states
                .iter()
                .all(|beat| is_on(fx, None, beat))
    }
}

/// The commands that switch `fx` on the selected notes or beats: on when it is off at the
/// cursor, off when it is on.
pub(super) fn fx_commands(fx: Fx, snapshot: &PaletteSnapshot) -> Vec<Command> {
    let note = snapshot
        .note_state
        .as_ref()
        .or(snapshot.target_states.first());
    let on = is_on_for_targets(fx, snapshot);
    let mut commands = Vec::new();
    let targets = &snapshot.note_targets;
    let beats = &snapshot.beats;
    let note_effect = match fx {
        Fx::Dead => {
            for (at, string) in targets {
                commands.push(Command::SetDeadNote {
                    at: *at,
                    string: *string,
                    dead: !on,
                });
            }
            return commands;
        }
        Fx::Ghost => Some(NoteEffect::Ghost(!on)),
        Fx::Hammer => Some(NoteEffect::Hammer(!on)),
        Fx::PalmMute => Some(NoteEffect::PalmMute(!on)),
        Fx::LetRing => Some(NoteEffect::LetRing(!on)),
        Fx::Staccato => Some(NoteEffect::Staccato(!on)),
        Fx::Trill => Some(NoteEffect::Trill(!on)),
        Fx::Tremolo => Some(NoteEffect::TremoloPicking(!on)),
        Fx::Slide(kind) => Some(NoteEffect::Slide(kind, !on)),
        Fx::Bend => Some(NoteEffect::Bend(if on {
            None
        } else {
            BendShape::new(BendKind::Up, default_bend()).ok()
        })),
        Fx::Vibrato(kind) => Some(NoteEffect::Vibrato((!on).then_some(kind))),
        Fx::Accent(kind) => Some(NoteEffect::Accent((!on).then_some(kind))),
        Fx::Harmonic(kind) => Some(NoteEffect::Harmonic((!on).then(|| Harmonic {
            kind,
            node: note.and_then(|n| n.harmonic.and_then(|h| h.node)),
        }))),
        _ => None,
    };
    if let Some(effect) = note_effect {
        apply_note(&mut commands, targets, effect);
        return commands;
    }
    match fx {
        Fx::Legato => {
            // A run: the first beat starts it, the last ends it, the ones between do both.
            let last = beats.len().saturating_sub(1);
            for (index, at) in beats.iter().enumerate() {
                let legato = (!on).then_some(Legato {
                    origin: index < last || beats.len() == 1,
                    destination: index > 0,
                });
                commands.push(Command::SetBeatEffect {
                    at: *at,
                    effect: BeatEffect::Legato(legato),
                });
            }
        }
        Fx::Pick(direction) => {
            apply_beat(
                &mut commands,
                beats,
                BeatEffect::PickStroke((!on).then_some(direction)),
            );
        }
        Fx::Brush(direction) => {
            apply_beat(
                &mut commands,
                beats,
                BeatEffect::BrushStroke((!on).then_some(direction)),
            );
        }
        Fx::Slap(kind) => apply_beat(
            &mut commands,
            beats,
            BeatEffect::Slap((!on).then_some(kind)),
        ),
        Fx::FadeIn => apply_beat(&mut commands, beats, BeatEffect::FadeIn(!on)),
        Fx::Whammy => {
            let shape = if on {
                None
            } else {
                WhammyShape::new(default_whammy()).ok()
            };
            apply_beat(&mut commands, beats, BeatEffect::Whammy(shape));
        }
        Fx::BeatVibrato(kind) => {
            apply_beat(
                &mut commands,
                beats,
                BeatEffect::Vibrato((!on).then_some(kind)),
            );
        }
        _ => {}
    }
    commands
}

impl EffectsState {
    /// Opens the bend editor on the cursor note's bend, or on a full-step bend when the
    /// note has none. Nothing changes until OK.
    pub(super) fn open_bend(&mut self, snapshot: &PaletteSnapshot) {
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
    pub(super) fn open_whammy(&mut self, snapshot: &PaletteSnapshot) {
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
    pub(super) fn is_editing(&self) -> bool {
        self.bend.is_some() || self.whammy.is_some()
    }

    /// Closes the bend and whammy editors without a change.
    pub(super) fn close_editors(&mut self) {
        self.bend = None;
        self.whammy = None;
    }

    /// Shows the open bend or whammy editor; OK and Remove turn into commands for the
    /// selected notes or beats.
    pub(super) fn show_editors(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        settings: &mut Settings,
        commands: &mut Vec<Command>,
    ) {
        self.show_bend(ctx, snapshot, &mut settings.bend_presets, commands);
        self.show_whammy(ctx, snapshot, &mut settings.whammy_presets, commands);
    }

    /// Draws the effect tiles; clicks turn into commands (the bend tile opens the bend
    /// editor). `hint` gives an action's keys for the tooltips (" (P)").
    pub(super) fn panel(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &PaletteSnapshot,
        hint: &dyn Fn(ActionId) -> String,
        commands: &mut Vec<Command>,
    ) {
        let note = snapshot.note_state.as_ref();
        let beat = snapshot.beat_state.clone().unwrap_or_else(no_beat);
        let has_notes = !snapshot.note_targets.is_empty();
        let has_beats = !snapshot.beats.is_empty();
        // The bend and whammy tiles open their editors.
        let mut open = None;
        let mut row = |ui: &mut egui::Ui, items: &[FxItem], commands: &mut Vec<Command>| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
                for item in items {
                    let enabled = if item.fx.on_note() {
                        has_notes
                    } else {
                        has_beats
                    };
                    let label = item.action.label();
                    let tip = format!("{label}{}", hint(item.action));
                    let on = is_on_for_targets(item.fx, snapshot);
                    if tiles::tile(ui, item.face, on, enabled, label, &tip).clicked() {
                        if matches!(item.fx, Fx::Bend | Fx::Whammy) {
                            open = Some(item.fx);
                        } else {
                            commands.extend(fx_commands(item.fx, snapshot));
                        }
                    }
                }
            });
        };
        style::section(ui, "note_effects", "Techniques", |ui| {
            if !has_notes {
                ui.label(egui::RichText::new("Put the cursor on a note").weak());
            }
            row(ui, NOTE_ITEMS, commands);
            row(ui, SLIDE_ITEMS, commands);
            row(ui, PITCH_ITEMS, commands);
            if let Some(state) = note
                && let Some(bend) = &state.bend
            {
                ui.label(
                    egui::RichText::new(format!(
                        "Bend {} · {}",
                        bend_name(bend.kind),
                        quarter_tones(bend)
                    ))
                    .weak(),
                );
            }
            if let Some(shape) = &beat.whammy {
                ui.label(
                    egui::RichText::new(format!("Whammy · {}", quarter_tones_shape(&shape.points)))
                        .weak(),
                );
            }
        });
        style::section(ui, "beat_effects", "Strokes and beat", |ui| {
            row(ui, BEAT_ITEMS, commands);
        });
        match open {
            Some(Fx::Bend) => self.open_bend(snapshot),
            Some(Fx::Whammy) => self.open_whammy(snapshot),
            _ => {}
        }
    }

    fn show_bend(
        &mut self,
        ctx: &egui::Context,
        snapshot: &PaletteSnapshot,
        presets: &mut Vec<ShapePreset>,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.bend else {
            return;
        };
        let effect = match editor.show(ctx, presets) {
            Outcome::Open => return,
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
    ) {
        let Some(editor) = &mut self.whammy else {
            return;
        };
        let effect = match editor.show(ctx, presets) {
            Outcome::Open => return,
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

/// Adds one effect to every note.
fn apply_note(
    commands: &mut Vec<Command>,
    targets: &[(gscore_model::BeatPosition, gscore_model::StringNumber)],
    effect: NoteEffect,
) {
    for (at, string) in targets {
        commands.push(Command::SetNoteEffect {
            at: *at,
            string: *string,
            effect: effect.clone(),
        });
    }
}

/// Adds one effect to every beat.
fn apply_beat(
    commands: &mut Vec<Command>,
    beats: &[gscore_model::BeatPosition],
    effect: BeatEffect,
) {
    for at in beats {
        commands.push(Command::SetBeatEffect {
            at: *at,
            effect: effect.clone(),
        });
    }
}

/// A full-step bend, like the editor's "Bend" preset: up to 4 quarter tones by the middle
/// of the note, then held.
fn default_bend() -> Vec<BendPoint> {
    vec![
        BendPoint {
            position: 0,
            value: 0,
        },
        BendPoint {
            position: 6,
            value: 4,
        },
        BendPoint {
            position: 12,
            value: 4,
        },
    ]
}

/// A dip of two steps, like the editor's "Dip" preset.
fn default_whammy() -> Vec<BendPoint> {
    vec![
        BendPoint {
            position: 0,
            value: 0,
        },
        BendPoint {
            position: 6,
            value: -8,
        },
        BendPoint {
            position: 12,
            value: 0,
        },
    ]
}

fn bend_name(kind: BendKind) -> &'static str {
    match kind {
        BendKind::Up => "up",
        BendKind::Release => "release",
    }
}

fn quarter_tones(bend: &BendShape) -> String {
    let highest = bend
        .points
        .iter()
        .map(|point| point.value)
        .max_by_key(|value| value.unsigned_abs())
        .unwrap_or(0);
    format!("{} quarter tones", highest)
}

fn quarter_tones_shape(points: &[BendPoint]) -> String {
    let highest = points
        .iter()
        .map(|point| point.value)
        .max_by_key(|value| value.unsigned_abs())
        .unwrap_or(0);
    format!("{highest} quarter tones")
}
