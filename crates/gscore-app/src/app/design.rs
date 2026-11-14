//! Guitar Pro's design mode on the page: a dashed guide with a drag handle for every beat
//! column and every barline, and the buttons that make a line hold more or fewer bars.
//!
//! The mode edits layout only: a barline drag sets a bar's width (`set_measure_scale`), a
//! column drag sets a beat's offset (`set_beat_spacing`). Both are ordinary commands, so the
//! whole drag is one undo step and a recovery snapshot holds it like any other edit.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use gscore_edit::Command;
use gscore_layout::{ColorToken, MeasurePlace, Places, Point, Theme};
use gscore_model::{BeatPosition, DESIGN_SCALE_RANGE, MeasureIndex, Song, StaffIndex, TrackIndex};

use crate::canvas::POINTS_PER_SPACE;

/// How far above a system's top line the handles sit, in staff spaces.
const HANDLE_RISE: f32 = 1.6;
/// The snap distance of the alignment guide, in points.
const SNAP: f32 = 8.0;
/// How much one press of `+` or `-` widens or narrows a bar, as a share of its width.
const WIDTH_STEP: f32 = 0.1;

/// What the design mode keeps between frames: the drag in progress.
#[derive(Debug, Default)]
pub(super) struct DesignState {
    drag: Option<Drag>,
}

impl DesignState {
    /// Whether a drag is in progress (the page must not select beats then).
    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// One handle of the design mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    /// A beat column, addressed by the beat that stands for it (the first of its column).
    Column(BeatPosition),
    /// The barline at the end of a measure.
    Barline(MeasureIndex),
}

/// A drag in progress.
#[derive(Debug)]
struct Drag {
    handle: Handle,
    /// Where the handle was when the drag started, in staff spaces.
    x: f32,
    /// The column's offset, or the bar's width, when the drag started (staff spaces).
    value: f32,
    /// The bar's natural width (at a scale of 1) when the drag started, in staff spaces.
    natural: f32,
}

/// What the overlay edits: the song, the staff the handles follow, and whose bars change.
pub(super) struct Overlay<'a> {
    /// The song.
    pub(super) song: &'a Song,
    /// The staff the handles follow (the cursor's).
    pub(super) track: TrackIndex,
    /// Which staff of the track.
    pub(super) staff: StaffIndex,
    /// The track whose bar width changes; `None` when several tracks are shown, where the
    /// bars are the song's and every track follows them.
    pub(super) scope: Option<TrackIndex>,
}

impl Overlay<'_> {
    /// The width command for `measure` at `percent` of its natural width.
    fn scale_command(&self, measure: MeasureIndex, percent: f32) -> Command {
        let percent = percent
            .round()
            .clamp(DESIGN_SCALE_RANGE.0 * 100.0, DESIGN_SCALE_RANGE.1 * 100.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let percent = percent as u16;
        Command::SetMeasureScale {
            track: self.scope,
            staff: self.staff,
            measure,
            percent: Some(percent),
        }
    }

    /// The percent of its natural width `measure` has now.
    fn percent_of(&self, measure: MeasureIndex) -> f32 {
        self.song.measure_scale(self.track, self.staff, measure) * 100.0
    }
}

