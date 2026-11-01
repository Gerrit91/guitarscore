//! `dump`: print a song's string tracks as ASCII tablature.
//!
//! Layout, matching Guitar Pro's ASCII export for simple scores: one row per string (highest
//! first, labelled with its note name), two dashes after and before each barline, and each
//! beat as wide as its length (a whole note is 24 columns, so an eighth is 3), but always
//! wide enough for its text. All voices of a measure share one time grid. Dead notes are
//! `x`, tied notes `(5)`, values that cannot be shown `?`. The song ends with `||`; long
//! tracks wrap at measure boundaries.

use std::collections::BTreeMap;
use std::path::Path;

use gscore_model::{
    BeatKind, Duration, InstrumentKind, MeasureView, ModelError, NoteKind, NoteView, Song,
    TrackIndex, TrackView,
};

use crate::CliError;

/// Columns of a whole note; a beat's width is its share of this.
const WHOLE_NOTE_COLUMNS: u64 = 24;
/// Dashes after and before every barline.
const BAR_PADDING: &str = "--";

/// Reads the file at `path` and returns its tablature. `track` is 1-based; `None` dumps all
/// tracks. `width` is the maximum line length before wrapping.
pub(crate) fn run(path: &Path, track: Option<usize>, width: usize) -> Result<String, CliError> {
    let imported = crate::input::read_song(path)?;
    let selected = match track {
        None => None,
        Some(number) => {
            let index = number
                .checked_sub(1)
                .map(TrackIndex::new)
                .filter(|&i| imported.song.track(i).is_some())
                .ok_or_else(|| {
                    CliError::Usage(format!(
                        "track {number} does not exist; the song has {} track(s)\n",
                        imported.song.track_count()
                    ))
                })?;
            Some(index)
        }
    };
    render(&imported.song, selected, width)
        .map_err(|e| CliError::Failed(format!("cannot dump {}: {e}", path.display())))
}

/// The tablature of one track, or of all tracks, separated by blank lines.
fn render(song: &Song, only: Option<TrackIndex>, width: usize) -> Result<String, ModelError> {
    let mut out = String::new();
    for track in song.tracks() {
        if only.is_some_and(|i| i != track.index()) {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&render_track(&track, width)?);
    }
    if out.is_empty() {
        out.push_str("(no tracks)\n");
    }
    Ok(out)
}

/// A heading line, then the systems (blocks of string rows) separated by blank lines.
fn render_track(track: &TrackView<'_>, width: usize) -> Result<String, ModelError> {
    let kind = track.instrument_kind();
    if kind != InstrumentKind::Strings {
        let what = track.instrument_name().unwrap_or(match kind {
            InstrumentKind::Drums => "Drums",
            _ => "not a string instrument",
        });
        return Ok(format!(
            "{}: {}: {what}, no tablature\n",
            track.index(),
            track.name()
        ));
    }
    let mut out = match track.tuning_name()? {
        Some(name) => format!("{}: {} ({name})\n\n", track.index(), track.name()),
        None => format!("{}: {}\n\n", track.index(), track.name()),
    };

    let labels = string_labels(track)?;
    let strings = labels.len();
    let measures = track
        .measures()
        .map(|m| render_measure(&m, strings))
        .collect::<Result<Vec<_>, _>>()?;

    let label_width = labels.iter().map(String::len).max().unwrap_or(0);
    let prefix_width = label_width.saturating_add(2); // "D" + " |"
    let mut systems: Vec<&[Vec<String>]> = Vec::new();
    let mut start = 0;
    let mut line = prefix_width;
    for (i, measure) in measures.iter().enumerate() {
        let measure_width = measure.first().map_or(0, String::len).saturating_add(1);
        if i > start && line.saturating_add(measure_width) > width {
            systems.push(measures.get(start..i).unwrap_or_default());
            start = i;
            line = prefix_width;
        }
        line = line.saturating_add(measure_width);
    }
    systems.push(measures.get(start..).unwrap_or_default());

    let last_system = systems.len().saturating_sub(1);
    for (s, system) in systems.iter().enumerate() {
        if s > 0 {
            out.push('\n');
        }
        for (row, label) in labels.iter().enumerate() {
            out.push_str(&format!("{label:<label_width$} |"));
            for measure in *system {
                out.push_str(measure.get(row).map_or("", String::as_str));
                out.push('|');
            }
            if s == last_system {
                out.push('|');
            }
            out.push('\n');
        }
    }
    Ok(out)
}

