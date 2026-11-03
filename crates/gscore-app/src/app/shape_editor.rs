//! The bend and whammy editor: a graph of the shape whose points are dragged with the
//! mouse, a list of presets (built in, and saved by the user), and OK / Remove / Cancel.

use egui::{
    Align, Align2, Color32, FontId, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, pos2, vec2,
};
use gscore_model::{BEND_MAX_POSITION, BEND_MAX_VALUE, BendKind, BendPoint, MAX_BEND_POINTS};

use super::style;
use crate::settings::{MAX_SHAPE_PRESETS, ShapePreset};

/// The size of the bend graph and of the (taller, up and down) whammy graph, with their
/// value labels.
const BEND_GRAPH: egui::Vec2 = vec2(460.0, 270.0);
const WHAMMY_GRAPH: egui::Vec2 = vec2(460.0, 380.0);
/// Room left of the plot for the value labels, and around it.
const LABEL_GUTTER: f32 = 40.0;
const PLOT_MARGIN: f32 = 14.0;
/// The width of the preset column.
const PRESETS_WIDTH: f32 = 200.0;
/// The size of a point's square handle, and of the area that catches the mouse.
const HANDLE: f32 = 10.0;
const HANDLE_HIT: f32 = 18.0;
/// The ranges offered for the graph, in whole steps.
const RANGES: [i8; 7] = [1, 2, 3, 4, 6, 8, 12];

/// Which shape is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShapeTarget {
    /// A note's bend: the graph runs from 0 up.
    Bend,
    /// A beat's whammy bar: the graph runs down and up from 0.
    Whammy,
}

/// What happened in the editor this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Still open.
    Open,
    /// OK: apply the shape.
    Apply,
    /// Remove the bend (or whammy) altogether.
    Remove,
    /// Closed without a change.
    Cancel,
}

/// A preset that comes with the app.
struct Builtin {
    name: &'static str,
    kind: BendKind,
    points: &'static [(u8, i8)],
}

const BEND_PRESETS: &[Builtin] = &[
    Builtin {
        name: "Bend",
        kind: BendKind::Up,
        points: &[(0, 0), (6, 4), (12, 4)],
    },
    Builtin {
        name: "Bend/Release",
        kind: BendKind::Up,
        points: &[(0, 0), (3, 4), (6, 4), (9, 0), (12, 0)],
    },
    Builtin {
        name: "Bend/Release/Bend",
        kind: BendKind::Up,
        points: &[(0, 0), (2, 4), (4, 4), (6, 0), (8, 0), (10, 4), (12, 4)],
    },
    Builtin {
        name: "Pre-Bend",
        kind: BendKind::Up,
        points: &[(0, 4), (12, 4)],
    },
    Builtin {
        name: "Pre-Bend/Release",
        kind: BendKind::Up,
        points: &[(0, 4), (4, 4), (8, 0), (12, 0)],
    },
    Builtin {
        name: "Release",
        kind: BendKind::Release,
        points: &[(0, 4), (6, 0), (12, 0)],
    },
];

/// The whammy bar presets, as Guitar Pro names them; two steps deep.
const WHAMMY_PRESETS: &[Builtin] = &[
    Builtin {
        name: "Dip",
        kind: BendKind::Up,
        points: &[(0, 0), (6, -8), (12, 0)],
    },
    Builtin {
        name: "Dive",
        kind: BendKind::Up,
        points: &[(0, 0), (12, -8)],
    },
    Builtin {
        name: "Release up",
        kind: BendKind::Up,
        points: &[(0, -8), (12, 0)],
    },
    Builtin {
        name: "Inverted Dip",
        kind: BendKind::Up,
        points: &[(0, 0), (6, 8), (12, 0)],
    },
    Builtin {
        name: "Return",
        kind: BendKind::Up,
        points: &[(0, 0), (4, -8), (8, -8), (12, 0)],
    },
    Builtin {
        name: "Release down",
        kind: BendKind::Up,
        points: &[(0, 8), (12, 0)],
    },
];

