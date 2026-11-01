//! `dump`: print a song's string tracks as ASCII tablature.
//!
//! One row per string (highest first, labelled with its note name), two dashes after and
//! before each barline, and each beat as wide as its length (a whole note is 24 columns, so
//! an eighth is 3), but always wide enough for its text. All voices of a measure share one
//! time grid. Dead notes are `x`, values that cannot be shown `?`.
//!
//! Effects follow the usual ASCII-tab symbols (Wikipedia's "ASCII tab" table): `h`
//! hammer-on, `p` pull-off, `b` bend up, `r` release bend, `/` and `\` slide up/down, `~`
//! vibrato, `tr` trill, `t` tap, `TP` tremolo picking, `*` harmonic, `>` accent. Ghost notes
//! are in parentheses (`(5)`) and staccato notes end in `.`. Bends name their target fret
//! (`15b17`); pinch and artificial harmonics name their node (`7[19]`). `L` marks the note
//! a tie starts from; the continuation stays a plain fret. Palm mutes and let ring are not
//! written after the fret but on rows under the staff (`PM----|`, `LR----|`), covering the
//! marked notes. Slap `T` (thumb), pop `P`, fade-in `<`, whammy `w`, brush `v`/`^` and pick
//! stroke `V`/`n` have no symbol in that table; every marker means one thing only
//! (`gscore-cli legend` lists them). The song ends with `||`; long tracks wrap at measure
//! boundaries.
//!
//! Modules: `symbols` (what the markers are), `notation` (how one note or beat is written),
//! `grid` (the layout of one measure).

mod grid;
mod notation;
mod symbols;

use std::path::Path;

use gscore_model::{InstrumentKind, ModelError, Song, StringTuning, TrackIndex, TrackView};

use crate::CliError;
use grid::{RenderedMeasure, render_measure};
pub(crate) use symbols::{Annotation, Symbol};

/// Columns of a whole note; a beat's width is its share of this.
pub(super) const WHOLE_NOTE_COLUMNS: u64 = 24;
/// Dashes after and before every barline.
pub(super) const BAR_PADDING: &str = "--";

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

    let tuning = track.tuning()?;
    let labels = string_labels(track, tuning.as_deref());
    let strings = labels.len();
    let measures = track
        .measures()
        .map(|m| render_measure(&m, strings, tuning.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;

    let label_width = labels.iter().map(String::len).max().unwrap_or(0);
    let prefix_width = label_width.saturating_add(2); // "D" + " |"
    let mut systems: Vec<&[RenderedMeasure]> = Vec::new();
    let mut start = 0;
    let mut line = prefix_width;
    for (i, measure) in measures.iter().enumerate() {
        let measure_width = measure
            .rows
            .first()
            .map_or(0, String::len)
            .saturating_add(1);
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
                out.push_str(measure.rows.get(row).map_or("", String::as_str));
                out.push('|');
            }
            if s == last_system {
                out.push('|');
            }
            out.push('\n');
        }
        // Palm mutes, let ring and other techniques go on their own rows under the staff,
        // aligned with the notes.
        for &annotation in Annotation::ALL {
            let present = system.iter().any(|m| {
                m.annotations
                    .get(&annotation)
                    .is_some_and(|row| !row.trim().is_empty())
            });
            if !present {
                continue;
            }
            let mut row = " ".repeat(prefix_width);
            for (i, measure) in system.iter().enumerate() {
                row.push_str(
                    measure
                        .annotations
                        .get(&annotation)
                        .map_or("", String::as_str),
                );
                if i.saturating_add(1) < system.len() {
                    // The string rows have a barline here; keep the columns aligned.
                    row.push(' ');
                }
            }
            out.push_str(row.trim_end_matches(' '));
            out.push('\n');
        }
    }
    Ok(out)
}

/// Row labels, string 1 (highest) first: note names without octave, or string numbers if
/// the track has no tuning.
fn string_labels(track: &TrackView<'_>, tuning: Option<&[StringTuning]>) -> Vec<String> {
    match tuning {
        Some(tuning) => tuning
            .iter()
            .map(|s| s.pitch.note_name().to_owned())
            .collect(),
        None => (1..=track.string_count()).map(|n| n.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_song_has_no_tracks() {
        assert_eq!(render(&Song::default(), None, 80).unwrap(), "(no tracks)\n");
    }
}
