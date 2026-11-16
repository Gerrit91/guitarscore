//! The Jumps and codas dialog: a bar's navigation signs as engraved tiles in three groups,
//! as a score uses them. The places the music returns to (segno, coda, fine) can be
//! combined; one jump back (D.C., D.S., D.S.S., each plain, al Fine, al Coda or al Double
//! Coda) and one "to coda" per bar. A line below explains the sign under the mouse, and a
//! warning says when a chosen jump has nowhere to go in the song.

use egui::{Align2, Color32, FontFamily, FontId, Painter, Rect, RichText, Sense, Stroke, vec2};
use gscore_model::Direction;

use super::edit_staff::StaffState;
use super::style;
use super::tile_faces::music_glyph;
use crate::fonts::BOLD;

/// Height of a sign tile.
const TILE_HEIGHT: f32 = 58.0;
/// Space between tiles.
const GAP: f32 = 6.0;
/// The engraved signs' font size.
const SIGN_SIZE: f32 = 30.0;

/// The places the music returns to or stops at.
const TARGETS: [Direction; 5] = [
    Direction::Segno,
    Direction::SegnoSegno,
    Direction::Coda,
    Direction::DoubleCoda,
    Direction::Fine,
];

/// The jumps back, one row per return point, one column per way of ending.
const JUMPS: [[Direction; 4]; 3] = [
    [
        Direction::DaCapo,
        Direction::DaCapoAlFine,
        Direction::DaCapoAlCoda,
        Direction::DaCapoAlDoubleCoda,
    ],
    [
        Direction::DaSegno,
        Direction::DaSegnoAlFine,
        Direction::DaSegnoAlCoda,
        Direction::DaSegnoAlDoubleCoda,
    ],
    [
        Direction::DaSegnoSegno,
        Direction::DaSegnoSegnoAlFine,
        Direction::DaSegnoSegnoAlCoda,
        Direction::DaSegnoSegnoAlDoubleCoda,
    ],
];

/// The jumps forward to a coda.
const TO_CODA: [Direction; 2] = [Direction::DaCoda, Direction::DaDoubleCoda];

/// The group a sign belongs to; one sign per bar from each but the targets.
fn exclusive_group(direction: Direction) -> &'static [Direction] {
    if JUMPS.iter().flatten().any(|&jump| jump == direction) {
        JUMPS.as_flattened()
    } else if TO_CODA.contains(&direction) {
        &TO_CODA
    } else {
        &[]
    }
}

/// Switches `direction` in `chosen`: off when it is on, else on, replacing the other sign
/// of its group (a bar jumps back once and leaves for one coda).
pub(super) fn toggle(chosen: &mut Vec<Direction>, direction: Direction) {
    if chosen.contains(&direction) {
        chosen.retain(|&other| other != direction);
        return;
    }
    let group = exclusive_group(direction);
    chosen.retain(|other| !group.contains(other));
    chosen.push(direction);
}

/// What a sign does, for the line under the tiles.
pub(super) fn explanation(direction: Direction) -> &'static str {
    match direction {
        Direction::Segno => "The music returns here from a D.S. (dal segno).",
        Direction::SegnoSegno => "A second return point: a D.S.S. comes back here.",
        Direction::Coda => "Where the ending starts: a \"To Coda\" jumps here on the way back.",
        Direction::DoubleCoda => "A second ending, for \"To Double Coda\".",
        Direction::Fine => "The end of the piece: after a D.C. or D.S. al Fine, it stops here.",
        Direction::DaCapo => "Da capo: play again from the beginning.",
        Direction::DaCapoAlFine => "Back to the beginning, and end at Fine.",
        Direction::DaCapoAlCoda => "Back to the beginning; at \"To Coda\", jump to the coda.",
        Direction::DaCapoAlDoubleCoda => {
            "Back to the beginning; at \"To Double Coda\", jump to the double coda."
        }
        Direction::DaSegno => "Dal segno: play again from the segno.",
        Direction::DaSegnoAlFine => "Back to the segno, and end at Fine.",
        Direction::DaSegnoAlCoda => "Back to the segno; at \"To Coda\", jump to the coda.",
        Direction::DaSegnoAlDoubleCoda => {
            "Back to the segno; at \"To Double Coda\", jump to the double coda."
        }
        Direction::DaSegnoSegno => "Play again from the second segno.",
        Direction::DaSegnoSegnoAlFine => "Back to the second segno, and end at Fine.",
        Direction::DaSegnoSegnoAlCoda => {
            "Back to the second segno; at \"To Coda\", jump to the coda."
        }
        Direction::DaSegnoSegnoAlDoubleCoda => {
            "Back to the second segno; at \"To Double Coda\", jump to the double coda."
        }
        Direction::DaCoda => {
            "On the way back (D.C. or D.S. al Coda), leave here for the coda; the first time \
             through, play on."
        }
        Direction::DaDoubleCoda => {
            "On the way back (al Double Coda), leave here for the double coda; the first time \
             through, play on."
        }
    }
}