/// A bend or whammy shape being edited.
#[derive(Debug)]
pub(super) struct ShapeEditor {
    target: ShapeTarget,
    /// The bend's kind (a whammy ignores it).
    pub(super) kind: BendKind,
    /// The points, positions strictly increasing.
    pub(super) points: Vec<BendPoint>,
    /// The graph's top (and, for a whammy, bottom) in quarter tones.
    range: i8,
    /// The name typed for "Save as preset…", while it is typed.
    naming: Option<String>,
    /// Why OK did not work.
    pub(super) error: Option<String>,
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<Pos2>,
}

impl ShapeEditor {
    /// An editor for `points` (a copy; nothing changes until OK).
    pub(super) fn new(target: ShapeTarget, kind: BendKind, points: Vec<BendPoint>) -> Self {
        let mut editor = ShapeEditor {
            target,
            kind,
            points,
            range: match target {
                ShapeTarget::Bend => 12,
                ShapeTarget::Whammy => 24,
            },
            naming: None,
            error: None,
            pos: None,
        };
        editor.fit_range();
        editor
    }

    fn title(&self) -> &'static str {
        match self.target {
            ShapeTarget::Bend => "Bend editor",
            ShapeTarget::Whammy => "Whammy bar editor",
        }
    }

    fn graph_size(&self) -> egui::Vec2 {
        match self.target {
            ShapeTarget::Bend => BEND_GRAPH,
            ShapeTarget::Whammy => WHAMMY_GRAPH,
        }
    }

    /// How values are labelled: bends say "full", the whammy bar counts whole steps.
    fn label(&self) -> fn(i8) -> String {
        match self.target {
            ShapeTarget::Bend => value_label,
            ShapeTarget::Whammy => step_label,
        }
    }

    fn builtins(&self) -> &'static [Builtin] {
        match self.target {
            ShapeTarget::Bend => BEND_PRESETS,
            ShapeTarget::Whammy => WHAMMY_PRESETS,
        }
    }

    /// The lowest and highest value on the graph, in quarter tones.
    fn span(&self) -> (i8, i8) {
        let below = self.target == ShapeTarget::Whammy || self.points.iter().any(|p| p.value < 0);
        (if below { -self.range } else { 0 }, self.range)
    }

    /// Widens the graph so every point fits, in whole steps.
    fn fit_range(&mut self) {
        let needed = needed_range(&self.points);
        if needed > self.range {
            self.range = RANGES
                .iter()
                .map(|steps| steps * 4)
                .find(|range| *range >= needed)
                .unwrap_or(BEND_MAX_VALUE);
        }
    }

    /// Takes a preset's kind and points.
    fn pick(&mut self, kind: BendKind, points: Vec<BendPoint>) {
        self.kind = kind;
        self.points = points;
        self.error = None;
        self.fit_range();
    }

    /// Whether the shape is the given preset.
    fn is(&self, kind: BendKind, points: &[BendPoint]) -> bool {
        self.points == points && (self.target == ShapeTarget::Whammy || self.kind == kind)
    }

    /// Shows the window; `presets` are the user's own presets for this kind of shape.
    pub(super) fn show(&mut self, ctx: &egui::Context, presets: &mut Vec<ShapePreset>) -> Outcome {
        let title = self.title();
        let graph_size = self.graph_size();
        let size = vec2(graph_size.x + PRESETS_WIDTH + 64.0, graph_size.y + 120.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                let span = self.span();
                                let label = self.label();
                                if graph(ui, &mut self.points, span, graph_size, label) {
                                    self.error = None;
                                }
                                ui.label(
                                    RichText::new(
                                        "Drag a point to move it · click to add one · \
                                         right-click to remove it",
                                    )
                                    .small()
                                    .weak(),
                                );
                            });
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.set_width(PRESETS_WIDTH);
                                self.presets_column(ui, presets);
                            });
                        });
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        let what = match self.target {
                            ShapeTarget::Bend => "bend",
                            ShapeTarget::Whammy => "whammy",
                        };
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 30.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                let button =
                                    |text: &str| egui::Button::new(text).min_size(vec2(88.0, 28.0));
                                if ui.add(button("Cancel")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                if ui
                                    .add(button("Remove"))
                                    .on_hover_text(format!("Take the {what} off"))
                                    .clicked()
                                {
                                    outcome = Outcome::Remove;
                                }
                                let tokens = style::tokens(ui);
                                let ok =
                                    egui::Button::new(RichText::new("OK").color(tokens.on_accent))
                                        .fill(tokens.accent)
                                        .min_size(vec2(88.0, 28.0));
                                if ui.add(ok).clicked() {
                                    outcome = Outcome::Apply;
                                }
                            },
                        );
                        if outcome == Outcome::Open && self.naming.is_none() {
                            ui.input(|input| {
                                if input.key_pressed(egui::Key::Escape) {
                                    outcome = Outcome::Cancel;
                                } else if input.key_pressed(egui::Key::Enter) {
                                    outcome = Outcome::Apply;
                                }
                            });
                        }
                    });
            });
        outcome
    }

    /// The preset list, saving and deleting presets, and the graph's range.
    fn presets_column(&mut self, ui: &mut egui::Ui, presets: &mut Vec<ShapePreset>) {
        let tokens = style::tokens(ui);
        ui.label(RichText::new("Presets").weak());
        let mut picked = None;
        let mut chosen_user = None;
        egui::Frame::new()
            .fill(tokens.chrome)
            .stroke(Stroke::new(1.0, tokens.line))
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("shape_presets")
                    .max_height(196.0)
                    .min_scrolled_height(196.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for builtin in self.builtins() {
                            let points = to_points(builtin.points);
                            let on = self.is(builtin.kind, &points);
                            if preset_row(ui, builtin.name, on).clicked() {
                                picked = Some((builtin.kind, points));
                            }
                        }
                        for (index, preset) in presets.iter().enumerate() {
                            let kind = if preset.release {
                                BendKind::Release
                            } else {
                                BendKind::Up
                            };
                            let points = preset.bend_points();
                            let on = self.is(kind, &points);
                            if on {
                                chosen_user = Some(index);
                            }
                            if preset_row(ui, &preset.name, on).clicked() {
                                picked = Some((kind, points));
                            }
                        }
                    });
            });
        if let Some((kind, points)) = picked {
            self.pick(kind, points);
        }

        ui.add_space(6.0);
        let full = vec2(PRESETS_WIDTH, 28.0);
        if let Some(name) = &mut self.naming {
            let field = ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("Preset name")
                    .desired_width(PRESETS_WIDTH),
            );
            ui.ctx()
                .accesskit_node_builder(field.id, |node| node.set_label("Preset name"));
            field.request_focus();
            let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
            let mut save = enter;
            let mut stop = escape;
            ui.horizontal(|ui| {
                let half = vec2((PRESETS_WIDTH - ui.spacing().item_spacing.x) / 2.0, 28.0);
                save |= ui
                    .add_enabled(
                        !name.trim().is_empty(),
                        egui::Button::new("Save").min_size(half),
                    )
                    .clicked();
                stop |= ui
                    .add(egui::Button::new("Cancel saving").min_size(half))
                    .clicked();
            });
            if save && !name.trim().is_empty() {
                let preset = ShapePreset {
                    name: name.trim().to_owned(),
                    release: self.target == ShapeTarget::Bend && self.kind == BendKind::Release,
                    points: self
                        .points
                        .iter()
                        .map(|point| (point.position, point.value))
                        .collect(),
                };
                save_preset(presets, preset);
                stop = true;
            }
            if stop {
                self.naming = None;
            }
        } else if ui
            .add_enabled(
                presets.len() < MAX_SHAPE_PRESETS,
                egui::Button::new("Save as preset…").min_size(full),
            )
            .clicked()
        {
            self.naming = Some(String::new());
        }
        if ui
            .add_enabled(
                chosen_user.is_some(),
                egui::Button::new("Delete preset").min_size(full),
            )
            .on_disabled_hover_text("Only your own presets can be deleted")
            .clicked()
            && let Some(index) = chosen_user
        {
            presets.remove(index);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Range").weak());
            let needed = needed_range(&self.points);
            let combo = egui::ComboBox::from_id_salt("shape_range")
                .selected_text(steps_name(self.range))
                .show_ui(ui, |ui| {
                    for steps in RANGES {
                        let range = steps * 4;
                        ui.add_enabled_ui(range >= needed, |ui| {
                            ui.selectable_value(&mut self.range, range, steps_name(range));
                        });
                    }
                });
            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                node.set_label("Graph range");
            });
        });
    }
}

