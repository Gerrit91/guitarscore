//! The layout of one measure: a time grid shared by all voices, one row per string, and the
//! rows under the staff (palm mute, let ring).

use std::collections::{BTreeMap, BTreeSet};

use gscore_model::{
    BeatKind, BeatView, Duration, MeasureView, ModelError, NoteKind, NoteView, StringTuning,
    VoiceView,
};

use super::notation::{beat_suffix, is_direct_slide, is_eighth_or_longer, note_pitch, note_text};
use super::symbols::Annotation;
use super::{BAR_PADDING, WHOLE_NOTE_COLUMNS};

/// One measure: its string rows and the annotation rows under them.
pub(super) struct RenderedMeasure {
    pub(super) rows: Vec<String>,
    /// One row per annotation, as wide as the string rows, with spaces where absent.
    pub(super) annotations: BTreeMap<Annotation, String>,
}

/// The rows of one measure between its barlines, one per string.
pub(super) fn render_measure(
    measure: &MeasureView<'_>,
    strings: usize,
    tuning: Option<&[StringTuning]>,
) -> Result<RenderedMeasure, ModelError> {
    let voices: Vec<VoiceView<'_>> = measure.voices().filter(|v| !v.is_empty()).collect();
    let tie_origins = tie_origins(&voices, strings)?;

    // Start time of each beat -> (row, text) of the notes starting there. Rests and empty
    // beats still create a column, so their length shows.
    let mut columns: BTreeMap<Duration, Vec<(usize, String)>> = BTreeMap::new();
    // Whether every note in a column is a direct slide that may sit against the next note.
    let mut abutting: BTreeMap<Duration, bool> = BTreeMap::new();
    let mut marked: BTreeMap<Annotation, BTreeSet<Duration>> = Annotation::ALL
        .iter()
        .copied()
        .map(|annotation| (annotation, BTreeSet::new()))
        .collect();
    let mut end = measure.time_signature()?.measure_length();
    for voice in &voices {
        let beats: Vec<BeatView<'_>> = voice.beats().collect();
        let next_pitches = next_note_pitches(&beats, strings, tuning);
        let mut onset = Duration::ZERO;
        for (index, beat) in beats.iter().enumerate() {
            if beat.kind() == BeatKind::Empty {
                continue;
            }
            let duration = beat.duration()?;
            let notes = columns.entry(onset).or_default();
            if beat.kind() == BeatKind::Normal {
                let suffix = beat_suffix(beat);
                let abuts = is_eighth_or_longer(duration)
                    && beat.notes().next().is_some()
                    && beat.notes().all(|note| is_direct_slide(&note));
                let slot = abutting.entry(onset).or_insert(true);
                *slot = *slot && abuts;
                for note in beat.notes() {
                    let Ok(string) = note.string() else { continue };
                    let row = usize::from(string.get()).saturating_sub(1);
                    if row < strings && !notes.iter().any(|(r, _)| *r == row) {
                        let tie_origin = tie_origins.contains(&(onset, row));
                        let pitch = note_pitch(&note, row, tuning);
                        let next = next_pitches.get(&(index, row)).copied();
                        let mut text = note_text(&note, tie_origin, pitch, next);
                        text.push_str(&suffix);
                        notes.push((row, text));
                        mark_note(&mut marked, onset, &note);
                    }
                }
            }
            onset = onset.checked_add(duration)?;
        }
        end = end.max(onset);
    }
    if columns.is_empty() {
        columns.insert(Duration::ZERO, Vec::new());
    }

    let starts: Vec<Duration> = columns.keys().copied().collect();
    let mut widths = Vec::with_capacity(starts.len());
    for (i, onset) in starts.iter().enumerate() {
        let next = starts.get(i.saturating_add(1)).copied().unwrap_or(end);
        let text_width = columns.get(onset).map_or(0, |notes| {
            notes.iter().map(|(_, t)| t.len()).max().unwrap_or(0)
        });
        let abuts = abutting.get(onset).copied().unwrap_or(false);
        // A column normally keeps one empty column after its text so the next note does
        // not touch it. A direct slide is written right against the note it slides to.
        let min_text = if abuts {
            text_width
        } else {
            text_width.saturating_add(1)
        };
        let mut width = columns_for(next.checked_sub(*onset)?).max(min_text);
        for &annotation in Annotation::ALL {
            if marked
                .get(&annotation)
                .is_some_and(|onsets| onsets.contains(onset))
            {
                width = width.max(annotation.mark().len());
            }
        }
        widths.push(width);
    }

    let mut rows = vec![BAR_PADDING.to_owned(); strings];
    // The annotation rows have no barline of their own: they are only marks under the
    // notes, with spaces where the staff has its padding.
    let mut annotations = BTreeMap::new();
    for &annotation in Annotation::ALL {
        let empty = BTreeSet::new();
        let onsets = marked.get(&annotation).unwrap_or(&empty);
        annotations.insert(
            annotation,
            annotation_row(annotation, &starts, &widths, onsets),
        );
    }
    for (i, onset) in starts.iter().enumerate() {
        let width = widths.get(i).copied().unwrap_or(1);
        let notes = columns.get(onset);
        for (row, cells) in rows.iter_mut().enumerate() {
            let text = notes
                .and_then(|notes| notes.iter().find(|(r, _)| *r == row))
                .map_or("", |(_, t)| t.as_str());
            cells.push_str(text);
            cells.push_str(&"-".repeat(width.saturating_sub(text.len())));
        }
    }
    for cells in &mut rows {
        cells.push_str(BAR_PADDING);
    }
    Ok(RenderedMeasure { rows, annotations })
}

