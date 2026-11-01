//! Where things are on a laid-out page: systems, measures and beats. The app uses this to
//! draw the edit cursor and to find the beat and string under a mouse click, without
//! looking into the draw operations.

use gscore_model::{BeatIndex, MeasureIndex, VoiceIndex};

use crate::draw::{Point, Rect};

/// Width of the cursor box around a fret number, in staff spaces.
const CURSOR_WIDTH: f32 = 1.8;
/// How far above a system's top and below its bottom a click still counts, in staff spaces.
const HIT_MARGIN: f32 = 2.0;

/// One line of measures.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemPlace {
    /// The top line of the topmost staff (notation, or the tab without notation).
    pub top: f32,
    /// The tab staff's top line.
    pub tab_top: f32,
    /// Distance between two tab lines.
    pub string_spacing: f32,
    /// Lines (strings) of the tab staff.
    pub strings: usize,
}

impl SystemPlace {
    /// The tab staff's bottom line.
    pub fn tab_bottom(&self) -> f32 {
        let gaps = self.strings.saturating_sub(1) as f32;
        self.tab_top + gaps * self.string_spacing
    }

    /// The y of `string` (1 = top line).
    pub fn string_y(&self, string: u8) -> f32 {
        self.tab_top + f32::from(string.saturating_sub(1)) * self.string_spacing
    }
}

/// One beat's column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatPlace {
    /// The voice.
    pub voice: VoiceIndex,
    /// The beat in that voice.
    pub beat: BeatIndex,
    /// The column's center.
    pub x: f32,
}

/// One measure on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasurePlace {
    /// The measure.
    pub measure: MeasureIndex,
    /// Index into [`Places::systems`].
    pub system: usize,
    /// Where the measure starts (after the previous barline).
    pub left: f32,
    /// Where it ends (its barline).
    pub right: f32,
    /// The first column's center: where a beat goes in a measure without beats.
    pub content: f32,
    /// Its beats, in column order.
    pub beats: Vec<BeatPlace>,
}

/// A beat and string found under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The measure.
    pub measure: MeasureIndex,
    /// The voice.
    pub voice: VoiceIndex,
    /// The beat (0 in a measure without beats).
    pub beat: BeatIndex,
    /// The string, 1 = highest.
    pub string: u8,
}

/// Where the systems, measures and beats of a page are.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Places {
    /// The systems, top to bottom.
    pub systems: Vec<SystemPlace>,
    /// The measures, in order.
    pub measures: Vec<MeasurePlace>,
}

impl Places {
    /// The place of `measure`.
    pub fn measure(&self, measure: MeasureIndex) -> Option<&MeasurePlace> {
        // Measures are stored in order, so the index usually matches the position.
        self.measures
            .get(measure.get())
            .filter(|m| m.measure == measure)
            .or_else(|| self.measures.iter().find(|m| m.measure == measure))
    }

    /// The x of a beat and its system. A beat the page does not show (in a measure without
    /// beats, or a placeholder) sits at the measure's first column.
    pub fn beat_x(
        &self,
        measure: MeasureIndex,
        voice: VoiceIndex,
        beat: BeatIndex,
    ) -> Option<(f32, &SystemPlace)> {
        let place = self.measure(measure)?;
        let system = self.systems.get(place.system)?;
        let x = place
            .beats
            .iter()
            .find(|b| b.voice == voice && b.beat == beat)
            .map_or(place.content, |b| b.x);
        Some((x, system))
    }

    /// The cursor box around `string` of a beat.
    pub fn cursor_rect(
        &self,
        measure: MeasureIndex,
        voice: VoiceIndex,
        beat: BeatIndex,
        string: u8,
    ) -> Option<Rect> {
        let (x, system) = self.beat_x(measure, voice, beat)?;
        let y = system.string_y(string);
        let height = system.string_spacing;
        Some(Rect {
            origin: Point::new(x - CURSOR_WIDTH / 2.0, y - height / 2.0),
            width: CURSOR_WIDTH,
            height,
        })
    }

    /// The column of a beat from the top staff to the tab's bottom line, for a highlight.
    pub fn column_rect(
        &self,
        measure: MeasureIndex,
        voice: VoiceIndex,
        beat: BeatIndex,
    ) -> Option<Rect> {
        let (x, system) = self.beat_x(measure, voice, beat)?;
        let top = system.top - 1.0;
        Some(Rect {
            origin: Point::new(x - CURSOR_WIDTH / 2.0, top),
            width: CURSOR_WIDTH,
            height: system.tab_bottom() + 1.0 - top,
        })
    }

    /// The beat and string nearest to `point`, if it is on a system: the nearest column of
    /// the measure under it, and the nearest string.
    pub fn hit(&self, point: Point) -> Option<Hit> {
        let (index, system) = self.systems.iter().enumerate().find(|(_, s)| {
            point.y >= s.top - HIT_MARGIN && point.y <= s.tab_bottom() + HIT_MARGIN
        })?;
        let place = self
            .measures
            .iter()
            .find(|m| m.system == index && point.x >= m.left && point.x < m.right)?;
        let nearest = place.beats.iter().min_by(|a, b| {
            (a.x - point.x)
                .abs()
                .total_cmp(&(b.x - point.x).abs())
                // Prefer the first voice where columns coincide.
                .then(a.voice.cmp(&b.voice))
        });
        let (voice, beat) = nearest.map_or((VoiceIndex::new(0), BeatIndex::new(0)), |b| {
            (b.voice, b.beat)
        });
        let strings = system.strings.max(1);
        let line = if system.string_spacing > 0.0 {
            ((point.y - system.tab_top) / system.string_spacing).round()
        } else {
            0.0
        };
        // Clamped to the staff, so the cast is in range.
        let line = line.clamp(0.0, strings.saturating_sub(1) as f32) as u8;
        Some(Hit {
            measure: place.measure,
            voice,
            beat,
            string: line.saturating_add(1),
        })
    }
}