/// One row of the preset list.
fn preset_row(ui: &mut egui::Ui, name: &str, on: bool) -> egui::Response {
    let response =
        ui.add(egui::Button::selectable(on, name).min_size(vec2(ui.available_width(), 26.0)));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("Preset {name}"));
    });
    response
}

/// Adds `preset`, replacing one of the same name.
fn save_preset(presets: &mut Vec<ShapePreset>, preset: ShapePreset) {
    if let Some(same) = presets.iter_mut().find(|p| p.name == preset.name) {
        *same = preset;
    } else if presets.len() < MAX_SHAPE_PRESETS {
        presets.push(preset);
    }
}

fn to_points(points: &[(u8, i8)]) -> Vec<BendPoint> {
    points
        .iter()
        .map(|&(position, value)| BendPoint { position, value })
        .collect()
}

/// The farthest point from 0, in quarter tones.
fn needed_range(points: &[BendPoint]) -> i8 {
    points
        .iter()
        .map(|point| point.value.unsigned_abs())
        .max()
        .map_or(0, |value| i8::try_from(value).unwrap_or(BEND_MAX_VALUE))
}

/// A range for the range menu: "3 steps".
fn steps_name(range: i8) -> String {
    match range / 4 {
        1 => "1 step".to_owned(),
        steps => format!("{steps} steps"),
    }
}

