//! The effects half of the edit palette: the note and beat effect matrix, and the bend and
//! whammy shape editors. It shows the state of the note under the cursor and applies a
//! click to every note or beat in the selection as commands.

mod dialogs;
mod editors;

use egui::vec2;
use gscore_edit::{ActionId, Command};
use gscore_model::{
    AccentKind, BeatEffect, BeatView, BendKind, BendPoint, BendShape, Fade, Finger, Grace, Hand,
    Harmonic, HarmonicKind, Legato, NoteEffect, NoteView, SlapKind, SlideKind, StrokeDirection,
    TremoloSpeed, Trill, TrillSpeed, Vibrato, WhammyShape,
};

use super::beat_text_editor::BeatTextEditor;
use super::edit_effects::dialogs::{TremoloEditor, TrillEditor};
use super::edit_palette::PaletteSnapshot;
use super::grace_editor::GraceEditor;
use super::harmonic_editor::HarmonicEditor;
use super::shape_editor::ShapeEditor;
use super::style;
use super::tiles::{self, Face};

/// The effects of the note under the cursor.
#[derive(Debug, Clone)]
pub(super) struct NoteState {
    dead: bool,
    hammer: bool,
    ghost: bool,
    palm_mute: bool,
    let_ring: bool,
    staccato: bool,
    trill: Option<Trill>,
    tremolo_picking: Option<TremoloSpeed>,
    left_hand_tap: bool,
    string_number: bool,
    finger_left: Option<Finger>,
    finger_right: Option<Finger>,
    accent: Option<AccentKind>,
    vibrato: Option<Vibrato>,
    harmonic: Option<Harmonic>,
    slides: Vec<SlideKind>,
    bend: Option<BendShape>,
    grace: Option<Grace>,
    /// The note's fret (0 when it has none), for the harmonic and grace note editors.
    fret: u8,
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
            trill: note.trill(),
            tremolo_picking: note.tremolo_speed(),
            left_hand_tap: note.is_left_hand_tapped(),
            string_number: note.shows_string_number(),
            finger_left: note.finger(Hand::Left),
            finger_right: note.finger(Hand::Right),
            accent: note.accent(),
            vibrato: note.vibrato(),
            harmonic: note.harmonic(),
            slides: note.slides().collect(),
            bend: note.bend_shape(),
            grace: note.grace(),
            fret: note.fret().map_or(0, |fret| fret.get()),
        }
    }
}

/// The effects of the beat under the cursor.
#[derive(Debug, Clone)]
pub(super) struct BeatState {
    pick: Option<StrokeDirection>,
    brush: Option<StrokeDirection>,
    arpeggio: Option<StrokeDirection>,
    slap: Option<SlapKind>,
    fade: Option<Fade>,
    dead_slapped: bool,
    rasgueado: bool,
    whammy: Option<WhammyShape>,
    legato: Option<Legato>,
    vibrato: Option<Vibrato>,
}