/// Draws the design mode over the page and turns the pointer's gestures into `commands`,
/// which the caller applies (with merge, so one drag is one undo step).
#[allow(clippy::too_many_arguments)]
pub(super) fn overlay(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    places: &Places,
    to_screen: &dyn Fn(Point) -> Pos2,
    visible: Rect,
    zoom: f32,
    theme: &Theme,
    design: &Overlay<'_>,
    state: &mut DesignState,
    dragged: &mut Option<Command>,
    commands: &mut Vec<Command>,
) {
    let scale = POINTS_PER_SPACE * zoom;
    let guide = canvas_color(theme.color(ColorToken::Notation)).gamma_multiply(0.5);
    let handle_color = canvas_color(theme.color(ColorToken::Cursor));
    // Guitar Pro's alignment guide is yellow; it is an editing aid, not part of the score.
    let align = Color32::from_rgb(224, 190, 60);

    // Every barline and column in sight, for the alignment guide to snap to.
    let mut candidates: Vec<f32> = Vec::new();
    for measure in &places.measures {
        let x = to_screen(Point::new(measure.left, 0.0)).x;
        if visible.x_range().expand(300.0).contains(x) {
            candidates.push(measure.right);
            candidates.extend(measure.beats.iter().map(|beat| beat.x));
        }
    }

    for (system_index, system) in places.systems.iter().enumerate() {
        let measures: Vec<&MeasurePlace> = places
            .measures
            .iter()
            .filter(|measure| measure.system == system_index)
            .collect();
        let Some(first) = measures.first() else {
            continue;
        };
        let top = to_screen(Point::new(first.left, system.top)).y;
        let bottom = to_screen(Point::new(first.left, system.tab_bottom())).y;
        let left = to_screen(Point::new(first.left, 0.0)).x;
        let right = measures
            .last()
            .map_or(left, |last| to_screen(Point::new(last.right, 0.0)).x);
        if !visible.intersects(Rect::from_min_max(
            Pos2::new(left - 20.0, top - 40.0),
            Pos2::new(right + 70.0, bottom + 10.0),
        )) {
            continue;
        }

        for measure in &measures {
            let mut handles: Vec<(Handle, f32)> = columns(design, measure);
            handles.push((Handle::Barline(measure.measure), measure.right));
            for (handle, x) in handles {
                let screen_x = to_screen(Point::new(x, 0.0)).x;
                if !visible.x_range().expand(20.0).contains(screen_x) {
                    continue;
                }
                let active = state
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.handle == handle);
                let rect = Rect::from_center_size(
                    Pos2::new(screen_x, top - HANDLE_RISE * scale),
                    Vec2::new(16.0, 20.0),
                );
                let id = ui.scope_id().with(("design_handle", handle_key(handle)));
                let response = ui.interact(rect, id, Sense::click_and_drag());
                let label = match handle {
                    Handle::Column(at) => format!(
                        "Move beat {} of measure {}",
                        at.beat.get().saturating_add(1),
                        at.measure.get().saturating_add(1)
                    ),
                    Handle::Barline(measure) => {
                        format!("Barline of measure {}", measure.get().saturating_add(1))
                    }
                };
                response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                let hot = active || response.hovered();
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                // The dashed guide through the staves, and the handle above them.
                painter.add(egui::Shape::dashed_line(
                    &[Pos2::new(screen_x, top - 8.0), Pos2::new(screen_x, bottom)],
                    Stroke::new(
                        if hot { 1.5 } else { 1.0 },
                        if hot { handle_color } else { guide },
                    ),
                    3.0,
                    3.0,
                ));
                let at = Pos2::new(screen_x, top - HANDLE_RISE * scale);
                painter.add(egui::Shape::convex_polygon(
                    vec![at + Vec2::new(-5.0, -7.0), at + Vec2::new(5.0, -7.0), at],
                    handle_color,
                    Stroke::NONE,
                ));

                if response.drag_started() {
                    state.drag = Some(start_drag(design, places, handle, x));
                }
                if response.dragged()
                    && state
                        .drag
                        .as_ref()
                        .is_some_and(|drag| drag.handle == handle)
                {
                    let delta = response.drag_delta().x / scale;
                    let mut target = x + delta;
                    if let Some(snapped) = snap(&candidates, target, x, scale) {
                        target = snapped;
                        let line = to_screen(Point::new(target, 0.0)).x;
                        painter.add(egui::Shape::dashed_line(
                            &[Pos2::new(line, top - 8.0), Pos2::new(line, bottom)],
                            Stroke::new(1.5, align),
                            4.0,
                            3.0,
                        ));
                    }
                    if let Some(command) = drag_command(design, state, handle, target) {
                        *dragged = Some(command);
                    }
                }
                if response.drag_stopped() {
                    state.drag = None;
                }
            }
        }

        // The buttons at the end of the line: wider (fewer bars), narrower (more bars), and
        // reset.
        // Stacked above the handles at the line's right end, inside the page's margin (a
        // row of them would run off the paper and out of reach).
        let end = Pos2::new(right + 9.0, top - HANDLE_RISE * scale - 20.0);
        for (index, label) in ["+", "-", "x"].iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let offset = index as f32 * 17.0;
            let rect = Rect::from_center_size(end - Vec2::new(0.0, offset), Vec2::new(16.0, 16.0));
            let id = ui.scope_id().with(("design_button", system_index, index));
            let response = ui.interact(rect, id, Sense::click());
            let description = match *label {
                "+" => "Widen the line's bars",
                "-" => "Narrow the line's bars",
                _ => "Reset the line's layout",
            };
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, description));
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            painter.rect_filled(
                rect,
                3.0,
                canvas_color(theme.color(ColorToken::Cursor)).gamma_multiply(0.18),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::proportional(12.0),
                canvas_color(theme.color(ColorToken::Notation)),
            );
            if !response.clicked() {
                continue;
            }
            let list: Vec<MeasureIndex> = measures.iter().map(|measure| measure.measure).collect();
            match *label {
                // One bar fewer: the line ends at its last bar (the rest flow on, and
                // the line stretches its remaining bars). A one-bar line stays as it is.
                "+" => {
                    if list.len() > 1
                        && let Some(last) = list.last().copied()
                    {
                        commands.push(Command::SetSystemBreak {
                            track: design.track,
                            measure: last,
                            kind: gscore_model::SystemBreak::Line,
                        });
                    }
                }
                // One bar more: the next line gives its first bar back when a break holds
                // it; otherwise the bars make room for it until it fits.
                "-" => {
                    let next = list
                        .last()
                        .map(|last| MeasureIndex::new(last.get().saturating_add(1)));
                    let forced = next.is_some_and(|measure| {
                        design
                            .song
                            .track(design.track)
                            .and_then(|track| track.measure(measure))
                            .is_some_and(|measure| {
                                measure.system_break() != gscore_model::SystemBreak::None
                            })
                    });
                    match next {
                        Some(measure) if forced => commands.push(Command::SetSystemBreak {
                            track: design.track,
                            measure,
                            kind: gscore_model::SystemBreak::None,
                        }),
                        _ => {
                            for measure in list {
                                let percent = design.percent_of(measure) * (1.0 - WIDTH_STEP);
                                commands.push(design.scale_command(measure, percent));
                            }
                        }
                    }
                }
                _ => commands.extend(reset_commands(design, &list)),
            }
        }
    }
}