/// A value label as bends are written: ½, full, 1½, 2 (steps); ¼ for a quarter tone.
pub(super) fn value_label(quarter_tones: i8) -> String {
    steps_label(quarter_tones, "full")
}

/// A value label in whole steps, as the whammy bar is written: +1, −2, −1½.
pub(super) fn step_label(quarter_tones: i8) -> String {
    steps_label(quarter_tones, "1")
}

/// A signed value in steps with fractions; `one` is how one whole step is written.
fn steps_label(quarter_tones: i8, one: &str) -> String {
    if quarter_tones == 0 {
        return "0".to_owned();
    }
    let sign = if quarter_tones < 0 { "−" } else { "+" };
    let quarters = quarter_tones.unsigned_abs();
    let whole = quarters / 4;
    let fraction = match quarters % 4 {
        1 => "¼",
        2 => "½",
        3 => "¾",
        _ => "",
    };
    let body = match (whole, fraction) {
        (1, "") => one.to_owned(),
        (0, fraction) => fraction.to_owned(),
        (whole, fraction) => format!("{whole}{fraction}"),
    };
    format!("{sign}{body}")
}

/// Where a point would go for the screen position `at`: the nearest position and quarter
/// tone inside the plot.
fn snap(plot: Rect, (bottom, top): (i8, i8), at: Pos2) -> BendPoint {
    let max = f32::from(BEND_MAX_POSITION);
    let x = ((at.x - plot.left()) / plot.width() * max).round();
    let span = f32::from(top) - f32::from(bottom);
    let y = (f32::from(bottom) + (plot.bottom() - at.y) / plot.height() * span).round();
    // Clamped to small ranges first, so the casts cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    BendPoint {
        position: x.clamp(0.0, max) as u8,
        value: y.clamp(f32::from(bottom), f32::from(top)) as i8,
    }
}

