//! How one note and one beat are written: the fret, then the effect symbols.

use gscore_model::{
    BeatView, BendKind, Duration, HarmonicKind, NoteKind, NoteView, SlapKind, SlideKind,
    StringTuning, StrokeDirection,
};

use super::symbols::Symbol;

/// The note's sounding pitch: the open string plus the fret. `None` for dead notes and for
/// tracks without a tuning.
pub(super) fn note_pitch(
    note: &NoteView<'_>,
    row: usize,
    tuning: Option<&[StringTuning]>,
) -> Option<i16> {
    if !matches!(note.kind(), NoteKind::Normal | NoteKind::Tie) {
        return None;
    }
    let fret = note.fret().ok()?;
    let open = tuning?.get(row)?.pitch.get();
    Some(i16::from(open).saturating_add(i16::from(fret.get())))
}

/// True if the note has a slide that ends on the next note (a shift or legato slide), so
/// the dump may write it right against that note.
pub(super) fn is_direct_slide(note: &NoteView<'_>) -> bool {
    note.slides()
        .any(|slide| matches!(slide, SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo))
}

/// True if the length is at least an eighth note.
pub(super) fn is_eighth_or_longer(length: Duration) -> bool {
    length.numerator().saturating_mul(8) >= length.denominator()
}

/// How a note is written: its fret (ghost notes in parentheses, `x` for dead notes), then
/// its effect symbols.
pub(super) fn note_text(
    note: &NoteView<'_>,
    tie_origin: bool,
    pitch: Option<i16>,
    next: Option<i16>,
) -> String {
    let fret = match note.kind() {
        NoteKind::Dead => Symbol::DeadNote.text().to_owned(),
        NoteKind::Normal | NoteKind::Tie | NoteKind::Rest | NoteKind::Unknown => note
            .fret()
            .map_or_else(|_| "?".to_owned(), |f| f.get().to_string()),
    };
    let mut text = if note.is_ghost() {
        Symbol::GhostNote.wrap(&fret)
    } else {
        fret
    };
    if note.is_hammer() {
        // Guitar Pro marks hammer-ons and pull-offs the same way; choose the symbol from
        // the pitch the next note on this string actually has.
        let pull_off = matches!((pitch, next), (Some(origin), Some(target)) if target < origin);
        text.push_str(if pull_off {
            Symbol::PullOff.text()
        } else {
            Symbol::HammerOn.text()
        });
    }
    if let Some(bend) = note.bend() {
        text.push_str(match bend.kind {
            BendKind::Up => Symbol::BendUp.text(),
            BendKind::Release => Symbol::BendRelease.text(),
        });
        if let Some(target) = note
            .fret()
            .ok()
            .and_then(|fret| bend_target(fret.get(), bend.quarter_tones))
        {
            text.push_str(&target);
        }
    }
    if let Some(slide) = slide_symbol(note, pitch, next) {
        text.push_str(slide.text());
    }
    if note.has_vibrato() {
        text.push_str(Symbol::Vibrato.text());
    }
    if note.has_trill() {
        text.push_str(Symbol::Trill.text());
    }
    if note.has_tremolo_picking() {
        text.push_str(Symbol::TremoloPicking.text());
    }
    if let Some(harmonic) = note.harmonic() {
        match harmonic.kind {
            HarmonicKind::Natural => text.push_str(Symbol::NaturalHarmonic.text()),
            HarmonicKind::Tapped => {
                text.push_str(&Symbol::TappedHarmonic.wrap(&node_text(harmonic.node)));
            }
            HarmonicKind::Artificial | HarmonicKind::Pinch | HarmonicKind::Semi => {
                text.push_str(&Symbol::HarmonicNode.wrap(&node_text(harmonic.node)));
            }
        }
    }
    if note.is_accented() {
        text.push_str(Symbol::Accent.text());
    }
    if note.is_staccato() {
        text.push_str(Symbol::Staccato.text());
    }
    if tie_origin {
        text.push_str(Symbol::Tie.text());
    }
    text
}

