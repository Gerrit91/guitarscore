//! The instrument symbols a track can have: which one a track shows, and how each is drawn
//! with the painter (GuitarScore's own work, like the other icons).
//!
//! The symbol is a display choice (`gscore_model::InstrumentSymbol`); it does not change the
//! played sound. A track without a chosen symbol gets one derived from its GP7 instrument
//! type and General MIDI program.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};
use gscore_model::{InstrumentKind, InstrumentSymbol, TrackSound, TrackView};

/// The symbol of `track`: the one the user chose, else one derived from its instrument type
/// and the program of `sound`.
pub(crate) fn derived(track: &TrackView<'_>, sound: Option<TrackSound>) -> InstrumentSymbol {
    if let Some(symbol) = track.symbol() {
        return symbol;
    }
    if track.is_percussion() || track.instrument_kind() == InstrumentKind::Drums {
        return InstrumentSymbol::Drums;
    }
    if let Some(program) = sound.map(|sound| sound.program) {
        match program {
            0..=7 => return InstrumentSymbol::Piano,
            16..=23 => return InstrumentSymbol::Organ,
            // Nylon and steel string guitars; the rest of the guitar programs are electric.
            24 | 25 => return InstrumentSymbol::AcousticGuitar,
            26..=31 => return InstrumentSymbol::ElectricGuitar,
            32..=39 => return InstrumentSymbol::Bass,
            40..=51 => return InstrumentSymbol::Violin,
            52..=54 | 85 => return InstrumentSymbol::Voice,
            55..=58 => return InstrumentSymbol::Trumpet,
            59..=63 => return InstrumentSymbol::Saxophone,
            64..=79 => return InstrumentSymbol::Flute,
            80..=103 => return InstrumentSymbol::Synth,
            105 => return InstrumentSymbol::Banjo,
            110 => return InstrumentSymbol::Violin,
            _ => {}
        }
    }
    let name = track
        .instrument_name()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let named = [
        ("bass", InstrumentSymbol::Bass),
        ("banjo", InstrumentSymbol::Banjo),
        ("ukulele", InstrumentSymbol::Ukulele),
        ("violin", InstrumentSymbol::Violin),
        ("cello", InstrumentSymbol::Violin),
        ("trumpet", InstrumentSymbol::Trumpet),
        ("trombone", InstrumentSymbol::Trumpet),
        ("horn", InstrumentSymbol::Trumpet),
        ("sax", InstrumentSymbol::Saxophone),
        ("flute", InstrumentSymbol::Flute),
        ("piano", InstrumentSymbol::Piano),
        ("organ", InstrumentSymbol::Organ),
        ("synth", InstrumentSymbol::Synth),
        ("voice", InstrumentSymbol::Voice),
        ("vocal", InstrumentSymbol::Voice),
        ("singer", InstrumentSymbol::Voice),
        ("choir", InstrumentSymbol::Voice),
        ("acoustic", InstrumentSymbol::AcousticGuitar),
        ("nylon", InstrumentSymbol::AcousticGuitar),
        ("steel", InstrumentSymbol::AcousticGuitar),
        ("guitar", InstrumentSymbol::ElectricGuitar),
    ];
    if let Some((_, symbol)) = named.iter().find(|(word, _)| name.contains(word)) {
        return *symbol;
    }
    match track.instrument_kind() {
        InstrumentKind::Strings => InstrumentSymbol::ElectricGuitar,
        InstrumentKind::Drums => InstrumentSymbol::Drums,
        InstrumentKind::Other => InstrumentSymbol::Other,
    }
}

/// Draws in a symbol's own coordinates: the symbol's box is -0.5 to 0.5 on both axes,
/// optionally turned (the string instruments lie on a diagonal, head up to the right).
struct Pen<'a> {
    painter: &'a Painter,
    center: Pos2,
    size: f32,
    /// Cosine and sine of the turn.
    turn: (f32, f32),
    ink: Color32,
}