/// Where `point` lies on the screen.
fn to_screen(plot: Rect, (bottom, top): (i8, i8), point: BendPoint) -> Pos2 {
    let span = f32::from(top) - f32::from(bottom);
    pos2(
        plot.left() + plot.width() * f32::from(point.position) / f32::from(BEND_MAX_POSITION),
        plot.bottom() - plot.height() * (f32::from(point.value) - f32::from(bottom)) / span,
    )
}

/// Inserts a point at `point`'s position, unless one is there or the shape is full;
/// returns whether it did.
pub(super) fn insert_point(points: &mut Vec<BendPoint>, point: BendPoint) -> bool {
    if points.len() >= MAX_BEND_POINTS || points.iter().any(|p| p.position == point.position) {
        return false;
    }
    let at = points
        .iter()
        .position(|p| p.position > point.position)
        .unwrap_or(points.len());
    points.insert(at, point);
    true
}

/// Moves point `index` to `to`, kept strictly between its neighbours; returns whether it
/// moved.
pub(super) fn move_point(points: &mut [BendPoint], index: usize, to: BendPoint) -> bool {
    let low = index
        .checked_sub(1)
        .and_then(|before| points.get(before))
        .map_or(0, |before| before.position.saturating_add(1));
    let high = points
        .get(index + 1)
        .map_or(BEND_MAX_POSITION, |next| next.position.saturating_sub(1));
    let Some(point) = points.get_mut(index) else {
        return false;
    };
    let moved = BendPoint {
        position: to.position.clamp(low, high.max(low)),
        value: to.value,
    };
    if *point == moved {
        return false;
    }
    *point = moved;
    true
}