/// The signs a chosen sign needs somewhere in the song, each with what is missing.
fn needs(direction: Direction) -> &'static [(Direction, &'static str)] {
    const SEGNO: (Direction, &str) = (Direction::Segno, "a segno");
    const SEGNO_SEGNO: (Direction, &str) = (Direction::SegnoSegno, "a segno segno");
    const FINE: (Direction, &str) = (Direction::Fine, "a Fine");
    const CODA: (Direction, &str) = (Direction::Coda, "a coda");
    const TO_CODA_SIGN: (Direction, &str) = (Direction::DaCoda, "a \"To Coda\"");
    const DOUBLE_CODA: (Direction, &str) = (Direction::DoubleCoda, "a double coda");
    const TO_DOUBLE: (Direction, &str) = (Direction::DaDoubleCoda, "a \"To Double Coda\"");
    match direction {
        Direction::DaCapoAlFine => &[FINE],
        Direction::DaCapoAlCoda => &[TO_CODA_SIGN, CODA],
        Direction::DaCapoAlDoubleCoda => &[TO_DOUBLE, DOUBLE_CODA],
        Direction::DaSegno => &[SEGNO],
        Direction::DaSegnoAlFine => &[SEGNO, FINE],
        Direction::DaSegnoAlCoda => &[SEGNO, TO_CODA_SIGN, CODA],
        Direction::DaSegnoAlDoubleCoda => &[SEGNO, TO_DOUBLE, DOUBLE_CODA],
        Direction::DaSegnoSegno => &[SEGNO_SEGNO],
        Direction::DaSegnoSegnoAlFine => &[SEGNO_SEGNO, FINE],
        Direction::DaSegnoSegnoAlCoda => &[SEGNO_SEGNO, TO_CODA_SIGN, CODA],
        Direction::DaSegnoSegnoAlDoubleCoda => &[SEGNO_SEGNO, TO_DOUBLE, DOUBLE_CODA],
        Direction::DaCoda => &[CODA],
        Direction::DaDoubleCoda => &[DOUBLE_CODA],
        _ => &[],
    }
}

/// What the chosen signs miss in the song (`elsewhere`: the signs of the other bars),
/// as a sentence, or `None` when every jump has somewhere to go.
pub(super) fn missing(chosen: &[Direction], elsewhere: &[Direction]) -> Option<String> {
    let mut missing: Vec<&str> = Vec::new();
    for &direction in chosen {
        for &(sign, name) in needs(direction) {
            let present = elsewhere.contains(&sign) || chosen.contains(&sign);
            if !present && !missing.contains(&name) {
                missing.push(name);
            }
        }
    }
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "Not in the song yet: {}. Until then, playback goes on without this jump.",
        join(&missing)
    ))
}

