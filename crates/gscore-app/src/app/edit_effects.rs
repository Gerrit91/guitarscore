//! The effects half of the edit palette: the note and beat effect matrix, and the bend and
//! whammy shape editors. It shows the state of the note under the cursor and applies a
//! click to every note or beat in the selection as commands.

use egui::{Color32, Stroke, pos2, vec2};
use gscore_edit::Command;
use gscore_model::{
    AccentKind, BeatEffect, BeatView, BendKind, BendPoint, BendShape, Harmonic, HarmonicKind,
    Legato, NoteEffect, NoteView, SlapKind, SlideKind, StrokeDirection, Vibrato, WhammyShape,
};

use super::edit_palette::PaletteSnapshot;

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
    bend: Option<BendEditor>,
    whammy: Option<WhammyEditor>,
}

/// A bend being edited.
#[derive(Debug)]
struct BendEditor {
    kind: BendKind,
    points: Vec<BendPoint>,
    error: Option<String>,
}

/// A whammy shape being edited.
#[derive(Debug)]
struct WhammyEditor {
    points: Vec<BendPoint>,
    error: Option<String>,
}

impl EffectsState {
    /// Draws the matrix; clicks turn into commands.
    pub(super) fn panel(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        ui.strong("Note effects");
        let Some(state) = snapshot.note_state.clone() else {
            ui.label(egui::RichText::new("Put the cursor on a note").weak());
            ui.separator();
            beat_panel(ui, snapshot, &mut self.whammy, commands);
            self.show_bend(ui, snapshot, commands);
            self.show_whammy(ui, snapshot, commands);
            return;
        };
        if snapshot.note_targets.is_empty() {
            ui.label(egui::RichText::new("Select some notes first").weak());
        }
        let targets = &snapshot.note_targets;
        ui.horizontal_wrapped(|ui| {
            toggle(
                ui,
                "Hammer",
                state.hammer,
                targets,
                NoteEffect::Hammer,
                commands,
            );
            if ui.selectable_label(state.dead, "Dead").clicked() {
                for (at, string) in targets {
                    commands.push(Command::SetDeadNote {
                        at: *at,
                        string: *string,
                        dead: !state.dead,
                    });
                }
            }
            toggle(
                ui,
                "Ghost",
                state.ghost,
                targets,
                NoteEffect::Ghost,
                commands,
            );
            toggle(
                ui,
                "Palm mute",
                state.palm_mute,
                targets,
                NoteEffect::PalmMute,
                commands,
            );
            toggle(
                ui,
                "Let ring",
                state.let_ring,
                targets,
                NoteEffect::LetRing,
                commands,
            );
            toggle(
                ui,
                "Staccato",
                state.staccato,
                targets,
                NoteEffect::Staccato,
                commands,
            );
        });
        ui.horizontal_wrapped(|ui| {
            toggle(
                ui,
                "Trill",
                state.trill,
                targets,
                NoteEffect::Trill,
                commands,
            );
            toggle(
                ui,
                "Tremolo",
                state.tremolo_picking,
                targets,
                NoteEffect::TremoloPicking,
                commands,
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Accent");
            for (label, value) in [
                ("none", None),
                ("normal", Some(AccentKind::Normal)),
                ("heavy", Some(AccentKind::Heavy)),
            ] {
                if ui.selectable_label(state.accent == value, label).clicked() {
                    apply_note(commands, targets, NoteEffect::Accent(value));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Vibrato");
            for (label, value) in [
                ("none", None),
                ("slight", Some(Vibrato::Slight)),
                ("wide", Some(Vibrato::Wide)),
            ] {
                if ui.selectable_label(state.vibrato == value, label).clicked() {
                    apply_note(commands, targets, NoteEffect::Vibrato(value));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Slides");
            for (label, kind) in [
                ("In up", SlideKind::IntoFromAbove),
                ("In down", SlideKind::IntoFromBelow),
                ("Shift", SlideKind::ShiftSlideTo),
                ("Legato", SlideKind::LegatoSlideTo),
                ("Out down", SlideKind::OutDownwards),
                ("Out up", SlideKind::OutUpwards),
            ] {
                let active = state.slides.contains(&kind);
                if ui.selectable_label(active, label).clicked() {
                    apply_note(commands, targets, NoteEffect::Slide(kind, !active));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Harmonic");
            for (label, kind) in [
                ("none", None),
                ("natural", Some(HarmonicKind::Natural)),
                ("artificial", Some(HarmonicKind::Artificial)),
                ("tapped", Some(HarmonicKind::Tapped)),
                ("pinch", Some(HarmonicKind::Pinch)),
                ("semi", Some(HarmonicKind::Semi)),
            ] {
                let active = state.harmonic.as_ref().map(|h| h.kind) == kind;
                if ui.selectable_label(active, label).clicked() {
                    apply_note(
                        commands,
                        targets,
                        NoteEffect::Harmonic(kind.map(|kind| Harmonic {
                            kind,
                            node: state.harmonic.as_ref().and_then(|h| h.node),
                        })),
                    );
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            match &state.bend {
                Some(bend) => {
                    ui.label(format!(
                        "Bend {} · {}",
                        bend_name(bend.kind),
                        quarter_tones(bend)
                    ));
                }
                None => {
                    ui.label(egui::RichText::new("No bend").weak());
                }
            }
            if ui.button("Bend…").clicked() {
                self.bend = Some(BendEditor {
                    kind: state.bend.as_ref().map_or(BendKind::Up, |bend| bend.kind),
                    points: state
                        .bend
                        .as_ref()
                        .map_or_else(default_bend, |bend| bend.points.clone()),
                    error: None,
                });
            }
            if state.bend.is_some() && ui.button("Remove bend").clicked() {
                apply_note(commands, targets, NoteEffect::Bend(None));
            }
        });
        ui.separator();
        beat_panel(ui, snapshot, &mut self.whammy, commands);
        self.show_bend(ui, snapshot, commands);
        self.show_whammy(ui, snapshot, commands);
    }

    fn show_bend(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.bend else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Bend shape")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Kind");
                    ui.selectable_value(&mut editor.kind, BendKind::Up, "up");
                    ui.selectable_value(&mut editor.kind, BendKind::Release, "release");
                });
                points_editor(ui, &mut editor.points, &mut editor.error);
                preview(ui, &editor.points);
                if let Some(error) = &editor.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            match BendShape::new(editor.kind, editor.points.clone()) {
                Ok(shape) => {
                    for (at, string) in &snapshot.note_targets {
                        commands.push(Command::SetNoteEffect {
                            at: *at,
                            string: *string,
                            effect: NoteEffect::Bend(Some(shape.clone())),
                        });
                    }
                    self.bend = None;
                }
                Err(error) => editor.error = Some(error.to_string()),
            }
        } else if cancel || !open {
            self.bend = None;
        }
    }

    fn show_whammy(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &PaletteSnapshot,
        commands: &mut Vec<Command>,
    ) {
        let Some(editor) = &mut self.whammy else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Whammy shape")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                points_editor(ui, &mut editor.points, &mut editor.error);
                preview(ui, &editor.points);
                if let Some(error) = &editor.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            match WhammyShape::new(editor.points.clone()) {
                Ok(shape) => {
                    for at in &snapshot.beats {
                        commands.push(Command::SetBeatEffect {
                            at: *at,
                            effect: BeatEffect::Whammy(Some(shape.clone())),
                        });
                    }
                    self.whammy = None;
                }
                Err(error) => editor.error = Some(error.to_string()),
            }
        } else if cancel || !open {
            self.whammy = None;
        }
    }
}

/// The beat effects.
fn beat_panel(
    ui: &mut egui::Ui,
    snapshot: &PaletteSnapshot,
    whammy: &mut Option<WhammyEditor>,
    commands: &mut Vec<Command>,
) {
    ui.strong("Beat effects");
    let state = snapshot.beat_state.clone();
    let beats = &snapshot.beats;
    let current = state.unwrap_or(BeatState {
        pick: None,
        brush: None,
        slap: None,
        fade_in: false,
        whammy: None,
        legato: None,
        vibrato: None,
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Pick");
        for (label, value) in [
            ("off", None),
            ("down", Some(StrokeDirection::Down)),
            ("up", Some(StrokeDirection::Up)),
        ] {
            if ui.selectable_label(current.pick == value, label).clicked() {
                apply_beat(commands, beats, BeatEffect::PickStroke(value));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Brush");
        for (label, value) in [
            ("off", None),
            ("down", Some(StrokeDirection::Down)),
            ("up", Some(StrokeDirection::Up)),
        ] {
            if ui.selectable_label(current.brush == value, label).clicked() {
                apply_beat(commands, beats, BeatEffect::BrushStroke(value));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Slap");
        for (label, value) in [
            ("off", None),
            ("tap", Some(SlapKind::Tapping)),
            ("slap", Some(SlapKind::Slapping)),
            ("pop", Some(SlapKind::Popping)),
        ] {
            if ui.selectable_label(current.slap == value, label).clicked() {
                apply_beat(commands, beats, BeatEffect::Slap(value));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        toggle_beat(
            ui,
            "Fade in",
            current.fade_in,
            beats,
            BeatEffect::FadeIn,
            commands,
        );
        ui.label("Legato");
        for (label, value) in [
            ("off", None),
            (
                "origin",
                Some(Legato {
                    origin: true,
                    destination: false,
                }),
            ),
            (
                "dest",
                Some(Legato {
                    origin: false,
                    destination: true,
                }),
            ),
            (
                "both",
                Some(Legato {
                    origin: true,
                    destination: true,
                }),
            ),
        ] {
            if ui
                .selectable_label(current.legato == value, label)
                .clicked()
            {
                apply_beat(commands, beats, BeatEffect::Legato(value));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Vibrato");
        for (label, value) in [
            ("off", None),
            ("slight", Some(Vibrato::Slight)),
            ("wide", Some(Vibrato::Wide)),
        ] {
            if ui
                .selectable_label(current.vibrato == value, label)
                .clicked()
            {
                apply_beat(commands, beats, BeatEffect::Vibrato(value));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if let Some(shape) = &current.whammy {
            ui.label(format!("Whammy · {}", quarter_tones_shape(&shape.points)));
        } else {
            ui.label(egui::RichText::new("No whammy").weak());
        }
        if ui.button("Whammy…").clicked() {
            *whammy = Some(WhammyEditor {
                points: current
                    .whammy
                    .as_ref()
                    .map_or_else(default_whammy, |shape| shape.points.clone()),
                error: None,
            });
        }
        if current.whammy.is_some() && ui.button("Remove whammy").clicked() {
            apply_beat(commands, beats, BeatEffect::Whammy(None));
        }
    });
}

/// A note-effect toggle: clicking sets the opposite of the shown state on every target.
fn toggle(
    ui: &mut egui::Ui,
    label: &str,
    on: bool,
    targets: &[(gscore_model::BeatPosition, gscore_model::StringNumber)],
    variant: fn(bool) -> NoteEffect,
    commands: &mut Vec<Command>,
) {
    if ui.selectable_label(on, label).clicked() {
        apply_note(commands, targets, variant(!on));
    }
}

/// A beat-effect toggle.
fn toggle_beat(
    ui: &mut egui::Ui,
    label: &str,
    on: bool,
    beats: &[gscore_model::BeatPosition],
    variant: fn(bool) -> BeatEffect,
    commands: &mut Vec<Command>,
) {
    if ui.selectable_label(on, label).clicked() {
        apply_beat(commands, beats, variant(!on));
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

/// Edits a list of shape points in place (position 0-12, value in quarter tones).
fn points_editor(ui: &mut egui::Ui, points: &mut Vec<BendPoint>, error: &mut Option<String>) {
    let mut remove = None;
    for (index, point) in points.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label("at");
            ui.add(egui::DragValue::new(&mut point.position).range(0..=12));
            ui.label("value");
            ui.add(egui::DragValue::new(&mut point.value).range(-48..=48));
            if ui.small_button("x").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove
        && points.len() > 1
    {
        points.remove(index);
        *error = None;
    }
    if ui.button("Add point").clicked() {
        let position = points
            .last()
            .map_or(6, |point| point.position.saturating_add(1).min(12));
        points.push(BendPoint { position, value: 0 });
        *error = None;
    }
}

/// A small line preview of a shape.
fn preview(ui: &mut egui::Ui, points: &[BendPoint]) {
    let (rect, _) = ui.allocate_exact_size(vec2(220.0, 60.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, ui.visuals().faint_bg_color);
    let to_screen = |point: &BendPoint| {
        pos2(
            rect.left() + rect.width() * f32::from(point.position) / 12.0,
            rect.center().y - f32::from(point.value) * rect.height() / 96.0,
        )
    };
    painter.line_segment(
        [
            pos2(rect.left(), rect.center().y),
            pos2(rect.right(), rect.center().y),
        ],
        Stroke::new(0.5, ui.visuals().weak_text_color()),
    );
    let screen: Vec<_> = points.iter().map(to_screen).collect();
    if let Some(first) = screen.first() {
        let mut last = *first;
        for point in screen.iter().skip(1) {
            painter.line_segment(
                [last, *point],
                Stroke::new(1.5, ui.visuals().selection.bg_fill),
            );
            last = *point;
        }
        for point in screen {
            painter.circle_filled(point, 2.5, Color32::from_rgb(0x1e, 0x6f, 0xd9));
        }
    }
}

fn default_bend() -> Vec<BendPoint> {
    vec![
        BendPoint {
            position: 0,
            value: 0,
        },
        BendPoint {
            position: 12,
            value: 4,
        },
    ]
}

fn default_whammy() -> Vec<BendPoint> {
    vec![
        BendPoint {
            position: 0,
            value: 0,
        },
        BendPoint {
            position: 6,
            value: -4,
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
