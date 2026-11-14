//! The marks of the page that a click can open an editor for: tempo marks, section
//! names, time and key signatures and the header's tuning.

use gscore_model::{MeasureIndex, TrackIndex};

use crate::draw::{Point, Rect};

/// What a mark on the page stands for, so a click on it can open what edits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreMarkKind {
    /// The tempo a bar starts at ("♩ = 120"); the header's tempo is the first bar's.
    Tempo {
        /// The bar the tempo belongs to.
        measure: MeasureIndex,
    },
    /// A section name in its frame.
    Section {
        /// The bar the section starts at.
        measure: MeasureIndex,
    },
    /// A time signature.
    TimeSignature {
        /// The bar that carries it.
        measure: MeasureIndex,
    },
    /// A key signature written in a bar (not the one repeated at a system's start).
    KeySignature {
        /// The bar that carries it.
        measure: MeasureIndex,
    },
    /// A track's tuning in the header.
    Tuning {
        /// The track.
        track: TrackIndex,
    },
}

/// A mark of the page and where it lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreMark {
    /// What the mark stands for.
    pub kind: ScoreMarkKind,
    /// The mark's box, in staff spaces.
    pub rect: Rect,
}

impl ScoreMark {
    /// A mark of `kind` from (`left`, `top`) to (`right`, `bottom`).
    pub(crate) fn new(kind: ScoreMarkKind, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        ScoreMark {
            kind,
            rect: Rect {
                origin: Point::new(left, top),
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            },
        }
    }

    /// Whether `point` is on the mark.
    pub(crate) fn contains(&self, point: Point) -> bool {
        point.x >= self.rect.origin.x
            && point.x <= self.rect.origin.x + self.rect.width
            && point.y >= self.rect.origin.y
            && point.y <= self.rect.origin.y + self.rect.height
    }
}
