//! The tempo dialog's picture: a mechanical metronome in one color, its pendulum swinging
//! at the chosen tempo (one swing per beat), and the Italian tempo names.

use egui::{Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, vec2};

/// Italian tempo names with a typical tempo for each (beats per minute), slow to fast.
pub(super) const TEMPO_NAMES: [(&str, u32); 8] = [
    ("Largo", 50),
    ("Adagio", 70),
    ("Andante", 92),
    ("Moderato", 112),
    ("Allegro", 132),
    ("Vivace", 160),
    ("Presto", 184),
    ("Prestissimo", 208),
];

/// The tempo name whose range holds `tempo`: each name reaches halfway to the next.
pub(super) fn tempo_name(tempo: u32) -> &'static str {
    let mut chosen = TEMPO_NAMES[0].0;
    for pair in TEMPO_NAMES.windows(2) {
        let [(_, low), (name, high)] = pair else {
            continue;
        };
        if tempo >= (low + high) / 2 {
            chosen = name;
        }
    }
    chosen
}

/// Records a tap-tempo click at `time` (seconds) in `taps` and returns the tempo of
/// the last few clicks, once there are two. A pause of two seconds starts over.
pub(super) fn tap_tempo(taps: &mut Vec<f64>, time: f64) -> Option<u32> {
    const KEEP: usize = 5;
    if taps.last().is_some_and(|&last| time - last > 2.0) {
        taps.clear();
    }
    taps.push(time);
    if taps.len() > KEEP {
        taps.remove(0);
    }
    let (first, last) = (taps.first()?, taps.last()?);
    let beats = taps.len().checked_sub(1).filter(|&n| n > 0)? as f64;
    let seconds = (last - first) / beats;
    (seconds > 0.0).then(|| (60.0 / seconds).round() as u32)
}

/// The pendulum's angle (radians, 0 = upright) at `time` seconds: it swings from side
/// to side once per beat, so a tick falls on each end of the swing.
fn pendulum_angle(tempo: u32, time: f64) -> f32 {
    const AMPLITUDE: f64 = 0.42;
    let beats = time * f64::from(tempo.max(1)) / 60.0;
    // A full period is two beats (left to right and back).
    (AMPLITUDE * (beats * std::f64::consts::PI).sin()) as f32
}

/// Draws the metronome into a `size` box. `running` holds when its pendulum was started
/// (seconds), or `None` while it rests; a click starts or stops it.
pub(super) fn metronome(
    ui: &mut egui::Ui,
    tempo: u32,
    size: egui::Vec2,
    running: &mut Option<f64>,
) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let label = if running.is_some() {
        "Stop the metronome"
    } else {
        "Start the metronome"
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let now = ui.input(|input| input.time);
    if response.clicked() {
        *running = match running {
            Some(_) => None,
            None => Some(now),
        };
    }
    let tokens = super::style::tokens(ui);
    let ink = if response.hovered() {
        tokens.text
    } else {
        tokens.text.gamma_multiply(0.92)
    };
    // At rest the pendulum leans to one side, as a wound-up metronome waits.
    let angle = match *running {
        Some(start) => {
            ui.ctx().request_repaint();
            pendulum_angle(tempo, now - start)
        }
        None => pendulum_angle(tempo, 30.0 / f64::from(tempo.max(1))),
    };
    paint(ui.painter(), rect, angle, tempo, ink);
    response.on_hover_text(if running.is_some() {
        "Click to stop the pendulum"
    } else {
        "Click to see the tempo swing"
    });
}