/// Records the note's onset on every annotation row that marks it.
fn mark_note(
    marked: &mut BTreeMap<Annotation, BTreeSet<Duration>>,
    onset: Duration,
    note: &NoteView<'_>,
) {
    for &annotation in Annotation::ALL {
        if annotation.marks(note)
            && let Some(onsets) = marked.get_mut(&annotation)
        {
            onsets.insert(onset);
        }
    }
}

/// One annotation row: its mark starts a run, dashes continue it and a `|` closes it.
pub(super) fn annotation_row(
    annotation: Annotation,
    starts: &[Duration],
    widths: &[usize],
    marked: &BTreeSet<Duration>,
) -> String {
    let mut row = " ".repeat(BAR_PADDING.len());
    for (i, onset) in starts.iter().enumerate() {
        let width = widths.get(i).copied().unwrap_or(1);
        let active = marked.contains(onset);
        let previous = i
            .checked_sub(1)
            .and_then(|p| starts.get(p))
            .is_some_and(|o| marked.contains(o));
        let next = starts
            .get(i.saturating_add(1))
            .is_some_and(|o| marked.contains(o));
        row.push_str(&annotation_cell(
            annotation,
            active,
            previous,
            active && !next,
            width,
        ));
    }
    row.push_str(&" ".repeat(BAR_PADDING.len()));
    row
}

/// One cell of an annotation row.
pub(super) fn annotation_cell(
    annotation: Annotation,
    active: bool,
    previous: bool,
    last: bool,
    width: usize,
) -> String {
    if !active {
        return " ".repeat(width);
    }
    let mut cell = if previous {
        String::new()
    } else {
        annotation.mark().to_owned()
    };
    while cell.len() < width {
        cell.push('-');
    }
    if last && cell.ends_with('-') {
        cell.pop();
        cell.push('|');
    }
    cell
}