/// The target fret of a bend: the fretted note plus how far the bend reaches. A fret is a
/// semitone, two quarter tones (`15b17` for a full, 4-quarter-tone bend at fret 15); an odd
/// number of quarter tones prints `.5`.
pub(super) fn bend_target(fret: u8, quarter_tones: i16) -> Option<String> {
    if quarter_tones == 0 {
        return None;
    }
    let half_frets = i32::from(fret)
        .checked_mul(2)?
        .checked_add(i32::from(quarter_tones))?;
    let whole = half_frets.div_euclid(2);
    Some(if half_frets.rem_euclid(2) == 1 {
        format!("{whole}.5")
    } else {
        whole.to_string()
    })
}

/// The node of a harmonic, or `?` when the file does not store one.
pub(super) fn node_text(node: Option<u8>) -> String {
    node.map_or_else(|| "?".to_owned(), |node| node.to_string())
}

/// The slide marker for a note: `\` down and `/` up when the target pitch is known,
/// otherwise `S` (shift) or `s` (legato). Notes without a slide get no marker, even when
/// the next note on the string is higher or lower.
pub(super) fn slide_symbol(
    note: &NoteView<'_>,
    pitch: Option<i16>,
    next: Option<i16>,
) -> Option<Symbol> {
    let mut fallback = None;
    let mut any = false;
    for slide in note.slides() {
        any = true;
        match slide {
            SlideKind::OutDownwards | SlideKind::IntoFromAbove => return Some(Symbol::SlideDown),
            SlideKind::OutUpwards | SlideKind::IntoFromBelow => return Some(Symbol::SlideUp),
            SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo => {
                if fallback.is_none() {
                    fallback = Some(if slide == SlideKind::ShiftSlideTo {
                        Symbol::SlideShift
                    } else {
                        Symbol::SlideLegato
                    });
                }
            }
        }
    }
    if !any {
        return None;
    }
    match (pitch, next) {
        (Some(origin), Some(target)) if target < origin => Some(Symbol::SlideDown),
        (Some(origin), Some(target)) if target > origin => Some(Symbol::SlideUp),
        _ => fallback,
    }
}

/// The symbols a beat adds after every note it contains (tapping, slap, pop, strokes,
/// fade-in, whammy bar).
pub(super) fn beat_suffix(beat: &BeatView<'_>) -> String {
    let mut text = String::new();
    match beat.slap() {
        Some(SlapKind::Tapping) => text.push_str(Symbol::Tapping.text()),
        Some(SlapKind::Slapping) => text.push_str(Symbol::Slap.text()),
        Some(SlapKind::Popping) => text.push_str(Symbol::Pop.text()),
        None => {}
    }
    match beat.brush_stroke() {
        Some(StrokeDirection::Up) => text.push_str(Symbol::BrushUp.text()),
        Some(StrokeDirection::Down) => text.push_str(Symbol::BrushDown.text()),
        None => {}
    }
    match beat.pick_stroke() {
        Some(StrokeDirection::Up) => text.push_str(Symbol::PickStrokeUp.text()),
        Some(StrokeDirection::Down) => text.push_str(Symbol::PickStrokeDown.text()),
        None => {}
    }
    if beat.has_fade_in() {
        text.push_str(Symbol::FadeIn.text());
    }
    if beat.has_whammy_bar() {
        text.push_str(Symbol::WhammyBar.text());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bends_name_their_target_fret() {
        // A full bend (4 quarter tones) at fret 15 sounds like fret 17.
        assert_eq!(bend_target(15, 4).as_deref(), Some("17"));
        assert_eq!(bend_target(15, 2).as_deref(), Some("16"));
        assert_eq!(bend_target(15, 3).as_deref(), Some("16.5"));
        assert_eq!(bend_target(15, 0), None);
    }
}