/// The shape's graph: a grid with value labels, the shape as a line, and a square handle
/// per point. Dragging a handle moves its point (snapped to a position and a quarter tone),
/// a click on the grid adds a point and a right-click (or double-click) on a handle removes
/// it. Returns whether the points changed.
fn graph(
    ui: &mut egui::Ui,
    points: &mut Vec<BendPoint>,
    span: (i8, i8),
    size: egui::Vec2,
    label: fn(i8) -> String,
) -> bool {
    let tokens = style::tokens(ui);
    let (rect, background) = ui.allocate_exact_size(size, Sense::click());
    ui.ctx()
        .accesskit_node_builder(background.id, |node| node.set_label("Shape graph"));
    let plot = Rect::from_min_max(
        pos2(rect.left() + LABEL_GUTTER, rect.top() + PLOT_MARGIN),
        pos2(rect.right() - PLOT_MARGIN, rect.bottom() - PLOT_MARGIN),
    );
    let (bottom, top) = span;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tokens.chrome);

    // The grid: every position, stronger every quarter of the note; every quarter tone
    // when there is room, labelled every half step (or wider when it is crowded).
    let minor = Stroke::new(1.0, tokens.line.gamma_multiply(0.55));
    let major = Stroke::new(1.0, tokens.line.gamma_multiply(1.6));
    for position in 0..=BEND_MAX_POSITION {
        let x = to_screen(plot, span, BendPoint { position, value: 0 }).x;
        let stroke = if position % 3 == 0 { major } else { minor };
        painter.vline(x, plot.y_range(), stroke);
    }
    let per_quarter = plot.height() / (f32::from(top) - f32::from(bottom));
    let label_every = [2_i8, 4, 8, 16]
        .into_iter()
        .find(|step| f32::from(*step) * per_quarter >= 22.0)
        .unwrap_or(16);
    let font = FontId::proportional(12.0);
    for value in bottom..=top {
        let y = to_screen(plot, span, BendPoint { position: 0, value }).y;
        if value % label_every == 0 {
            let stroke = if value == 0 && bottom < 0 {
                Stroke::new(1.5, tokens.text_weak.gamma_multiply(0.6))
            } else {
                major
            };
            painter.hline(plot.x_range(), y, stroke);
            painter.text(
                pos2(rect.left() + 8.0, y),
                Align2::LEFT_CENTER,
                label(value),
                font.clone(),
                tokens.text_weak,
            );
        } else if per_quarter >= 6.0 {
            painter.hline(plot.x_range(), y, minor);
        }
    }

    let mut changed = false;
    let mut remove = None;
    let mut hot = None;
    for index in 0..points.len() {
        let Some(point) = points.get(index) else {
            break;
        };
        let center = to_screen(plot, span, *point);
        let hit = Rect::from_center_size(center, vec2(HANDLE_HIT, HANDLE_HIT));
        let response = ui.interact(hit, background.id.with(index), Sense::click_and_drag());
        if response.dragged()
            && let Some(at) = response.interact_pointer_pos()
        {
            changed |= move_point(points, index, snap(plot, span, at));
        }
        if response.secondary_clicked() || response.double_clicked() {
            remove = Some(index);
        }
        if response.hovered() || response.dragged() {
            hot = Some(index);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }
    if let Some(index) = remove
        && points.len() > 1
    {
        points.remove(index);
        changed = true;
        hot = None;
    }
    if background.clicked()
        && let Some(at) = background.interact_pointer_pos()
        && plot.expand(HANDLE).contains(at)
    {
        changed |= insert_point(points, snap(plot, span, at));
    }

    // The shape, held flat before the first point and after the last.
    let screen: Vec<Pos2> = points.iter().map(|p| to_screen(plot, span, *p)).collect();
    let line = Stroke::new(2.5, tokens.accent);
    if let (Some(first), Some(last)) = (screen.first(), screen.last()) {
        let faint = Stroke::new(1.5, tokens.accent.gamma_multiply(0.4));
        painter.line_segment([pos2(plot.left(), first.y), *first], faint);
        painter.line_segment([*last, pos2(plot.right(), last.y)], faint);
    }
    painter.add(egui::Shape::line(screen.clone(), line));
    for (index, center) in screen.iter().enumerate() {
        let square = Rect::from_center_size(*center, vec2(HANDLE, HANDLE));
        let fill = if hot == Some(index) {
            tokens.accent
        } else {
            Color32::from_rgb(0xF2, 0xF4, 0xF7)
        };
        painter.rect_filled(square, 1.0, fill);
        painter.rect_stroke(
            square,
            1.0,
            Stroke::new(1.0, tokens.chrome),
            StrokeKind::Outside,
        );
    }
    if let Some(index) = hot
        && let Some(point) = points.get(index)
    {
        let center = to_screen(plot, span, *point);
        let text = format!("{} at {}/12", label(point.value), point.position);
        let anchor = if center.x > plot.center().x {
            Align2::RIGHT_BOTTOM
        } else {
            Align2::LEFT_BOTTOM
        };
        painter.text(
            center
                + vec2(
                    if anchor == Align2::LEFT_BOTTOM {
                        8.0
                    } else {
                        -8.0
                    },
                    -8.0,
                ),
            anchor,
            text,
            font,
            tokens.text,
        );
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(position: u8, value: i8) -> BendPoint {
        BendPoint { position, value }
    }

    #[test]
    fn values_are_labelled_like_bends_are_written() {
        let labels: Vec<_> = [0, 1, 2, 4, 6, 8, 11, -4, -2].map(value_label).into();
        assert_eq!(
            labels,
            ["0", "+¼", "+½", "+full", "+1½", "+2", "+2¾", "−full", "−½"]
        );
    }

    #[test]
    fn the_whammy_bar_counts_whole_steps() {
        let labels: Vec<_> = [0, 4, 8, -4, -6, -24].map(step_label).into();
        assert_eq!(labels, ["0", "+1", "+2", "−1", "−1½", "−6"]);
    }

    #[test]
    fn points_snap_to_positions_and_quarter_tones_inside_the_plot() {
        let plot = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 120.0));
        // 0 at the bottom, 12 quarter tones at the top.
        assert_eq!(snap(plot, (0, 12), pos2(61.0, 59.0)), p(6, 6));
        assert_eq!(snap(plot, (0, 12), pos2(-30.0, 500.0)), p(0, 0));
        assert_eq!(snap(plot, (0, 12), pos2(500.0, -30.0)), p(12, 12));
        // A whammy graph runs from -8 to 8: the middle is 0.
        assert_eq!(snap(plot, (-8, 8), pos2(0.0, 60.0)), p(0, 0));
        assert_eq!(snap(plot, (-8, 8), pos2(0.0, 120.0)), p(0, -8));
        let back = to_screen(plot, (-8, 8), p(6, -4));
        assert_eq!(snap(plot, (-8, 8), back), p(6, -4));
    }

    #[test]
    fn a_moved_point_stays_between_its_neighbours() {
        let mut points = vec![p(0, 0), p(6, 4), p(12, 4)];
        assert!(move_point(&mut points, 1, p(12, 2)));
        assert_eq!(points[1], p(11, 2));
        assert!(move_point(&mut points, 1, p(0, 2)));
        assert_eq!(points[1], p(1, 2));
        assert!(!move_point(&mut points, 1, p(1, 2)), "no change, no move");
        assert!(move_point(&mut points, 0, p(5, 3)));
        assert_eq!(points[0], p(0, 3), "the first point cannot pass the second");
    }

    #[test]
    fn points_are_inserted_in_order_once_per_position() {
        let mut points = vec![p(0, 0), p(12, 4)];
        assert!(insert_point(&mut points, p(6, 2)));
        assert_eq!(points, [p(0, 0), p(6, 2), p(12, 4)]);
        assert!(!insert_point(&mut points, p(6, 3)), "taken");
        let mut full: Vec<_> = (0..16).map(|i| p(i, 0)).collect();
        assert!(!insert_point(&mut full, p(5, 1)));
    }

    #[test]
    fn every_builtin_preset_is_a_valid_shape() {
        for builtin in BEND_PRESETS {
            gscore_model::BendShape::new(builtin.kind, to_points(builtin.points))
                .unwrap_or_else(|error| panic!("{}: {error}", builtin.name));
        }
        for builtin in WHAMMY_PRESETS {
            gscore_model::WhammyShape::new(to_points(builtin.points))
                .unwrap_or_else(|error| panic!("{}: {error}", builtin.name));
        }
    }

    #[test]
    fn the_graph_widens_to_fit_a_picked_shape() {
        let mut editor = ShapeEditor::new(ShapeTarget::Bend, BendKind::Up, vec![p(0, 0)]);
        assert_eq!(editor.span(), (0, 12));
        editor.pick(BendKind::Up, vec![p(0, 0), p(12, 20)]);
        assert_eq!(editor.span(), (0, 24));
        let whammy = ShapeEditor::new(ShapeTarget::Whammy, BendKind::Up, vec![p(0, -10)]);
        assert_eq!(whammy.span(), (-24, 24), "six steps up and down by default");
        let deep = ShapeEditor::new(ShapeTarget::Whammy, BendKind::Up, vec![p(0, -30)]);
        assert_eq!(deep.span(), (-32, 32));
    }

    #[test]
    fn saving_a_preset_under_a_taken_name_replaces_it() {
        let preset = |name: &str, value| ShapePreset {
            name: name.to_owned(),
            release: false,
            points: vec![(0, value)],
        };
        let mut presets = vec![preset("Mine", 2)];
        save_preset(&mut presets, preset("Mine", 4));
        save_preset(&mut presets, preset("Other", 6));
        assert_eq!(presets, [preset("Mine", 4), preset("Other", 6)]);
    }
}