/// The (onset, row) of every note that a later tied note continues, so the dump can put
/// `L` on the tie's origin. Ties are matched within one measure.
pub(super) fn tie_origins(
    voices: &[VoiceView<'_>],
    strings: usize,
) -> Result<BTreeSet<(Duration, usize)>, ModelError> {
    let mut origins = BTreeSet::new();
    for voice in voices {
        let mut onset = Duration::ZERO;
        let mut last: Vec<Option<Duration>> = vec![None; strings];
        for beat in voice.beats() {
            if beat.kind() == BeatKind::Empty {
                continue;
            }
            if beat.kind() == BeatKind::Normal {
                for note in beat.notes() {
                    let Ok(string) = note.string() else { continue };
                    let row = usize::from(string.get()).saturating_sub(1);
                    if row >= strings {
                        continue;
                    }
                    let Some(slot) = last.get_mut(row) else {
                        continue;
                    };
                    match note.kind() {
                        NoteKind::Tie => {
                            if let Some(origin) = *slot {
                                origins.insert((origin, row));
                            }
                        }
                        NoteKind::Normal | NoteKind::Dead => *slot = Some(onset),
                        NoteKind::Rest | NoteKind::Unknown => {}
                    }
                }
            }
            onset = onset.checked_add(beat.duration()?)?;
        }
    }
    Ok(origins)
}

/// For every note in a voice, the pitch of the next note on the same string (within the
/// measure), keyed by beat index and row. Hammer-ons and slides use it to tell up from down.
pub(super) fn next_note_pitches(
    beats: &[BeatView<'_>],
    strings: usize,
    tuning: Option<&[StringTuning]>,
) -> BTreeMap<(usize, usize), i16> {
    let mut next = BTreeMap::new();
    let mut upcoming: Vec<Option<i16>> = vec![None; strings];
    for (index, beat) in beats.iter().enumerate().rev() {
        if beat.kind() != BeatKind::Normal {
            continue;
        }
        for note in beat.notes() {
            let Ok(string) = note.string() else { continue };
            let row = usize::from(string.get()).saturating_sub(1);
            if row >= strings {
                continue;
            }
            if let Some(pitch) = upcoming.get(row).copied().flatten() {
                next.insert((index, row), pitch);
            }
            let pitch = note_pitch(&note, row, tuning);
            if let Some(slot) = upcoming.get_mut(row) {
                *slot = pitch;
            }
        }
    }
    next
}

/// Columns for a length: its share of [`WHOLE_NOTE_COLUMNS`], rounded up, at least 1.
pub(super) fn columns_for(length: Duration) -> usize {
    let scaled = length.numerator().saturating_mul(WHOLE_NOTE_COLUMNS);
    let columns = scaled.div_ceil(length.denominator().max(1));
    usize::try_from(columns).unwrap_or(usize::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(num: u64, den: u64) -> Duration {
        Duration::new(num, den).unwrap()
    }

    #[test]
    fn beats_are_as_wide_as_their_length() {
        assert_eq!(columns_for(d(1, 1)), 24);
        assert_eq!(columns_for(d(1, 4)), 6);
        assert_eq!(columns_for(d(1, 8)), 3);
        assert_eq!(columns_for(d(1, 16)), 2); // 1.5 rounded up
        assert_eq!(columns_for(d(1, 12)), 2); // triplet eighth
        assert_eq!(columns_for(d(1, 128)), 1);
        assert_eq!(columns_for(Duration::ZERO), 1);
    }

    #[test]
    fn annotation_runs_start_with_pm_and_end_with_a_bar() {
        assert_eq!(
            annotation_cell(Annotation::PalmMute, true, false, false, 3),
            "PM-"
        );
        assert_eq!(
            annotation_cell(Annotation::PalmMute, true, true, false, 3),
            "---"
        );
        assert_eq!(
            annotation_cell(Annotation::PalmMute, true, true, true, 3),
            "--|"
        );
        assert_eq!(
            annotation_cell(Annotation::LetRing, true, false, true, 2),
            "LR"
        );
        assert_eq!(
            annotation_cell(Annotation::LetRing, false, true, false, 3),
            "   "
        );
    }

    #[test]
    fn palm_mute_and_let_ring_rows_are_independent() {
        let starts = [d(0, 1), d(1, 4)];
        let widths = [6, 6];
        let onsets: BTreeSet<Duration> = [starts[0]].into_iter().collect();
        assert_eq!(
            annotation_row(Annotation::PalmMute, &starts, &widths, &onsets),
            format!("  PM---|{}", " ".repeat(8))
        );
        assert_eq!(
            annotation_row(Annotation::LetRing, &starts, &widths, &onsets),
            format!("  LR---|{}", " ".repeat(8))
        );
    }
}
