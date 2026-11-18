//! The instrument symbols a track can have: which one a track shows, and how each is drawn
//! with the painter (GuitarScore's own work, like the other icons).
//!
//! The symbol is a display choice (`gscore_model::InstrumentSymbol`); it does not change the
//! played sound. A track without a chosen symbol gets one derived from its GP7 instrument
//! type and General MIDI program.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};
use gscore_model::{InstrumentKind, InstrumentSymbol, TrackSound, TrackView};

use super::vector_icon::{self, Part};

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
    if let Some(parts) = outline(symbol) {
        vector_icon::paint(painter, rect, symbol.id(), parts, ink);
        return;
    }
    let line = 0.085;
    match symbol {
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
        // Drawn from their outlines above.
        InstrumentSymbol::ElectricGuitar
        | InstrumentSymbol::AcousticGuitar
        | InstrumentSymbol::Bass
        | InstrumentSymbol::Ukulele
        | InstrumentSymbol::Banjo
        | InstrumentSymbol::Drums
        | InstrumentSymbol::Other => {}
    }
}

/// The outlines of the symbols drawn as filled shapes (see [`vector_icon`]), in a 100 by
/// 100 box: the string instruments on the diagonal, head up to the right, each body with
/// its waist, soundhole and bridge. `None`: the symbol is drawn by [`paint`] itself.
fn outline(symbol: InstrumentSymbol) -> Option<&'static [Part]> {
    Some(match symbol {
        InstrumentSymbol::ElectricGuitar => &[
            Part::Fill(&[
                "M 46.8 47.5 C 44.7 45.4 44.7 40.8 45.5 36.3 C 46.3 31.9 43 27.8 38.9 28.2 C 34.8 28.7 32.4 33.8 31.1 37.8 C 29.9 41.8 27.5 43.3 23 44.1 C 15.3 45.4 8.6 48.4 4.7 55.1 C -0.3 63.7 6.4 76.3 15 84.2 C 24.6 93 37.3 99.7 45.3 93.4 C 51.7 88.5 51.1 81.2 52.6 75.1 C 53.8 70.3 57.3 68.6 60.9 66.8 C 64.5 65.1 65.7 61.1 63 58.4 C 60.7 56.1 57.9 57 55.4 56.8 C 53.3 56.6 51.7 55.4 50.3 54 Z",
            ]),
            Part::Fill(&["M 80.7 13.6 L 85.7 18.6 L 49.5 54.7 L 44.6 49.7 Z"]),
            Part::Fill(&[
                "M 80 14.3 C 82.5 10.9 83.9 7 87.4 4.5 C 89.7 3 94.8 6.3 96.6 10.4 C 97.9 13.5 95.8 15.1 93.1 15.4 C 90 16.1 87.5 17.3 84.9 19.3 Z",
            ]),
        ],
        InstrumentSymbol::AcousticGuitar => &[
            Part::Fill(&[
                "M 51.3 43.6 C 45.4 37.8 37.9 37.5 33 42.3 C 29 46.4 29.8 50.5 27 53.3 C 23.8 56.5 14.8 51.6 7.6 58.9 C -0.5 67 2.9 79.3 11.8 88.2 C 20.7 97.1 33 100.5 41.1 92.4 C 48.4 85.2 43.5 76.2 46.7 73 C 49.5 70.2 53.6 71 57.7 67 C 62.5 62.1 62.2 54.6 56.4 48.7 Z",
                "M 43.4 65.7 C 45.5 63.6 45.1 59.9 42.6 57.4 C 40.1 54.9 36.4 54.5 34.3 56.6 C 32.3 58.6 32.6 62.3 35.2 64.8 C 37.7 67.4 41.4 67.7 43.4 65.7 Z",
                "M 17.7 71.5 L 15.6 73.6 L 26.4 84.4 L 28.5 82.3 Z",
            ]),
            Part::Fill(&["M 81.2 13.7 L 86.3 18.8 L 55.6 49.5 L 50.5 44.4 Z"]),
            Part::Fill(&["M 79.8 13.9 L 87.6 3.9 L 89.9 3 L 97 10.1 L 96.1 12.4 L 86.1 20.2 Z"]),
        ],
        InstrumentSymbol::Bass => &[
            Part::Fill(&[
                "M 39.1 56.7 C 37.1 54.7 38.3 49.9 40.2 44.3 C 41.7 39.1 38.4 35.1 34.8 36 C 31.2 36.9 28.4 41.5 26.8 45.9 C 25.3 49.6 23.2 51.2 19.3 52.5 C 12.1 54.2 7 57.4 4.3 62.9 C 0.5 70.3 5.4 80 12.9 87.1 C 21.3 95 31.8 100 39 94.6 C 44.5 90.4 44.1 84 45.6 78.9 C 46.9 74.9 49.3 73.6 52.3 72.1 C 55.2 70.6 56 67.6 53.8 65.4 C 51.8 63.4 49.3 64.1 46.9 63.9 C 45.3 63.8 43.8 62.9 42.5 61.6 Z",
            ]),
            Part::Fill(&["M 79.3 16.4 L 83.5 20.6 L 41.8 62.3 L 37.6 58.2 Z"]),
            Part::Fill(&[
                "M 78.6 17.2 C 81.1 13.7 82.5 9.8 86 7.8 C 88.3 6.6 92.7 9.8 94 13.2 C 95 15.9 92.8 17.3 90.4 17.9 C 87.5 18.6 84.9 19.7 82.8 21.3 Z",
            ]),
            Part::Fill(&[
                "M 84.7 7.1 C 83.9 7.9 82.5 7.7 81.5 6.8 C 80.6 5.8 80.4 4.4 81.2 3.6 C 82 2.9 83.4 3 84.3 4 C 85.3 4.9 85.4 6.3 84.7 7.1 Z",
            ]),
            Part::Fill(&[
                "M 81.4 10.4 C 80.6 11.2 79.2 11 78.2 10.1 C 77.3 9.1 77.1 7.7 77.9 6.9 C 78.7 6.1 80.1 6.3 81.1 7.2 C 82 8.2 82.2 9.6 81.4 10.4 Z",
            ]),
            Part::Fill(&[
                "M 96.5 20.7 C 95.7 21.5 94.3 21.4 93.3 20.4 C 92.4 19.5 92.3 18.1 93 17.3 C 93.8 16.5 95.2 16.6 96.2 17.6 C 97.1 18.5 97.3 19.9 96.5 20.7 Z",
            ]),
            Part::Fill(&[
                "M 93.2 24 C 92.4 24.8 91 24.7 90.1 23.7 C 89.1 22.7 89 21.3 89.8 20.6 C 90.5 19.8 91.9 19.9 92.9 20.9 C 93.9 21.8 94 23.2 93.2 24 Z",
            ]),
        ],
        InstrumentSymbol::Ukulele => &[
            Part::Fill(&[
                "M 56.6 38.5 C 51.2 33.1 43.7 33.1 38.6 38.2 C 34.5 42.3 35.1 45.9 32.1 48.8 C 28.8 52.1 20.8 48.6 13.4 56 C 5.6 63.7 9 75.3 16.9 83.1 C 24.7 91 36.3 94.4 44 86.6 C 51.4 79.2 47.9 71.2 51.2 67.9 C 54.1 64.9 57.7 65.5 61.8 61.4 C 66.9 56.3 66.9 48.8 61.5 43.4 Z",
                "M 48.6 60.1 C 50.7 57.9 50.5 54.3 48.1 51.9 C 45.7 49.5 42.1 49.3 39.9 51.4 C 37.8 53.6 38 57.2 40.4 59.6 C 42.8 62 46.4 62.2 48.6 60.1 Z",
                "M 22.3 67.8 L 20 70 L 30 80 L 32.2 77.7 Z",
            ]),
            Part::Fill(&["M 75 20 L 80 25 L 60.8 44.2 L 55.8 39.2 Z"]),
            Part::Fill(&["M 73.8 20.2 L 81.4 10 L 84 9 L 91 16 L 90 18.6 L 79.8 26.2 Z"]),
        ],
        InstrumentSymbol::Banjo => &[
            Part::Fill(&[
                "M 46.5 90.5 C 37.2 99.8 21.4 99.1 11.2 88.8 C 0.9 78.6 0.2 62.8 9.5 53.5 C 18.8 44.3 34.6 45 44.8 55.2 C 55 65.4 55.7 81.2 46.5 90.5 Z",
                "M 41.4 85.4 C 48.1 78.7 47.6 67.3 40.1 59.9 C 32.7 52.4 21.3 51.9 14.6 58.6 C 7.8 65.4 8.4 76.8 15.8 84.2 C 23.2 91.6 34.6 92.2 41.4 85.4 Z",
            ]),
            Part::Fill(&["M 82.9 12.7 L 87.3 17.1 L 45.3 59.1 L 40.9 54.7 Z"]),
            Part::Fill(&[
                "M 82.1 13.5 C 84.2 10.6 84.3 6.5 88.2 3.7 C 90.8 1.8 92.5 4.5 95.5 4.5 C 95.5 7.5 98.2 9.2 96.3 11.8 C 93.5 15.7 89.4 15.8 86.5 17.9 Z",
            ]),
            Part::Fill(&[
                "M 59.5 36.3 C 58.6 37.2 57.1 37.1 56.1 36.1 C 55.1 35.2 55.1 33.7 55.9 32.8 C 56.8 31.9 58.3 32 59.3 32.9 C 60.3 33.9 60.3 35.4 59.5 36.3 Z",
            ]),
            Part::Fill(&["M 19.1 71.6 L 28.4 80.9 L 26.4 82.9 L 17.1 73.6 Z"]),
            Part::Fill(&["M 14.6 80.6 L 19.4 85.4 L 13.4 90.3 L 9.7 86.6 Z"]),
        ],
        InstrumentSymbol::Drums => &[
            Part::Fill(&[
                "M 9.5 54 C 9.5 46.5 27.6 40.5 50 40.5 C 72.4 40.5 90.5 46.5 90.5 54 L 90.5 83.5 C 90.5 90.9 72.4 97 50 97 C 27.6 97 9.5 90.9 9.5 83.5 Z",
                "M 83.8 54 C 83.8 49.2 68.7 45.4 50 45.4 C 31.3 45.4 16.2 49.2 16.2 54 C 16.2 58.7 31.3 62.6 50 62.6 C 68.7 62.6 83.8 58.7 83.8 54 Z",
            ]),
            Part::Line("M 14.4 6.1 L 56.1 47.8", 6.1),
            Part::Line("M 85.6 6.1 L 43.9 47.8", 6.1),
        ],
        InstrumentSymbol::Other => &[
            Part::Fill(&[
                "M 34.7 80.4 C 36.8 85.7 32.5 92.5 25 95.6 C 17.5 98.6 9.6 96.7 7.5 91.4 C 5.3 86 9.6 79.2 17.1 76.2 C 24.7 73.1 32.5 75 34.7 80.4 Z",
            ]),
            Part::Fill(&[
                "M 92 70.2 C 94.2 75.5 89.8 82.3 82.3 85.4 C 74.8 88.4 67 86.5 64.8 81.2 C 62.7 75.8 67 69 74.5 66 C 82 62.9 89.9 64.8 92 70.2 Z",
            ]),
            Part::Fill(&["M 30.6 24.7 L 35.7 23.4 L 35.7 84.6 L 30.6 85.9 Z"]),
            Part::Fill(&["M 88 14.5 L 93.1 13.2 L 93.1 74.4 L 88 75.7 Z"]),
            Part::Fill(&["M 30.6 14.5 L 93.1 3 L 93.1 20.8 L 30.6 32.3 Z"]),
        ],
        _ => return None,
    })
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
        let _gpu = crate::test_support::lock_gpu();
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