/// Row labels, string 1 (highest) first: note names without octave, or string numbers if
/// the track has no tuning.
fn string_labels(track: &TrackView<'_>) -> Result<Vec<String>, ModelError> {
    Ok(match track.tuning()? {
        Some(tuning) => tuning
            .iter()
            .map(|s| s.pitch.note_name().to_owned())
            .collect(),
        None => (1..=track.string_count()).map(|n| n.to_string()).collect(),
    })
}

/// The rows of one measure between its barlines, one per string.
fn render_measure(measure: &MeasureView<'_>, strings: usize) -> Result<Vec<String>, ModelError> {
    // Start time of each beat -> (row, text) of the notes starting there. Rests and empty
    // beats still create a column, so their length shows.
    let mut columns: BTreeMap<Duration, Vec<(usize, String)>> = BTreeMap::new();
    let mut end = measure.time_signature()?.measure_length();
    for voice in measure.voices().filter(|v| !v.is_empty()) {
        let mut onset = Duration::ZERO;
        for beat in voice.beats() {
            if beat.kind() == BeatKind::Empty {
                continue;
            }
            let notes = columns.entry(onset).or_default();
            if beat.kind() == BeatKind::Normal {
                for note in beat.notes() {
                    let Ok(string) = note.string() else { continue };
                    let row = usize::from(string.get()).saturating_sub(1);
                    if row < strings && !notes.iter().any(|(r, _)| *r == row) {
                        notes.push((row, note_text(&note)));
                    }
                }
            }
            onset = onset.checked_add(beat.duration()?)?;
        }
        end = end.max(onset);
    }
    if columns.is_empty() {
        columns.insert(Duration::ZERO, Vec::new());
    }

    let mut rows = vec![BAR_PADDING.to_owned(); strings];
    let starts: Vec<Duration> = columns.keys().copied().collect();
    for (i, (onset, notes)) in columns.iter().enumerate() {
        let next = starts.get(i.saturating_add(1)).copied().unwrap_or(end);
        let text_width = notes.iter().map(|(_, t)| t.len()).max().unwrap_or(0);
        let width = columns_for(next.checked_sub(*onset)?).max(text_width.saturating_add(1));
        for (row, cells) in rows.iter_mut().enumerate() {
            let text = notes
                .iter()
                .find(|(r, _)| *r == row)
                .map_or("", |(_, t)| t.as_str());
            cells.push_str(text);
            cells.push_str(&"-".repeat(width.saturating_sub(text.len())));
        }
    }
    for cells in &mut rows {
        cells.push_str(BAR_PADDING);
    }
    Ok(rows)
}

/// How a note is written: its fret, `x` for dead notes, `(fret)` for tied notes.
fn note_text(note: &NoteView<'_>) -> String {
    let fret = note
        .fret()
        .map_or_else(|_| "?".to_owned(), |f| f.get().to_string());
    match note.kind() {
        NoteKind::Dead => "x".to_owned(),
        NoteKind::Tie => format!("({fret})"),
        NoteKind::Normal | NoteKind::Rest | NoteKind::Unknown => fret,
    }
}

/// Columns for a length: its share of [`WHOLE_NOTE_COLUMNS`], rounded up, at least 1.
fn columns_for(length: Duration) -> usize {
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
    fn empty_song_has_no_tracks() {
        assert_eq!(render(&Song::default(), None, 80).unwrap(), "(no tracks)\n");
    }
}
