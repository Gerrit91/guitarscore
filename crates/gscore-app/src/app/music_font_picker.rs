//! The music font selector in Settings › Page layout: one card per font the build bundles, each
//! with a strip of its own glyphs, so the choice is made by eye.

use egui::{
    FontId, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use gscore_layout::{Glyph, MusicFontChoice};
use gscore_model::{Dynamic, Fermata};

/// Height of a font's card: its name over a strip of its glyphs.
const CARD_HEIGHT: f32 = 84.0;
/// Room between two cards.
const CARD_GAP: f32 = 10.0;
/// The narrowest and widest a card gets.
const CARD_WIDTH: (f32, f32) = (168.0, 260.0);
/// Height of the glyph strip at the bottom of a card.
const SAMPLE_HEIGHT: f32 = 46.0;
/// Size of the sample glyphs.
const SAMPLE_SIZE: f32 = 25.0;
/// The glyphs a card shows: clefs, noteheads, a rest, accidentals, a dynamic and a fermata
/// say the most about a font's character in a small room.
const SAMPLE: [Glyph; 8] = [
    Glyph::GClef,
    Glyph::NoteheadBlack,
    Glyph::NoteheadWhole,
    Glyph::RestQuarter,
    Glyph::Sharp,
    Glyph::Flat,
    Glyph::Dynamic(Dynamic::F),
    Glyph::Fermata(Fermata::Medium),
];

/// Draws the selector; true when the user chose another font.
pub(super) fn picker(ui: &mut Ui, current: &mut MusicFontChoice) -> bool {
    let choices = MusicFontChoice::available();
    let count = choices.len().max(1) as f32;
    let width = ((ui.available_width() - CARD_GAP * (count - 1.0)) / count)
        .clamp(CARD_WIDTH.0, CARD_WIDTH.1);
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        for &choice in choices {
            if card(ui, choice, *current == choice, width).clicked() {
                *current = choice;
                changed = true;
            }
        }
    });
    changed
}

/// One font's card: its name and glyphs drawn with it (they say what it looks like).
fn card(ui: &mut Ui, choice: MusicFontChoice, selected: bool, width: f32) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, CARD_HEIGHT), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    let visuals = ui.visuals().clone();
    let background = if selected {
        visuals.selection.bg_fill.gamma_multiply(0.22)
    } else if hovered {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.faint_bg_color
    };
    let stroke = if selected {
        Stroke::new(1.5, visuals.selection.stroke.color)
    } else {
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
    };
    let painter = ui.painter();
    painter.rect(rect, 7.0, background, stroke, StrokeKind::Inside);
    if selected {
        // A tick, drawn (the text fonts have no check mark glyph).
        let color = visuals.selection.stroke.color;
        let at = pos2(rect.right() - 20.0, rect.top() + 12.0);
        painter.line_segment([at, at + vec2(4.0, 4.0)], Stroke::new(1.8, color));
        painter.line_segment(
            [at + vec2(4.0, 4.0), at + vec2(12.0, -6.0)],
            Stroke::new(1.8, color),
        );
    }
    let inner = rect.shrink2(vec2(11.0, 8.0));
    let mut text = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    text.label(RichText::new(choice.name()).strong());
    let strip = Rect::from_min_max(
        pos2(inner.left(), inner.bottom() - SAMPLE_HEIGHT),
        pos2(inner.right(), inner.bottom()),
    );
    sample(text.painter(), strip, choice, visuals.text_color());
    // Interaction on top of the card's text, so a click anywhere on it counts.
    let response = ui.interact(
        rect,
        ui.scope_id().with(("music_font", choice.id())),
        Sense::click(),
    );
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::Role::RadioButton);
        node.set_label(format!("{} music font", choice.name()));
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Paints the sample glyphs of `choice`, left to right, centered in `strip`.
fn sample(painter: &egui::Painter, strip: Rect, choice: MusicFontChoice, ink: egui::Color32) {
    let family = crate::fonts::music_family(choice);
    let mut at = Pos2::new(strip.left(), 0.0);
    for glyph in SAMPLE {
        let galley = painter.layout_no_wrap(
            glyph.codepoint().to_string(),
            FontId::new(SAMPLE_SIZE, family.clone()),
            ink,
        );
        let width = galley.size().x;
        if at.x + width > strip.right() {
            break;
        }
        let y = strip.center().y - galley.size().y / 2.0;
        painter.galley(pos2(at.x, y), galley, ink);
        at.x += width + 3.0;
    }
}