/// "a, b and c".
fn join(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// How a tile shows its sign.
#[derive(Debug, Clone, Copy)]
enum Sign {
    /// Engraved music glyphs.
    Glyphs(&'static str),
    /// The score's lettering: a main line and a smaller second one ("D.S." over "al Coda").
    Text(&'static str, &'static str),
    /// Lettering followed by a small engraved sign ("To Coda 𝄌").
    TextWithGlyph(&'static str, &'static str),
}

/// A sign's face and its short name under it (empty when the face says it all).
fn face(direction: Direction) -> (Sign, &'static str) {
    match direction {
        Direction::Segno => (Sign::Glyphs("\u{E047}"), "Segno"),
        Direction::SegnoSegno => (Sign::Glyphs("\u{E047}\u{E047}"), "Segno segno"),
        Direction::Coda => (Sign::Glyphs("\u{E048}"), "Coda"),
        Direction::DoubleCoda => (Sign::Glyphs("\u{E049}"), "Double coda"),
        Direction::Fine => (Sign::Text("Fine", ""), "End here"),
        Direction::DaCapo => (Sign::Text("D.C.", ""), ""),
        Direction::DaCapoAlFine => (Sign::Text("D.C.", "al Fine"), ""),
        Direction::DaCapoAlCoda => (Sign::Text("D.C.", "al Coda"), ""),
        Direction::DaCapoAlDoubleCoda => (Sign::Text("D.C.", "al Double Coda"), ""),
        Direction::DaSegno => (Sign::Text("D.S.", ""), ""),
        Direction::DaSegnoAlFine => (Sign::Text("D.S.", "al Fine"), ""),
        Direction::DaSegnoAlCoda => (Sign::Text("D.S.", "al Coda"), ""),
        Direction::DaSegnoAlDoubleCoda => (Sign::Text("D.S.", "al Double Coda"), ""),
        Direction::DaSegnoSegno => (Sign::Text("D.S.S.", ""), ""),
        Direction::DaSegnoSegnoAlFine => (Sign::Text("D.S.S.", "al Fine"), ""),
        Direction::DaSegnoSegnoAlCoda => (Sign::Text("D.S.S.", "al Coda"), ""),
        Direction::DaSegnoSegnoAlDoubleCoda => (Sign::Text("D.S.S.", "al Double Coda"), ""),
        Direction::DaCoda => (Sign::TextWithGlyph("To Coda", "\u{E048}"), ""),
        Direction::DaDoubleCoda => (Sign::TextWithGlyph("To Double Coda", "\u{E049}"), ""),
    }
}

/// Paints a sign centered in `rect`.
fn paint_sign(painter: &Painter, rect: Rect, sign: Sign, ink: Color32) {
    let bold = |size: f32| FontId::new(size, FontFamily::Name(BOLD.into()));
    match sign {
        Sign::Glyphs(text) => {
            // Bravura's segno and coda stand on the baseline, about a font size tall.
            music_glyph(
                painter,
                rect.center() + vec2(0.0, SIGN_SIZE * 0.32),
                text,
                SIGN_SIZE,
                ink,
            );
        }
        Sign::Text(main, "") => {
            painter.text(rect.center(), Align2::CENTER_CENTER, main, bold(16.0), ink);
        }
        Sign::Text(main, second) => {
            painter.text(
                rect.center() - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                main,
                bold(16.0),
                ink,
            );
            painter.text(
                rect.center() + vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                second,
                bold(11.5),
                ink,
            );
        }
        Sign::TextWithGlyph(text, glyph) => {
            let galley = painter.layout_no_wrap(text.to_owned(), bold(14.0), ink);
            let glyph_width = 20.0;
            let width = galley.size().x + 4.0 + glyph_width;
            let left = rect.center().x - width / 2.0;
            painter.galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                ink,
            );
            music_glyph(
                painter,
                egui::pos2(
                    rect.center().x + width / 2.0 - glyph_width / 2.0,
                    rect.center().y + 6.0,
                ),
                glyph,
                20.0,
                ink,
            );
        }
    }
}

/// One sign as a tile: the engraved sign, its short name under it. Returns whether it was
/// clicked; sets `hovered` while the mouse is on it.
fn sign_tile(
    ui: &mut egui::Ui,
    width: f32,
    direction: Direction,
    selected: bool,
    hovered: &mut Option<Direction>,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, selected, direction.label())
    });
    let tokens = style::tokens(ui);
    let painter = ui.painter();
    let fill = if selected {
        tokens.accent.gamma_multiply(0.28)
    } else if response.hovered() {
        tokens.control_hover
    } else {
        tokens.control_hover.gamma_multiply(0.35)
    };
    painter.rect_filled(rect, 6.0, fill);
    if selected {
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.5, tokens.accent),
            egui::StrokeKind::Inside,
        );
    }
    let (sign, name) = face(direction);
    let sign_rect = if name.is_empty() {
        rect
    } else {
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - 16.0))
    };
    paint_sign(painter, sign_rect, sign, tokens.text);
    if !name.is_empty() {
        painter.text(
            egui::pos2(rect.center().x, rect.max.y - 9.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(11.0),
            tokens.text_weak,
        );
    }
    if response.hovered() {
        *hovered = Some(direction);
    }
    response.clicked()
}