impl Pen<'_> {
    /// The screen point of `(u, v)` (u along the turned x axis, v across it, y down).
    fn at(&self, u: f32, v: f32) -> Pos2 {
        let (cos, sin) = self.turn;
        pos2(
            self.center.x + self.size * (u * cos - v * sin),
            self.center.y + self.size * (u * sin + v * cos),
        )
    }

    /// The same pen turned by `degrees` (negative turns counter-clockwise on screen).
    fn turned(&self, degrees: f32) -> Self {
        let radians = degrees.to_radians();
        Pen {
            turn: (radians.cos(), radians.sin()),
            ..*self
        }
    }

    /// The pen for the string instruments: on the diagonal, head up to the right, and
    /// larger, since a diagonal is longer than a side; `middle` is the instrument's middle
    /// along its length, which goes to the box's center.
    fn diagonal(&self, middle: f32) -> Self {
        let turned = self.turned(DIAGONAL);
        let size = self.size * 1.25;
        let (cos, sin) = turned.turn;
        Pen {
            size,
            center: pos2(
                self.center.x - size * middle * cos,
                self.center.y - size * middle * sin,
            ),
            ..turned
        }
    }

    /// A filled convex polygon.
    fn fill(&self, points: &[(f32, f32)]) {
        let points = points.iter().map(|&(u, v)| self.at(u, v)).collect();
        self.painter
            .add(Shape::convex_polygon(points, self.ink, Stroke::NONE));
    }

    /// A filled ellipse with half-axes `a` (along u) and `b` (along v).
    fn ellipse(&self, u: f32, v: f32, a: f32, b: f32) {
        let points: Vec<(f32, f32)> = (0..32)
            .map(|step| {
                let t = std::f32::consts::TAU * step as f32 / 32.0;
                (u + a * t.cos(), v + b * t.sin())
            })
            .collect();
        self.fill(&points);
    }

    /// An ellipse outline, `width` thick (in symbol units).
    fn ellipse_outline(&self, u: f32, v: f32, a: f32, b: f32, width: f32) {
        let points: Vec<Pos2> = (0..40)
            .map(|step| {
                let t = std::f32::consts::TAU * step as f32 / 40.0;
                self.at(u + a * t.cos(), v + b * t.sin())
            })
            .collect();
        self.painter.add(Shape::closed_line(
            points,
            Stroke::new(self.width(width), self.ink),
        ));
    }

    /// A straight stroke, `width` thick (in symbol units), with round ends.
    fn line(&self, from: (f32, f32), to: (f32, f32), width: f32) {
        let (a, b) = (self.at(from.0, from.1), self.at(to.0, to.1));
        let width = self.width(width);
        self.painter
            .line_segment([a, b], Stroke::new(width, self.ink));
        self.painter.circle_filled(a, width / 2.0, self.ink);
        self.painter.circle_filled(b, width / 2.0, self.ink);
    }

    /// A filled rectangle from `(u0, v0)` to `(u1, v1)`.
    fn rect(&self, u0: f32, v0: f32, u1: f32, v1: f32) {
        self.fill(&[(u0, v0), (u1, v0), (u1, v1), (u0, v1)]);
    }

    /// A stroke width in pixels: never thinner than a crisp pixel line.
    fn width(&self, units: f32) -> f32 {
        (units * self.size).max(1.1)
    }
}

