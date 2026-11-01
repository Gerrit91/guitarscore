//! The symbols `dump` writes in the tab grid and the rows under the staff. `legend` lists
//! them from the same definitions, so the two cannot drift apart.

use gscore_model::NoteView;

/// A symbol the dump writes in the tab grid (after a fret or a beat).
///
/// Symbols that wrap a number (`GhostNote`, `HarmonicNode`, `TappedHarmonic`) are written
/// with [`Symbol::wrap`]; for them `text` is an example for the legend (`(5)`, `7[19]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Symbol {
    Tie,
    DeadNote,
    GhostNote,
    HammerOn,
    PullOff,
    BendUp,
    BendRelease,
    SlideUp,
    SlideDown,
    SlideShift,
    SlideLegato,
    Vibrato,
    Trill,
    Tapping,
    TremoloPicking,
    NaturalHarmonic,
    HarmonicNode,
    TappedHarmonic,
    Accent,
    Staccato,
    FadeIn,
    WhammyBar,
    BrushUp,
    BrushDown,
    PickStrokeUp,
    PickStrokeDown,
    Slap,
    Pop,
}

impl Symbol {
    /// Every symbol, in legend order.
    pub(crate) const ALL: [Symbol; 28] = [
        Symbol::Tie,
        Symbol::DeadNote,
        Symbol::GhostNote,
        Symbol::HammerOn,
        Symbol::PullOff,
        Symbol::BendUp,
        Symbol::BendRelease,
        Symbol::SlideUp,
        Symbol::SlideDown,
        Symbol::SlideShift,
        Symbol::SlideLegato,
        Symbol::Vibrato,
        Symbol::Trill,
        Symbol::Tapping,
        Symbol::TremoloPicking,
        Symbol::NaturalHarmonic,
        Symbol::HarmonicNode,
        Symbol::TappedHarmonic,
        Symbol::Accent,
        Symbol::Staccato,
        Symbol::FadeIn,
        Symbol::WhammyBar,
        Symbol::BrushUp,
        Symbol::BrushDown,
        Symbol::PickStrokeUp,
        Symbol::PickStrokeDown,
        Symbol::Slap,
        Symbol::Pop,
    ];

    /// The marker as written in the grid, or an example for wrapped symbols.
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Symbol::Tie => "L",
            Symbol::DeadNote => "x",
            Symbol::GhostNote => "(5)",
            Symbol::HammerOn => "h",
            Symbol::PullOff => "p",
            Symbol::BendUp => "b",
            Symbol::BendRelease => "r",
            Symbol::SlideUp => "/",
            Symbol::SlideDown => "\\",
            Symbol::SlideShift => "S",
            Symbol::SlideLegato => "s",
            Symbol::Vibrato => "~",
            Symbol::Trill => "tr",
            Symbol::Tapping => "t",
            Symbol::TremoloPicking => "TP",
            Symbol::NaturalHarmonic => "*",
            Symbol::HarmonicNode => "7[19]",
            Symbol::TappedHarmonic => "7(19)",
            Symbol::Accent => ">",
            Symbol::Staccato => ".",
            Symbol::FadeIn => "<",
            Symbol::WhammyBar => "w",
            Symbol::BrushUp => "v",
            Symbol::BrushDown => "^",
            Symbol::PickStrokeUp => "V",
            Symbol::PickStrokeDown => "n",
            Symbol::Slap => "T",
            Symbol::Pop => "P",
        }
    }

    /// What the symbol means, for `gscore-cli legend`.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Symbol::Tie => "tie (on the first note; the continuation is a plain fret)",
            Symbol::DeadNote => "dead note",
            Symbol::GhostNote => "ghost note: the fret in parentheses",
            Symbol::HammerOn => "hammer-on",
            Symbol::PullOff => "pull-off",
            Symbol::BendUp => "bend up, followed by the target fret",
            Symbol::BendRelease => "release bend, followed by the target fret",
            Symbol::SlideUp => "slide up",
            Symbol::SlideDown => "slide down",
            Symbol::SlideShift => "shift slide, when the target note is unknown",
            Symbol::SlideLegato => "legato slide, when the target note is unknown",
            Symbol::Vibrato => "vibrato",
            Symbol::Trill => "trill",
            Symbol::Tapping => "tapping",
            Symbol::TremoloPicking => "tremolo picking",
            Symbol::NaturalHarmonic => "natural harmonic",
            Symbol::HarmonicNode => "pinch or artificial harmonic: the node after the fret",
            Symbol::TappedHarmonic => "tapped harmonic: written fret, then the node",
            Symbol::Accent => "accent",
            Symbol::Staccato => "staccato",
            Symbol::FadeIn => "fade in",
            Symbol::WhammyBar => "whammy bar",
            Symbol::BrushUp => "brush stroke up",
            Symbol::BrushDown => "brush stroke down",
            Symbol::PickStrokeUp => "pick stroke up",
            Symbol::PickStrokeDown => "pick stroke down",
            Symbol::Slap => "slap (thumb)",
            Symbol::Pop => "pop",
        }
    }

    /// The opening and closing markers of a symbol that wraps a number.
    const fn delimiters(self) -> Option<(&'static str, &'static str)> {
        match self {
            Symbol::GhostNote | Symbol::TappedHarmonic => Some(("(", ")")),
            Symbol::HarmonicNode => Some(("[", "]")),
            _ => None,
        }
    }

    /// Writes this symbol around `inner`: `(5)` for a ghost note, `[19]` for a harmonic
    /// node. For symbols that do not wrap anything it is the marker followed by `inner`.
    pub(super) fn wrap(self, inner: &str) -> String {
        match self.delimiters() {
            Some((open, close)) => format!("{open}{inner}{close}"),
            None => format!("{}{inner}", self.text()),
        }
    }
}

/// A row under the staff marking a technique that lasts several notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Annotation {
    PalmMute,
    LetRing,
}

impl Annotation {
    /// Every annotation row, in display order. A new variant needs an entry here and in
    /// [`Annotation::marks`]; the rows are independent, so palm mute, let ring and later
    /// up/down picking stack under the staff.
    pub(crate) const ALL: &[Annotation] = &[Annotation::PalmMute, Annotation::LetRing];

    /// The mark that starts a run on the row.
    pub(crate) const fn mark(self) -> &'static str {
        match self {
            Annotation::PalmMute => "PM",
            Annotation::LetRing => "LR",
        }
    }

    /// What the row means, for `gscore-cli legend`.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Annotation::PalmMute => "palm mute",
            Annotation::LetRing => "let ring",
        }
    }
    /// True if `note` is part of a run on this row.
    pub(crate) fn marks(self, note: &NoteView<'_>) -> bool {
        match self {
            Annotation::PalmMute => note.is_palm_muted(),
            Annotation::LetRing => note.is_let_ring(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_wrap_numbers() {
        assert_eq!(Symbol::GhostNote.wrap("5"), "(5)");
        assert_eq!(Symbol::HarmonicNode.wrap("19"), "[19]");
        assert_eq!(Symbol::TappedHarmonic.wrap("19"), "(19)");
    }

    #[test]
    fn every_marker_is_unique() {
        let texts: Vec<&str> = Symbol::ALL.iter().map(|s| s.text()).collect();
        for (i, text) in texts.iter().enumerate() {
            assert!(
                !texts[i + 1..].contains(text),
                "{text:?} is used by two symbols"
            );
        }
    }
}