/// A row of tiles sharing the available width.
fn row(
    ui: &mut egui::Ui,
    signs: &[Direction],
    state: &mut StaffState,
    hovered: &mut Option<Direction>,
) {
    let count = signs.len().max(1) as f32;
    let width = (ui.available_width() - GAP * (count - 1.0)) / count;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        for &direction in signs {
            let selected = state.directions.contains(&direction);
            if sign_tile(ui, width, direction, selected, hovered) {
                toggle(&mut state.directions, direction);
            }
        }
    });
}

/// A group's heading.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).strong());
    ui.add_space(2.0);
}

/// The dialog's fields.
pub(super) fn show(ui: &mut egui::Ui, state: &mut StaffState) {
    let mut hovered = None;
    ui.spacing_mut().item_spacing.y = GAP;
    heading(ui, "Return points");
    row(ui, &TARGETS, state, &mut hovered);
    heading(ui, "Jump back");
    for jumps in &JUMPS {
        row(ui, jumps, state, &mut hovered);
    }
    heading(ui, "Leave for the coda");
    row(ui, &TO_CODA, state, &mut hovered);
    ui.add_space(4.0);
    // What the sign under the mouse does, else what the bar does now.
    let tokens = style::tokens(ui);
    let shown = hovered.or_else(|| state.directions.last().copied());
    let text = shown.map_or(
        "Click the signs this bar carries; click again to take one away.",
        explanation,
    );
    let title = shown.map_or("", Direction::label);
    egui::Frame::new()
        .fill(tokens.control_hover.gamma_multiply(0.35))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(38.0);
            if !title.is_empty() {
                ui.label(RichText::new(title).strong());
            }
            ui.label(RichText::new(text).small());
        });
    if let Some(warning) = missing(&state.directions, &state.directions_elsewhere) {
        ui.colored_label(ui.visuals().warn_fg_color, RichText::new(warning).small());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sign_has_one_tile() {
        let mut all: Vec<Direction> = TARGETS
            .iter()
            .chain(JUMPS.iter().flatten())
            .chain(&TO_CODA)
            .copied()
            .collect();
        all.sort_by_key(|direction| Direction::ALL.iter().position(|d| d == direction));
        assert_eq!(all, Direction::ALL);
    }

    #[test]
    fn a_bar_jumps_back_once_but_may_hold_several_targets() {
        let mut chosen = Vec::new();
        toggle(&mut chosen, Direction::Segno);
        toggle(&mut chosen, Direction::Coda);
        toggle(&mut chosen, Direction::DaCapo);
        toggle(&mut chosen, Direction::DaSegnoAlCoda);
        toggle(&mut chosen, Direction::DaCoda);
        assert_eq!(
            chosen,
            [
                Direction::Segno,
                Direction::Coda,
                Direction::DaSegnoAlCoda,
                Direction::DaCoda
            ]
        );
        toggle(&mut chosen, Direction::DaDoubleCoda);
        toggle(&mut chosen, Direction::Segno);
        assert_eq!(
            chosen,
            [
                Direction::Coda,
                Direction::DaSegnoAlCoda,
                Direction::DaDoubleCoda
            ]
        );
    }

    #[test]
    fn a_jump_without_its_targets_is_warned_about() {
        assert_eq!(missing(&[Direction::Segno], &[]), None);
        assert_eq!(
            missing(&[Direction::DaSegnoAlCoda], &[Direction::Coda]),
            Some(
                "Not in the song yet: a segno and a \"To Coda\". Until then, playback goes \
                 on without this jump."
                    .to_owned()
            )
        );
        assert_eq!(
            missing(
                &[Direction::DaSegnoAlCoda],
                &[Direction::Segno, Direction::DaCoda, Direction::Coda]
            ),
            None
        );
    }
}