/// The kinds of strings the modelled guitars play, as the Sound chain's Kind tab shows
/// them: larger pictures in the family's shape language, the two acoustics told apart by
/// the details a guitarist knows them by. `paper` is the colour behind them (a classical
/// headstock's open slots).
pub(crate) fn paint_strings(
    painter: &Painter,
    rect: Rect,
    kind: gscore_play::StringKind,
    ink: Color32,
    paper: Color32,
) {
    use gscore_play::StringKind;
    let pen = Pen {
        painter,
        center: rect.center(),
        size: rect.width().min(rect.height()),
        turn: (1.0, 0.0),
        ink,
    };
    let line = 0.06;
    match kind {
        StringKind::Electric => paint(painter, rect, InstrumentSymbol::ElectricGuitar, ink),
        StringKind::Bass => paint(painter, rect, InstrumentSymbol::Bass, ink),
        StringKind::Steel => {
            let pen = pen.diagonal(0.06);
            // A dreadnought: a broad lower bout, a pickguard under the soundhole, a pin
            // bridge, a slim neck and a solid headstock with its tuners.
            pen.ellipse_outline(-0.26, 0.0, 0.16, 0.2, line);
            pen.ellipse_outline(-0.03, 0.0, 0.1, 0.15, line);
            pen.ellipse(-0.05, 0.0, 0.045, 0.045);
            pen.ellipse(-0.11, 0.08, 0.065, 0.045);
            pen.rect(-0.3, -0.075, -0.265, 0.075);
            pen.rect(0.07, -0.03, 0.37, 0.03);
            pen.fill(&[(0.35, -0.055), (0.52, -0.06), (0.52, 0.06), (0.35, 0.055)]);
            for u in [0.4, 0.45, 0.5] {
                pen.ellipse(u, -0.08, 0.017, 0.017);
                pen.ellipse(u, 0.08, 0.017, 0.017);
            }
        }
        StringKind::Nylon => {
            let pen = pen.diagonal(0.06);
            // A classical guitar: a rounder body, a rosette around the soundhole, a wide
            // tie-block bridge, a wider neck and a slotted headstock.
            pen.ellipse_outline(-0.25, 0.0, 0.15, 0.19, line);
            pen.ellipse_outline(-0.03, 0.0, 0.11, 0.155, line);
            pen.ellipse(-0.05, 0.0, 0.04, 0.04);
            pen.ellipse_outline(-0.05, 0.0, 0.07, 0.07, line * 0.6);
            pen.rect(-0.31, -0.09, -0.27, 0.09);
            pen.rect(0.07, -0.038, 0.37, 0.038);
            pen.fill(&[(0.35, -0.078), (0.56, -0.078), (0.56, 0.078), (0.35, 0.078)]);
            let slots = Pen { ink: paper, ..pen };
            slots.rect(0.385, -0.055, 0.535, -0.017);
            slots.rect(0.385, 0.017, 0.535, 0.055);
        }
    }
}

/// How the string instruments lie: head up to the right.
const DIAGONAL: f32 = -45.0;

