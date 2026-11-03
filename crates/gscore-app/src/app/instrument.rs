//! The instrument a track is played on, as a family with its own small icon (guitar, bass,
//! drums, keyboard, voice, anything else), drawn with the painter like the other icons.

use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};
use gscore_model::{InstrumentKind, TrackSound, TrackView};

/// An instrument family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Instrument {
    Guitar,
    Bass,
    Drums,
    Keyboard,
    Voice,
    Other,
}

impl Instrument {
    /// The family of `track`: a drum kit, else the General MIDI program's family (the
    /// sound the panel chooses), else what the GP7 instrument name says, else a string
    /// instrument counts as a guitar.
    pub(crate) fn of(track: &TrackView<'_>, sound: Option<TrackSound>) -> Self {
        if track.is_percussion() || track.instrument_kind() == InstrumentKind::Drums {
            return Instrument::Drums;
        }
        match sound.map(|sound| sound.program) {
            Some(0..=7 | 16..=23) => return Instrument::Keyboard,
            Some(24..=31) => return Instrument::Guitar,
            Some(32..=39) => return Instrument::Bass,
            // The string family (violin, cello, ...) is the generic "instrument" symbol.
            Some(40..=51) => return Instrument::Other,
            Some(52..=54 | 85) => return Instrument::Voice,
            _ => {}
        }
        let name = track
            .instrument_name()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let named = [
            ("bass", Instrument::Bass),
            ("guitar", Instrument::Guitar),
            ("piano", Instrument::Keyboard),
            ("keyboard", Instrument::Keyboard),
            ("organ", Instrument::Keyboard),
            ("synth", Instrument::Keyboard),
            ("voice", Instrument::Voice),
            ("vocal", Instrument::Voice),
            ("singer", Instrument::Voice),
            ("choir", Instrument::Voice),
        ];
        if let Some((_, family)) = named.iter().find(|(word, _)| name.contains(word)) {
            return *family;
        }
        if track.instrument_kind() == InstrumentKind::Strings {
            Instrument::Guitar
        } else {
            Instrument::Other
        }
    }

    /// What screen readers and tooltips call it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Instrument::Guitar => "Guitar",
            Instrument::Bass => "Bass",
            Instrument::Drums => "Drums",
            Instrument::Keyboard => "Keyboard",
            Instrument::Voice => "Voice",
            Instrument::Other => "Instrument",
        }
    }

    /// Draws the icon inside `rect` in `ink`.
    pub(crate) fn paint(self, painter: &Painter, rect: Rect, ink: Color32) {
        let c = rect.center();
        let s = rect.width().min(rect.height());
        let stroke = Stroke::new((s / 10.0).max(1.2), ink);
        match self {
            Instrument::Guitar | Instrument::Bass => {
                // A body of two lobes, a neck and a head, leaning right.
                let bass = self == Instrument::Bass;
                let body = c + vec2(-s * 0.16, s * 0.18);
                let lower = s * if bass { 0.2 } else { 0.24 };
                let upper = lower * 0.78;
                painter.circle_filled(body, lower, ink);
                painter.circle_filled(body + vec2(s * 0.14, -s * 0.15), upper, ink);
                let neck_start = body + vec2(s * 0.18, -s * 0.2);
                let head = c + vec2(s * 0.44, -s * if bass { 0.48 } else { 0.42 });
                painter.line_segment([neck_start, head], Stroke::new(s * 0.09, ink));
                painter.circle_filled(head, s * 0.07, ink);
            }
            Instrument::Drums => {
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
                    painter.line_segment(
                        [
                            pos2(c.x + side * s * 0.05, top - s * 0.04),
                            pos2(c.x + side * s * 0.38, c.y - s * 0.44),
                        ],
                        stroke,
                    );
                }
            }
            Instrument::Keyboard => {
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
            Instrument::Voice => {
                // A microphone: a rounded head, a stand and a foot.
                let head =
                    Rect::from_center_size(c + vec2(0.0, -s * 0.18), vec2(s * 0.34, s * 0.5));
                painter.rect_filled(head, s * 0.17, ink);
                let arc: Vec<_> = (0..=10)
                    .map(|step| {
                        let t = std::f32::consts::PI * step as f32 / 10.0;
                        c + vec2(-t.cos() * s * 0.3, s * 0.02 + t.sin() * s * 0.2)
                    })
                    .collect();
                painter.add(Shape::line(arc, stroke));
                painter.line_segment([c + vec2(0.0, s * 0.22), c + vec2(0.0, s * 0.42)], stroke);
                painter.line_segment(
                    [c + vec2(-s * 0.18, s * 0.42), c + vec2(s * 0.18, s * 0.42)],
                    stroke,
                );
            }
            Instrument::Other => {
                // An eighth note.
                let head = c + vec2(-s * 0.12, s * 0.26);
                painter.add(Shape::ellipse_filled(head, vec2(s * 0.17, s * 0.12), ink));
                let stem_x = head.x + s * 0.15;
                painter.line_segment([pos2(stem_x, head.y), pos2(stem_x, c.y - s * 0.42)], stroke);
                painter.line_segment(
                    [
                        pos2(stem_x, c.y - s * 0.42),
                        pos2(stem_x + s * 0.24, c.y - s * 0.12),
                    ],
                    stroke,
                );
            }
        }
    }
}