impl BeatState {
    pub(super) fn of(beat: &BeatView<'_>) -> Self {
        BeatState {
            pick: beat.pick_stroke(),
            brush: beat.brush_stroke(),
            arpeggio: beat.arpeggio(),
            slap: beat.slap(),
            fade: beat.fade(),
            dead_slapped: beat.is_dead_slapped(),
            rasgueado: beat.has_rasgueado(),
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
    harmonic: Option<HarmonicEditor>,
    grace: Option<GraceEditor>,
    trill: Option<TrillEditor>,
    tremolo: Option<TremoloEditor>,
    beat_text: Option<BeatTextEditor>,
    finger_left: Option<dialogs::FingerEditor>,
    finger_right: Option<dialogs::FingerEditor>,
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
    /// Any harmonic: the tile that opens the harmonic editor.
    AnyHarmonic,
    /// A grace note: the tile opens the grace note dialog.
    Grace,
    Pick(StrokeDirection),
    Brush(StrokeDirection),
    Arpeggio(StrokeDirection),
    Slap(SlapKind),
    FadeIn,
    FadeOut,
    VolumeSwell,
    DeadSlapped,
    Rasgueado,
    LeftHandTap,
    StringNumber,
    FingerLeft,
    FingerRight,
    Whammy,
    Legato,
    BeatVibrato(Vibrato),
}

impl Fx {
    /// Whether the effect belongs to a note (else to a beat).
    fn on_note(self) -> bool {
        matches!(
            self,
            Fx::Dead
                | Fx::Ghost
                | Fx::Hammer
                | Fx::PalmMute
                | Fx::LetRing
                | Fx::Staccato
                | Fx::Trill
                | Fx::Tremolo
                | Fx::Slide(_)
                | Fx::Bend
                | Fx::Vibrato(_)
                | Fx::Accent(_)
                | Fx::Harmonic(_)
                | Fx::AnyHarmonic
                | Fx::Grace
                | Fx::LeftHandTap
                | Fx::StringNumber
                | Fx::FingerLeft
                | Fx::FingerRight
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
        Face::GlyphMore {
            text: "\u{E566}",
            size: 30.0,
            baseline: 5.0,
        },
        Fx::Trill,
    ),
    item(
        ActionId::ToggleTremoloPicking,
        Face::GlyphMore {
            text: "\u{E222}",
            size: 28.0,
            baseline: 4.0,
        },
        Fx::Tremolo,
    ),
    item(
        ActionId::ToggleLeftHandTap,
        Face::CircledText("T"),
        Fx::LeftHandTap,
    ),
    item(
        ActionId::ToggleStringNumber,
        Face::CircledText("6"),
        Fx::StringNumber,
    ),
    item(
        ActionId::EditLeftFinger,
        Face::TextMore("L.H."),
        Fx::FingerLeft,
    ),
    item(
        ActionId::EditRightFinger,
        Face::TextMore("R.H."),
        Fx::FingerRight,
    ),
];

/// Slides.
const SLIDE_ITEMS: &[FxItem] = &[
    item(
        ActionId::ToggleShiftSlide,
        Face::SlideTo { legato: false },
        Fx::Slide(SlideKind::ShiftSlideTo),
    ),
    item(
        ActionId::ToggleLegatoSlide,
        Face::SlideTo { legato: true },
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
    item(
        ActionId::TogglePickSlideDown,
        Face::PickSlide { down: true },
        Fx::Slide(SlideKind::PickSlideDown),
    ),
    item(
        ActionId::TogglePickSlideUp,
        Face::PickSlide { down: false },
        Fx::Slide(SlideKind::PickSlideUp),
    ),
];

/// The bend, the whammy bar (a beat effect, but its neighbour) and the harmonics.
const PITCH_ITEMS: &[FxItem] = &[
    item(ActionId::ToggleBend, Face::BendMore, Fx::Bend),
    item(ActionId::ToggleWhammy, Face::WhammyMore, Fx::Whammy),
    // One tile for every harmonic: it opens the harmonic editor.
    item(
        ActionId::ToggleNaturalHarmonic,
        Face::GlyphMore {
            text: "\u{E0DD}",
            size: 42.0,
            baseline: 0.0,
        },
        Fx::AnyHarmonic,
    ),
    // A slashed grace note: it opens the grace note dialog.
    item(
        ActionId::GraceNote,
        Face::GlyphMore {
            text: "\u{E560}",
            size: 40.0,
            baseline: 6.0,
        },
        Fx::Grace,
    ),
];

/// The harmonics, each with its key (their palette tile is the one harmonic tile).
const HARMONIC_ITEMS: &[FxItem] = &[
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
        glyph("\u{E634}", 15.0, 10.0),
        Fx::Brush(StrokeDirection::Down),
    ),
    item(
        ActionId::ToggleBrushUp,
        glyph("\u{E635}", 15.0, 10.0),
        Fx::Brush(StrokeDirection::Up),
    ),
    item(
        ActionId::ToggleArpeggioDown,
        Face::Arpeggio { up: true },
        Fx::Arpeggio(StrokeDirection::Down),
    ),
    item(
        ActionId::ToggleArpeggioUp,
        Face::Arpeggio { up: false },
        Fx::Arpeggio(StrokeDirection::Up),
    ),
    item(ActionId::ToggleDeadSlapped, Face::DeadSlap, Fx::DeadSlapped),
    item(
        ActionId::ToggleRasgueado,
        Face::Word("rasg."),
        Fx::Rasgueado,
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
    item(ActionId::ToggleLegato, Face::Slur, Fx::Legato),
    item(
        ActionId::ToggleBeatVibrato,
        glyph("\u{EAB2}\u{EAB2}\u{EAB2}", 30.0, 3.0),
        Fx::BeatVibrato(Vibrato::Slight),
    ),
];

/// The fades: dynamics, so they sit in the dynamics section of the palette.
const FADE_ITEMS: &[FxItem] = &[
    item(ActionId::ToggleFadeIn, Face::Hairpin, Fx::FadeIn),
    item(ActionId::ToggleFadeOut, Face::HairpinOut, Fx::FadeOut),
    item(
        ActionId::ToggleVolumeSwell,
        Face::HairpinSwell,
        Fx::VolumeSwell,
    ),
];

/// The fade tiles, in the dynamics section: a fade-in, a fade-out and a volume swell.
pub(super) fn fade_tiles(
    ui: &mut egui::Ui,
    snapshot: &PaletteSnapshot,
    hint: &dyn Fn(ActionId) -> String,
    commands: &mut Vec<Command>,
) {
    let enabled = !snapshot.beats.is_empty();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
        for item in FADE_ITEMS {
            let label = item.action.label();
            let mut tip = format!("{label}{}", hint(item.action));
            if !enabled {
                tip.push_str("\nPut the cursor on a beat first.");
            }
            let on = is_on_for_targets(item.fx, snapshot);
            if tiles::tile(ui, item.face, on, enabled, label, &tip).clicked() {
                commands.extend(fx_commands(item.fx, snapshot));
            }
        }
    });
}

/// The effect item bound to `action`, for a key press.
pub(super) fn fx_item(action: ActionId) -> Option<&'static FxItem> {
    // The harmonic keys first: the harmonic tile shares the natural harmonic's action.
    [
        HARMONIC_ITEMS,
        NOTE_ITEMS,
        SLIDE_ITEMS,
        PITCH_ITEMS,
        FADE_ITEMS,
        BEAT_ITEMS,
    ]
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
        arpeggio: None,
        slap: None,
        fade: None,
        dead_slapped: false,
        rasgueado: false,
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
        Fx::Trill => note_on(|n| n.trill.is_some()),
        Fx::Tremolo => note_on(|n| n.tremolo_picking.is_some()),
        Fx::LeftHandTap => note_on(|n| n.left_hand_tap),
        Fx::StringNumber => note_on(|n| n.string_number),
        Fx::FingerLeft => note_on(|n| n.finger_left.is_some()),
        Fx::FingerRight => note_on(|n| n.finger_right.is_some()),
        Fx::Bend => note_on(|n| n.bend.is_some()),
        Fx::Slide(kind) => note.is_some_and(|n| n.slides.contains(&kind)),
        Fx::Vibrato(kind) => note.is_some_and(|n| n.vibrato == Some(kind)),
        Fx::Accent(kind) => note.is_some_and(|n| n.accent == Some(kind)),
        Fx::Harmonic(kind) => note.is_some_and(|n| n.harmonic.is_some_and(|h| h.kind == kind)),
        Fx::AnyHarmonic => note.is_some_and(|n| n.harmonic.is_some()),
        Fx::Grace => note.is_some_and(|n| n.grace.is_some()),
        Fx::Pick(direction) => beat.pick == Some(direction),
        Fx::Brush(direction) => beat.brush == Some(direction),
        Fx::Arpeggio(direction) => beat.arpeggio == Some(direction),
        Fx::Slap(kind) => beat.slap == Some(kind),
        Fx::FadeIn => beat.fade == Some(Fade::In),
        Fx::FadeOut => beat.fade == Some(Fade::Out),
        Fx::VolumeSwell => beat.fade == Some(Fade::Swell),
        Fx::DeadSlapped => beat.dead_slapped,
        Fx::Rasgueado => beat.rasgueado,
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
        Fx::Trill => Some(NoteEffect::Trill((!on).then_some(Trill {
            fret: None,
            speed: TrillSpeed::Sixteenth,
        }))),
        Fx::Tremolo => Some(NoteEffect::TremoloPicking(
            (!on).then_some(TremoloSpeed::Sixteenth),
        )),
        Fx::LeftHandTap => Some(NoteEffect::LeftHandTap(!on)),
        Fx::StringNumber => Some(NoteEffect::StringNumber(!on)),
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
        Fx::FadeOut => apply_beat(&mut commands, beats, BeatEffect::FadeOut(!on)),
        Fx::VolumeSwell => apply_beat(&mut commands, beats, BeatEffect::VolumeSwell(!on)),
        Fx::Arpeggio(direction) => apply_beat(
            &mut commands,
            beats,
            BeatEffect::Arpeggio((!on).then_some(direction)),
        ),
        Fx::DeadSlapped => apply_beat(&mut commands, beats, BeatEffect::DeadSlapped(!on)),
        Fx::Rasgueado => apply_beat(&mut commands, beats, BeatEffect::Rasgueado(!on)),
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
                    let label = if item.fx == Fx::AnyHarmonic {
                        "Harmonic"
                    } else {
                        item.action.label()
                    };
                    let tip = if enabled {
                        format!("{label}{}", hint(item.action))
                    } else if item.fx.on_note() {
                        format!(
                            "{label}{}\nPut the cursor on a note first.",
                            hint(item.action)
                        )
                    } else {
                        format!(
                            "{label}{}\nPut the cursor on a beat first.",
                            hint(item.action)
                        )
                    };
                    let on = is_on_for_targets(item.fx, snapshot);
                    if tiles::tile(ui, item.face, on, enabled, label, &tip).clicked() {
                        // These tiles open a dialog; the others toggle the effect.
                        let opens = matches!(
                            item.fx,
                            Fx::Bend
                                | Fx::Whammy
                                | Fx::AnyHarmonic
                                | Fx::Grace
                                | Fx::Trill
                                | Fx::Tremolo
                                | Fx::FingerLeft
                                | Fx::FingerRight
                        );
                        if opens {
                            open = Some(item.fx);
                        } else {
                            commands.extend(fx_commands(item.fx, snapshot));
                        }
                    }
                }
            });
        };
        style::section(ui, "note_effects", "Techniques", |ui| {
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
            Some(Fx::AnyHarmonic) => self.open_harmonic(snapshot),
            Some(Fx::Grace) => self.open_grace(snapshot),
            Some(Fx::Trill) => self.open_trill(snapshot),
            Some(Fx::Tremolo) => self.open_tremolo(snapshot),
            Some(Fx::FingerLeft) => self.open_finger(snapshot, Hand::Left),
            Some(Fx::FingerRight) => self.open_finger(snapshot, Hand::Right),
            _ => {}
        }
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