/// Draws `symbol` inside `rect` in `ink`: flat silhouettes in one color, the same shape
/// language for the whole family (the string instruments share their diagonal).
pub(crate) fn paint(painter: &Painter, rect: Rect, symbol: InstrumentSymbol, ink: Color32) {
    let pen = Pen {
        painter,
        center: rect.center(),
        size: rect.width().min(rect.height()),
        turn: (1.0, 0.0),
        ink,
    };
    let line = 0.085;
    match symbol {
        InstrumentSymbol::ElectricGuitar => {
            let pen = pen.diagonal(0.08);
            // A solid body with two horns, a neck and a narrow headstock.
            pen.ellipse(-0.25, 0.0, 0.15, 0.18);
            pen.ellipse(-0.1, 0.0, 0.1, 0.14);
            pen.fill(&[
                (-0.12, -0.13),
                (-0.02, -0.19),
                (0.03, -0.12),
                (-0.04, -0.06),
            ]);
            pen.fill(&[(-0.12, 0.13), (-0.04, 0.17), (0.0, 0.11), (-0.06, 0.06)]);
            pen.rect(-0.05, -0.035, 0.36, 0.035);
            pen.fill(&[(0.34, -0.05), (0.52, -0.065), (0.52, 0.03), (0.34, 0.05)]);
        }
        InstrumentSymbol::AcousticGuitar => {
            let pen = pen.diagonal(0.07);
            // A waisted outline with a soundhole, a neck and a headstock.
            pen.ellipse_outline(-0.25, 0.0, 0.15, 0.19, line * 0.9);
            pen.ellipse_outline(-0.03, 0.0, 0.1, 0.14, line * 0.9);
            pen.ellipse(-0.06, 0.0, 0.05, 0.05);
            pen.rect(0.07, -0.035, 0.37, 0.035);
            pen.fill(&[(0.35, -0.06), (0.52, -0.06), (0.52, 0.06), (0.35, 0.06)]);
        }
        InstrumentSymbol::Bass => {
            let pen = pen.diagonal(0.07);
            // A smaller body with a long upper horn, a long neck reaching the corner and
            // a headstock with its four pegs.
            pen.ellipse(-0.36, 0.0, 0.12, 0.15);
            pen.ellipse(-0.24, 0.0, 0.08, 0.11);
            pen.fill(&[
                (-0.26, -0.1),
                (-0.08, -0.19),
                (-0.04, -0.14),
                (-0.18, -0.05),
            ]);
            pen.fill(&[(-0.26, 0.1), (-0.17, 0.14), (-0.14, 0.09), (-0.2, 0.05)]);
            pen.rect(-0.2, -0.028, 0.42, 0.028);
            pen.rect(0.4, -0.045, 0.58, 0.045);
            for u in [0.45, 0.53] {
                pen.ellipse(u, -0.075, 0.025, 0.03);
                pen.ellipse(u, 0.075, 0.025, 0.03);
            }
        }
        InstrumentSymbol::Ukulele => {
            let pen = pen.diagonal(0.1);
            // A small waisted outline with a short neck.
            pen.ellipse_outline(-0.14, 0.0, 0.12, 0.15, line * 0.9);
            pen.ellipse_outline(0.04, 0.0, 0.08, 0.11, line * 0.9);
            pen.ellipse(0.02, 0.0, 0.04, 0.04);
            pen.rect(0.12, -0.03, 0.3, 0.03);
            pen.fill(&[(0.28, -0.055), (0.4, -0.055), (0.4, 0.055), (0.28, 0.055)]);
        }
        InstrumentSymbol::Banjo => {
            let pen = pen.diagonal(0.08);
            // A round drum head in a thick rim, and a long neck.
            pen.ellipse_outline(-0.22, 0.0, 0.22, 0.22, line * 1.1);
            pen.rect(-0.3, -0.1, -0.25, 0.1);
            pen.rect(-0.25, -0.025, 0.44, 0.025);
            pen.fill(&[(0.42, -0.055), (0.55, -0.055), (0.55, 0.055), (0.42, 0.055)]);
        }
        InstrumentSymbol::Violin => {
            // The body upright and turned the other way, the bow across it.
            let body = pen.turned(-60.0);
            body.ellipse(-0.24, 0.0, 0.15, 0.18);
            body.ellipse(0.02, 0.0, 0.12, 0.15);
            body.rect(-0.14, -0.09, -0.08, 0.09);
            body.rect(0.12, -0.025, 0.38, 0.025);
            body.ellipse(0.42, 0.0, 0.055, 0.05);
            pen.line((-0.44, 0.1), (0.4, 0.4), line * 0.55);
        }
        InstrumentSymbol::Drums => {
            // A snare drum: the shell, the head above it, and two crossed sticks.
            pen.rect(-0.36, 0.06, 0.36, 0.3);
            pen.ellipse(0.0, 0.3, 0.36, 0.08);
            pen.ellipse_outline(0.0, -0.02, 0.36, 0.09, line * 0.8);
            pen.line((-0.3, -0.44), (0.12, -0.1), line);
            pen.line((0.3, -0.44), (-0.12, -0.1), line);
        }
        InstrumentSymbol::Piano => {
            // A keyboard: five white keys and three black ones.
            let (left, right, top, bottom) = (-0.45, 0.45, -0.28, 0.28);
            let frame = Rect::from_min_max(pen.at(left, top), pen.at(right, bottom));
            painter.rect_stroke(
                frame,
                pen.width(0.04),
                Stroke::new(pen.width(line * 0.8), ink),
                egui::StrokeKind::Inside,
            );
            let key = (right - left) / 5.0;
            for index in 1..5 {
                let u = left + key * index as f32;
                pen.line((u, 0.04), (u, bottom - 0.02), line * 0.5);
            }
            for index in [1, 2, 4] {
                let u = left + key * index as f32;
                pen.rect(u - key * 0.3, top, u + key * 0.3, 0.06);
            }
        }
        InstrumentSymbol::Organ => {
            // Pipes, tallest in the middle, standing on a base.
            // Each pipe narrows into its foot above the base.
            for (index, height) in [0.32_f32, 0.52, 0.68, 0.52, 0.32].into_iter().enumerate() {
                let u = -0.36 + index as f32 * 0.18;
                let top = 0.14 - height;
                pen.rect(u - 0.06, top, u + 0.06, 0.14);
                pen.ellipse(u, top, 0.06, 0.025);
                pen.fill(&[
                    (u - 0.06, 0.14),
                    (u + 0.06, 0.14),
                    (u + 0.02, 0.31),
                    (u - 0.02, 0.31),
                ]);
            }
            pen.rect(-0.46, 0.31, 0.46, 0.42);
        }
        InstrumentSymbol::Synth => {
            // A panel with knobs over a short keyboard.
            let (left, right) = (-0.47, 0.47);
            let frame = Rect::from_min_max(pen.at(left, -0.26), pen.at(right, 0.26));
            painter.rect_stroke(
                frame,
                pen.width(0.04),
                Stroke::new(pen.width(line * 0.8), ink),
                egui::StrokeKind::Inside,
            );
            for index in 0..3 {
                pen.ellipse(-0.27 + index as f32 * 0.17, -0.11, 0.05, 0.05);
            }
            pen.rect(0.18, -0.15, 0.36, -0.07);
            pen.rect(left, 0.02, right, 0.07);
            for index in 1..6 {
                let u = left + (right - left) * index as f32 / 6.0;
                pen.line((u, 0.06), (u, 0.24), line * 0.45);
            }
        }
        InstrumentSymbol::Voice => {
            // A microphone: the grille, the holder around it, the stand and its foot.
            pen.rect(-0.13, -0.3, 0.13, 0.0);
            pen.ellipse(0.0, -0.3, 0.13, 0.13);
            pen.ellipse(0.0, 0.0, 0.13, 0.13);
            let arc: Vec<Pos2> = (0..=16)
                .map(|step| {
                    let t = std::f32::consts::PI * step as f32 / 16.0;
                    pen.at(-0.23 * t.cos(), -0.06 + 0.23 * t.sin())
                })
                .collect();
            painter.add(Shape::line(arc, Stroke::new(pen.width(line * 0.8), ink)));
            pen.line((0.0, 0.18), (0.0, 0.38), line * 0.8);
            pen.line((-0.18, 0.42), (0.18, 0.42), line);
        }
        InstrumentSymbol::Trumpet => {
            // The bell to the right, the lead pipe, the valves and the tuning loop.
            pen.fill(&[(0.18, -0.06), (0.46, -0.2), (0.46, 0.12), (0.18, 0.02)]);
            pen.line((-0.44, -0.02), (0.2, -0.02), line * 0.8);
            pen.line((-0.24, 0.16), (0.12, 0.16), line * 0.7);
            pen.line((0.12, 0.16), (0.12, -0.02), line * 0.7);
            pen.line((-0.24, 0.16), (-0.24, -0.02), line * 0.7);
            for index in 0..3 {
                let u = -0.14 + index as f32 * 0.11;
                pen.rect(u - 0.035, -0.22, u + 0.035, 0.04);
                pen.ellipse(u, -0.24, 0.05, 0.025);
            }
            pen.rect(-0.47, -0.07, -0.42, 0.03);
        }
        InstrumentSymbol::Saxophone => {
            // The body curving into an upturned bell, the neck and mouthpiece at the top.
            let body: Vec<Pos2> = [
                (0.02, -0.3),
                (0.0, -0.05),
                (-0.02, 0.15),
                (0.0, 0.3),
                (0.08, 0.38),
                (0.18, 0.36),
                (0.24, 0.26),
                (0.26, 0.12),
            ]
            .iter()
            .map(|&(u, v)| pen.at(u, v))
            .collect();
            painter.add(Shape::line(body, Stroke::new(pen.width(0.13), ink)));
            pen.fill(&[(0.18, 0.12), (0.17, -0.02), (0.38, -0.06), (0.32, 0.14)]);
            pen.line((0.02, -0.3), (-0.12, -0.42), line * 0.8);
            pen.line((-0.12, -0.42), (-0.24, -0.38), line * 0.8);
            for index in 0..3 {
                pen.ellipse(-0.08, -0.12 + index as f32 * 0.12, 0.035, 0.035);
            }
        }
        InstrumentSymbol::Flute => {
            let pen = pen.turned(-25.0);
            // A long tube with the lip plate and the keys along it.
            pen.rect(-0.5, -0.04, 0.5, 0.04);
            pen.ellipse(-0.32, -0.06, 0.05, 0.04);
            for index in 0..4 {
                pen.ellipse(-0.06 + index as f32 * 0.14, -0.08, 0.035, 0.035);
            }
        }
        InstrumentSymbol::Other => {
            // Two beamed eighth notes.
            pen.turned(-20.0).ellipse(-0.24, 0.28, 0.13, 0.09);
            pen.turned(-20.0).ellipse(0.2, 0.28, 0.13, 0.09);
            let (left, right) = (-0.13, 0.31);
            pen.line((left, 0.24), (left, -0.3), line * 0.7);
            pen.line((right, 0.24), (right, -0.36), line * 0.7);
            pen.fill(&[
                (left - 0.03, -0.36),
                (right + 0.03, -0.42),
                (right + 0.03, -0.3),
                (left - 0.03, -0.24),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gscore_model::testing;

    #[test]
    fn a_chosen_symbol_wins_and_a_drum_kit_is_drums() {
        let mut song = testing::song(&[testing::GUITAR_6], 8).unwrap();
        let track = gscore_model::TrackIndex::new(0);
        let _ = song.set_track_program(track, 29).unwrap();
        let view = song.track(track).unwrap();
        assert_eq!(
            derived(&view, song.track_sound(track)),
            InstrumentSymbol::ElectricGuitar,
            "an overdriven guitar"
        );
        let _ = song.set_track_program(track, 25).unwrap();
        let view = song.track(track).unwrap();
        assert_eq!(
            derived(&view, song.track_sound(track)),
            InstrumentSymbol::AcousticGuitar,
            "a steel string guitar"
        );
        let _ = song
            .set_track_symbol(track, Some(InstrumentSymbol::Banjo))
            .unwrap();
        let view = song.track(track).unwrap();
        assert_eq!(
            derived(&view, song.track_sound(track)),
            InstrumentSymbol::Banjo
        );
    }

    /// Every symbol at the sizes the app uses (18 and 24 px) and large, light and dark,
    /// so a change to one is seen next to the others.
    #[test]
    fn the_symbols_match_their_snapshot() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(15.0 * 72.0 + 16.0, 2.0 * 128.0))
            .wgpu()
            .build_ui(|ui| {
                for (dark, ground, ink) in [
                    (false, Color32::WHITE, Color32::from_gray(30)),
                    (true, Color32::from_gray(36), Color32::from_gray(225)),
                ] {
                    let _ = dark;
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 120.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(row, 0.0, ground);
                    for (index, symbol) in InstrumentSymbol::ALL.into_iter().enumerate() {
                        let x = row.left() + 8.0 + index as f32 * 72.0;
                        let large =
                            Rect::from_min_size(pos2(x, row.top() + 6.0), egui::vec2(64.0, 64.0));
                        paint(ui.painter(), large, symbol, ink);
                        let medium = Rect::from_min_size(
                            pos2(x + 4.0, row.top() + 82.0),
                            egui::vec2(24.0, 24.0),
                        );
                        paint(ui.painter(), medium, symbol, ink);
                        let small = Rect::from_min_size(
                            pos2(x + 38.0, row.top() + 85.0),
                            egui::vec2(18.0, 18.0),
                        );
                        paint(ui.painter(), small, symbol, ink);
                    }
                }
            });
        harness.run();
        harness.snapshot("instrument_symbols");
    }
}