/// The column handles of a measure: one per column, addressed by the first beat that starts
/// there.
fn columns(design: &Overlay<'_>, measure: &MeasurePlace) -> Vec<(Handle, f32)> {
    let mut columns: Vec<(f32, BeatPosition)> = Vec::new();
    for beat in &measure.beats {
        let at = BeatPosition {
            track: design.track,
            staff: design.staff,
            measure: measure.measure,
            voice: beat.voice,
            beat: beat.beat,
        };
        match columns.last_mut() {
            Some((x, first)) if (*x - beat.x).abs() < 0.01 => {
                // The first beat of the column stands for it.
                if (beat.voice.get(), beat.beat.get()) < (first.voice.get(), first.beat.get()) {
                    *first = at;
                }
            }
            _ => columns.push((beat.x, at)),
        }
    }
    columns
        .into_iter()
        .map(|(x, at)| (Handle::Column(at), x))
        .collect()
}

/// A handle's identity for egui's interaction.
fn handle_key(handle: Handle) -> (u8, usize, usize, usize) {
    match handle {
        Handle::Column(at) => (1, at.measure.get(), at.voice.get(), at.beat.get()),
        Handle::Barline(measure) => (2, measure.get(), 0, 0),
    }
}

/// What a drag starts from.
fn start_drag(design: &Overlay<'_>, places: &Places, handle: Handle, x: f32) -> Drag {
    match handle {
        Handle::Column(at) => Drag {
            handle,
            x,
            value: design
                .song
                .beat(at)
                .and_then(|beat| beat.spacing())
                .unwrap_or(0.0),
            natural: 1.0,
        },
        Handle::Barline(measure) => {
            let width = places
                .measure(measure)
                .map_or(0.0, |place| (place.right - place.left).max(0.0));
            let percent = design.percent_of(measure) / 100.0;
            Drag {
                handle,
                x,
                value: width,
                natural: width / percent.max(0.01),
            }
        }
    }
}

