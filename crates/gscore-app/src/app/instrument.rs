//! The instrument symbols a track can have: which one a track shows, and how each is drawn
//! with the painter (GuitarScore's own work, like the other icons).
//!
//! The symbol is a display choice (`gscore_model::InstrumentSymbol`); it does not change the
//! played sound. A track without a chosen symbol gets one derived from its GP7 instrument
//! type and General MIDI program.

use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};
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
            24..=31 => return InstrumentSymbol::ElectricGuitar,
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

/// Draws `symbol` inside `rect` in `ink`.
pub(crate) fn paint(painter: &Painter, rect: Rect, symbol: InstrumentSymbol, ink: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new((s / 11.0).max(1.2), ink);
    let at = |dx: f32, dy: f32| pos2(c.x + s * dx, c.y + s * dy);
    let line = |from: egui::Pos2, to: egui::Pos2| painter.line_segment([from, to], stroke);
    match symbol {
        InstrumentSymbol::ElectricGuitar => {
            // A solid double-cutaway body, a neck and a headstock.
            let body = c + vec2(-s * 0.16, s * 0.18);
            painter.circle_filled(body, s * 0.24, ink);
            painter.circle_filled(body + vec2(s * 0.14, -s * 0.15), s * 0.19, ink);
            line(body + vec2(s * 0.18, -s * 0.2), at(0.44, -0.42));
            painter.circle_filled(at(0.44, -0.42), s * 0.07, ink);
        }
        InstrumentSymbol::AcousticGuitar => {
            // The same body, drawn as an outline with a soundhole.
            let body = c + vec2(-s * 0.16, s * 0.18);
            painter.circle_stroke(body, s * 0.24, stroke);
            painter.circle_stroke(body + vec2(s * 0.14, -s * 0.15), s * 0.19, stroke);
            painter.circle_filled(body + vec2(s * 0.05, -s * 0.02), s * 0.07, ink);
            line(body + vec2(s * 0.18, -s * 0.2), at(0.44, -0.42));
            painter.circle_filled(at(0.44, -0.42), s * 0.07, ink);
        }
        InstrumentSymbol::Bass => {
            // A longer neck and a thicker body than the guitar.
            let body = c + vec2(-s * 0.1, s * 0.22);
            painter.circle_filled(body, s * 0.28, ink);
            painter.circle_filled(body + vec2(s * 0.16, -s * 0.17), s * 0.2, ink);
            line(body + vec2(s * 0.2, -s * 0.24), at(0.46, -0.5));
            painter.circle_filled(at(0.46, -0.5), s * 0.08, ink);
        }
        InstrumentSymbol::Drums => {
            // A drum seen from the side, and two sticks.
            let top = c.y - s * 0.02;
            let width = s * 0.72;
            let shell = Rect::from_min_max(
                pos2(c.x - width / 2.0, top),
                pos2(c.x + width / 2.0, c.y + s * 0.34),
            );
            painter.rect_filled(shell, s * 0.06, ink);
            painter.add(Shape::ellipse_stroke(
                pos2(c.x, top),
                vec2(width / 2.0, s * 0.1),
                stroke,
            ));
            for side in [-1.0, 1.0] {
                line(
                    pos2(c.x + side * s * 0.05, top - s * 0.04),
                    pos2(c.x + side * s * 0.38, c.y - s * 0.44),
                );
            }
        }
        InstrumentSymbol::Piano => {
            let keys = Rect::from_center_size(c, vec2(s * 0.9, s * 0.62));
            painter.rect_stroke(keys, s * 0.06, stroke, egui::StrokeKind::Inside);
            for index in 1..4 {
                let x = keys.left() + keys.width() * index as f32 / 4.0;
                painter.rect_filled(
                    Rect::from_center_size(
                        pos2(x, keys.top() + keys.height() * 0.3),
                        vec2(s * 0.1, keys.height() * 0.6),
                    ),
                    0.0,
                    ink,
                );
            }
        }
        InstrumentSymbol::Organ => {
            // Three pipes over a keyboard.
            let keys = Rect::from_center_size(c + vec2(0.0, s * 0.2), vec2(s * 0.9, s * 0.38));
            painter.rect_stroke(keys, s * 0.06, stroke, egui::StrokeKind::Inside);
            for index in 1..4 {
                let x = keys.left() + keys.width() * index as f32 / 4.0;
                painter.rect_filled(
                    Rect::from_center_size(
                        pos2(x, keys.top() + keys.height() * 0.3),
                        vec2(s * 0.08, keys.height() * 0.6),
                    ),
                    0.0,
                    ink,
                );
            }
            for (index, height) in [0.34_f32, 0.44, 0.34].into_iter().enumerate() {
                let x = c.x + (index as f32 - 1.0) * s * 0.26;
                let pipe = Rect::from_min_max(
                    pos2(x - s * 0.08, keys.top() - s * height),
                    pos2(x + s * 0.08, keys.top() - s * 0.06),
                );
                painter.rect_filled(pipe, s * 0.04, ink);
            }
        }
        InstrumentSymbol::Synth => {
            // A keyboard with a wave above it.
            let keys = Rect::from_center_size(c + vec2(0.0, s * 0.2), vec2(s * 0.9, s * 0.42));
            painter.rect_stroke(keys, s * 0.06, stroke, egui::StrokeKind::Inside);
            for index in 1..4 {
                let x = keys.left() + keys.width() * index as f32 / 4.0;
                painter.rect_filled(
                    Rect::from_center_size(
                        pos2(x, keys.top() + keys.height() * 0.3),
                        vec2(s * 0.08, keys.height() * 0.6),
                    ),
                    0.0,
                    ink,
                );
            }
            let wave: Vec<_> = (0..=12)
                .map(|step| {
                    let t = step as f32 / 12.0;
                    pos2(
                        keys.left() + keys.width() * t,
                        keys.top() - s * 0.16 - (t * std::f32::consts::TAU).sin() * s * 0.12,
                    )
                })
                .collect();
            painter.add(Shape::line(wave, stroke));
        }
        InstrumentSymbol::Voice => {
            // A microphone: a rounded head, a stand and a foot.
            let head = Rect::from_center_size(c + vec2(0.0, -s * 0.18), vec2(s * 0.34, s * 0.5));
            painter.rect_filled(head, s * 0.17, ink);
            let arc: Vec<_> = (0..=10)
                .map(|step| {
                    let t = std::f32::consts::PI * step as f32 / 10.0;
                    c + vec2(-t.cos() * s * 0.3, s * 0.02 + t.sin() * s * 0.2)
                })
                .collect();
            painter.add(Shape::line(arc, stroke));
            line(c + vec2(0.0, s * 0.22), c + vec2(0.0, s * 0.42));
            line(c + vec2(-s * 0.18, s * 0.42), c + vec2(s * 0.18, s * 0.42));
        }
        InstrumentSymbol::Violin => {
            // A waisted body with two f-holes and a bow across it.
            let body = [
                (-0.1, -0.18),
                (-0.28, 0.02),
                (-0.2, 0.32),
                (0.02, 0.36),
                (0.18, 0.12),
            ];
            let outline: Vec<_> = body.iter().map(|&(x, y)| at(x, y)).collect();
            painter.add(Shape::closed_line(outline, stroke));
            for side in [-1.0, 1.0] {
                line(at(side * 0.05, -0.06), at(side * 0.09, 0.18));
            }
            line(at(-0.3, 0.36), at(0.34, -0.42));
            line(at(-0.34, 0.28), at(0.3, -0.5));
        }
        InstrumentSymbol::Trumpet => {
            // A tube with a bell and three valves.
            line(at(-0.34, 0.08), at(0.16, 0.08));
            line(at(0.16, 0.08), at(0.4, -0.16));
            line(at(0.16, 0.08), at(0.4, 0.24));
            line(at(0.4, -0.22), at(0.4, 0.3));
            for step in 0..3 {
                let x = -0.2 + step as f32 * 0.12;
                line(at(x, 0.08), at(x, -0.1));
            }
            line(at(-0.34, -0.06), at(-0.34, 0.22));
        }
        InstrumentSymbol::Saxophone => {
            // A curved tube with a bell and keys.
            let curve: Vec<_> = (0..=10)
                .map(|step| {
                    let t = step as f32 / 10.0;
                    at(-0.24 + t * 0.02, -0.34 + t * 0.5)
                })
                .collect();
            painter.add(Shape::line(curve, Stroke::new(s * 0.09, ink)));
            painter.add(Shape::line(
                (0..=8)
                    .map(|step| {
                        let t = step as f32 / 8.0;
                        at(-0.22 + t * 0.45, 0.16 + t * 0.2)
                    })
                    .collect::<Vec<_>>(),
                Stroke::new(s * 0.09, ink),
            ));
            painter.circle_stroke(at(-0.2, -0.34), s * 0.08, stroke);
        }
        InstrumentSymbol::Flute => {
            // A long tube with finger holes and a mouthpiece, lying diagonally.
            line(at(-0.42, 0.22), at(0.36, -0.26));
            line(at(-0.42, 0.36), at(0.36, -0.12));
            for step in 0..4 {
                let t = 0.2 + step as f32 * 0.16;
                painter.circle_filled(at(-0.42 + t * 0.78, 0.22 - t * 0.48), s * 0.035, ink);
            }
        }
        InstrumentSymbol::Banjo => {
            // A round head, a neck and a headstock.
            painter.circle_stroke(c + vec2(-s * 0.12, s * 0.2), s * 0.24, stroke);
            painter.circle_filled(c + vec2(-s * 0.12, s * 0.2), s * 0.06, ink);
            line(c + vec2(s * 0.02, s * 0.08), at(0.46, -0.44));
            painter.circle_filled(at(0.46, -0.44), s * 0.07, ink);
        }
        InstrumentSymbol::Ukulele => {
            // A small body with a short neck.
            let body = c + vec2(-s * 0.08, s * 0.18);
            painter.circle_filled(body, s * 0.22, ink);
            painter.circle_filled(body + vec2(s * 0.1, -s * 0.12), s * 0.16, ink);
            line(body + vec2(s * 0.14, -s * 0.16), at(0.36, -0.3));
            painter.circle_filled(at(0.36, -0.3), s * 0.06, ink);
        }
        InstrumentSymbol::Other => {
            // An eighth note.
            let head = c + vec2(-s * 0.12, s * 0.26);
            painter.add(Shape::ellipse_filled(head, vec2(s * 0.17, s * 0.12), ink));
            let stem_x = head.x + s * 0.15;
            line(pos2(stem_x, head.y), pos2(stem_x, c.y - s * 0.42));
            line(
                pos2(stem_x, c.y - s * 0.42),
                pos2(stem_x + s * 0.24, c.y - s * 0.12),
            );
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
        let view = song.track(track).unwrap();
        assert_eq!(
            derived(&view, song.track_sound(track)),
            InstrumentSymbol::ElectricGuitar
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
}