/// The metronome: a tall trapezoid body with a scale, the pendulum rod and its sliding
/// weight (higher for slower tempos, as on the real instrument), and the base.
fn paint(painter: &Painter, rect: Rect, angle: f32, tempo: u32, ink: Color32) {
    let w = rect.width();
    let h = rect.height();
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x * w, rect.top() + y * h);
    let body = vec![
        at(0.36, 0.06),
        at(0.64, 0.06),
        at(0.86, 0.88),
        at(0.14, 0.88),
    ];
    let stroke = Stroke::new((w * 0.035).max(1.5), ink);
    painter.add(Shape::closed_line(body.clone(), stroke));
    // The base.
    let base = Rect::from_min_max(at(0.08, 0.88), at(0.92, 0.96));
    painter.rect_filled(base, 3.0, ink);
    // The scale behind the rod.
    let scale_top = at(0.5, 0.16);
    let scale_bottom = at(0.5, 0.66);
    for step in 0..=10 {
        let t = step as f32 / 10.0;
        let y = scale_top.y + (scale_bottom.y - scale_top.y) * t;
        let half = if step % 5 == 0 { 0.07 } else { 0.04 } * w;
        painter.line_segment(
            [
                Pos2::new(rect.center().x - half - w * 0.09, y),
                Pos2::new(rect.center().x - w * 0.09, y),
            ],
            Stroke::new(1.0, ink.gamma_multiply(0.6)),
        );
        painter.line_segment(
            [
                Pos2::new(rect.center().x + w * 0.09, y),
                Pos2::new(rect.center().x + w * 0.09 + half, y),
            ],
            Stroke::new(1.0, ink.gamma_multiply(0.6)),
        );
    }
    // The pivot and the rod, swinging about it.
    let pivot = at(0.5, 0.78);
    let length = h * 0.70;
    let direction = vec2(angle.sin(), -angle.cos());
    let tip = pivot + direction * length;
    painter.line_segment([pivot, tip], Stroke::new((w * 0.03).max(1.5), ink));
    // The weight: high on the rod for slow tempos, low for fast ones.
    let share = 0.25 + 0.6 * (1.0 - (tempo.clamp(40, 240) - 40) as f32 / 200.0);
    let weight = pivot + direction * length * share;
    let normal = vec2(direction.y, -direction.x);
    let half_w = w * 0.09;
    let half_h = h * 0.035;
    painter.add(Shape::convex_polygon(
        vec![
            weight + normal * half_w - direction * half_h,
            weight - normal * half_w - direction * half_h,
            weight - normal * half_w * 0.7 + direction * half_h,
            weight + normal * half_w * 0.7 + direction * half_h,
        ],
        ink,
        Stroke::NONE,
    ));
    painter.circle_filled(pivot, w * 0.04, ink);
    // The winding key at the side.
    let key = at(0.86, 0.6);
    painter.circle_stroke(key, w * 0.04, Stroke::new(1.5, ink));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapping_finds_the_tempo_and_a_pause_starts_over() {
        let mut taps = Vec::new();
        assert_eq!(tap_tempo(&mut taps, 10.0), None);
        assert_eq!(tap_tempo(&mut taps, 10.5), Some(120));
        assert_eq!(tap_tempo(&mut taps, 11.0), Some(120));
        // Three seconds later: a new count.
        assert_eq!(tap_tempo(&mut taps, 14.0), None);
        assert_eq!(tap_tempo(&mut taps, 15.0), Some(60));
    }

    #[test]
    fn tempo_names_cover_their_ranges() {
        assert_eq!(tempo_name(40), "Largo");
        assert_eq!(tempo_name(60), "Adagio");
        assert_eq!(tempo_name(120), "Moderato");
        assert_eq!(tempo_name(125), "Allegro");
        assert_eq!(tempo_name(300), "Prestissimo");
    }

    #[test]
    fn the_pendulum_reaches_an_end_on_each_beat() {
        // At 60 BPM a beat lasts a second: upright at the start, at an end after half a
        // beat, upright again after one.
        assert!(pendulum_angle(60, 0.0).abs() < 1e-6);
        assert!(pendulum_angle(60, 0.5) > 0.4);
        assert!(pendulum_angle(60, 1.0).abs() < 1e-3);
    }
}