/// The command a drag to `target` (staff spaces) makes.
fn drag_command(
    design: &Overlay<'_>,
    state: &DesignState,
    handle: Handle,
    target: f32,
) -> Option<Command> {
    let drag = state.drag.as_ref()?;
    if drag.handle != handle {
        return None;
    }
    let moved = target - drag.x;
    match handle {
        Handle::Column(at) => {
            let offset = drag.value + moved;
            let hundredths = (offset * 100.0).round().clamp(-400.0, 400.0);
            #[allow(clippy::cast_possible_truncation)]
            let hundredths = hundredths as i16;
            Some(Command::SetBeatSpacing {
                at,
                offset: (hundredths != 0).then_some(hundredths),
            })
        }
        Handle::Barline(measure) => {
            let width = drag.value + moved;
            let percent = (width / drag.natural.max(0.01)) * 100.0;
            Some(design.scale_command(measure, percent))
        }
    }
}

/// The nearest candidate to `target` (staff spaces) within the snap distance, if any.
fn snap(candidates: &[f32], target: f32, own: f32, scale: f32) -> Option<f32> {
    candidates
        .iter()
        .copied()
        .filter(|candidate| (candidate - own).abs() > 0.01)
        .filter(|candidate| (candidate - target).abs() * scale <= SNAP)
        .min_by(|a, b| {
            (a - target)
                .abs()
                .partial_cmp(&(b - target).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// The commands that clear a system's design-mode layout: bar widths and moved columns.
fn reset_commands(design: &Overlay<'_>, measures: &[MeasureIndex]) -> Vec<Command> {
    let mut commands = Vec::new();
    for &measure in measures {
        if design.song.song_measure_scale(measure).is_some() {
            commands.push(Command::SetMeasureScale {
                track: None,
                staff: design.staff,
                measure,
                percent: None,
            });
        }
        let Some(view) = design
            .song
            .track(design.track)
            .and_then(|track| track.measure(measure))
        else {
            continue;
        };
        if view.display_scale().is_some() {
            commands.push(Command::SetMeasureScale {
                track: Some(design.track),
                staff: design.staff,
                measure,
                percent: None,
            });
        }
        for voice in view.voices() {
            for beat in voice.beats() {
                if beat.spacing().is_some() {
                    commands.push(Command::SetBeatSpacing {
                        at: BeatPosition {
                            track: design.track,
                            staff: design.staff,
                            measure,
                            voice: voice.index(),
                            beat: beat.index(),
                        },
                        offset: None,
                    });
                }
            }
        }
    }
    commands
}

/// A layout color as an egui color.
fn canvas_color(rgb: gscore_layout::Rgb) -> Color32 {
    crate::canvas::color(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gscore_model::testing;

    fn test_song() -> Song {
        testing::song(&[testing::GUITAR_6], 2).unwrap()
    }

    fn column(beat: usize) -> BeatPosition {
        BeatPosition {
            track: TrackIndex::new(0),
            staff: StaffIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(beat),
        }
    }

    fn overlay(song: &Song) -> Overlay<'_> {
        Overlay {
            song,
            track: TrackIndex::new(0),
            staff: StaffIndex::new(0),
            scope: Some(TrackIndex::new(0)),
        }
    }

    #[test]
    fn a_column_drag_moves_it_by_whole_hundredths_of_a_staff_space() {
        let song = test_song();
        let design = overlay(&song);
        let mut state = DesignState {
            drag: Some(Drag {
                handle: Handle::Column(column(1)),
                x: 10.0,
                value: 0.0,
                natural: 1.0,
            }),
        };
        let command = drag_command(&design, &state, Handle::Column(column(1)), 10.234).unwrap();
        assert_eq!(
            command,
            Command::SetBeatSpacing {
                at: column(1),
                offset: Some(23)
            }
        );
        // Back where it was is the natural place (no offset at all).
        let command = drag_command(&design, &state, Handle::Column(column(1)), 10.0).unwrap();
        assert_eq!(
            command,
            Command::SetBeatSpacing {
                at: column(1),
                offset: None
            }
        );
        // An absurd drag stays inside the offset range.
        let command = drag_command(&design, &state, Handle::Column(column(1)), 90.0).unwrap();
        assert_eq!(
            command,
            Command::SetBeatSpacing {
                at: column(1),
                offset: Some(400)
            }
        );
        // Another handle is not this drag's.
        assert!(drag_command(&design, &state, Handle::Column(column(2)), 11.0).is_none());
        state.drag = None;
        assert!(drag_command(&design, &state, Handle::Column(column(1)), 11.0).is_none());
    }

    #[test]
    fn a_barline_drag_sets_the_bar_width_in_percent() {
        let song = test_song();
        let design = overlay(&song);
        let measure = MeasureIndex::new(0);
        let state = DesignState {
            drag: Some(Drag {
                handle: Handle::Barline(measure),
                x: 20.0,
                value: 10.0,
                natural: 10.0,
            }),
        };
        let command = drag_command(&design, &state, Handle::Barline(measure), 22.0).unwrap();
        assert_eq!(
            command,
            Command::SetMeasureScale {
                track: Some(TrackIndex::new(0)),
                staff: StaffIndex::new(0),
                measure,
                percent: Some(120)
            }
        );
        // Written down twice does not make it wider and wider: the drag is absolute.
        let again = drag_command(&design, &state, Handle::Barline(measure), 22.0).unwrap();
        assert_eq!(again, command);
        // It never shrinks a bar below a quarter of its width.
        let command = drag_command(&design, &state, Handle::Barline(measure), -100.0).unwrap();
        assert_eq!(
            command,
            Command::SetMeasureScale {
                track: Some(TrackIndex::new(0)),
                staff: StaffIndex::new(0),
                measure,
                percent: Some(25)
            }
        );
    }

    #[test]
    fn the_guide_snaps_to_a_nearby_line_only() {
        let candidates = [0.0, 10.0, 10.5];
        // Half a staff space at 7 points per space is 3.5 points: within the snap distance.
        assert_eq!(snap(&candidates, 10.4, 5.0, 7.0), Some(10.5));
        // Two staff spaces away is not.
        assert_eq!(snap(&candidates, 12.0, 5.0, 7.0), None);
        // Without another line nearby there is nothing to snap to.
        assert_eq!(snap(&[10.0], 10.0, 10.0, 7.0), None);
    }

    #[test]
    fn reset_clears_the_widths_and_the_moved_columns_of_the_line() {
        let mut song = test_song();
        let measure = MeasureIndex::new(0);
        let _ = song
            .set_measure_scale(gscore_model::ScaleScope::Song, measure, Some(1.5))
            .unwrap();
        let _ = song
            .set_measure_scale(
                gscore_model::ScaleScope::Staff {
                    track: TrackIndex::new(0),
                    staff: StaffIndex::new(0),
                },
                measure,
                Some(1.25),
            )
            .unwrap();
        let _ = song.set_beat_spacing(column(1), Some(0.5)).unwrap();
        let design = overlay(&song);
        let commands = reset_commands(&design, &[measure]);
        // Both widths and the moved column, as a batch.
        assert_eq!(commands.len(), 3, "{commands:?}");
        for command in commands {
            match command {
                Command::SetMeasureScale { percent, .. } => assert_eq!(percent, None),
                Command::SetBeatSpacing { offset, .. } => assert_eq!(offset, None),
                other => panic!("unexpected {other:?}"),
            }
        }
        // A line with nothing set asks for nothing.
        let plain = test_song();
        assert!(reset_commands(&overlay(&plain), &[measure]).is_empty());
    }
}
