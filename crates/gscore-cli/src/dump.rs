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
//! marked notes. Slap `S`, pop `P`, fade-in `<`, whammy `w`, brush `v`/`^` and pick stroke
//! `V`/`n` have no symbol in that table and keep the letters used before. The song ends
//! with `||`; long tracks wrap at measure boundaries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gscore_model::{
    BeatKind, BeatView, BendKind, Duration, HarmonicKind, InstrumentKind, MeasureView, ModelError,
    NoteKind, NoteView, SlapKind, SlideKind, Song, StringTuning, StrokeDirection, TrackIndex,
    TrackView, VoiceView,
};

use crate::CliError;

/// Columns of a whole note; a beat's width is its share of this.
const WHOLE_NOTE_COLUMNS: u64 = 24;
/// Dashes after and before every barline.
const BAR_PADDING: &str = "--";

/// A symbol the dump writes in the tab grid (after a fret or a beat).
///
/// Symbols that wrap a number (`GhostNote`, `HarmonicNode`, `TappedHarmonic`) are written
/// with [`Symbol::wrap`]; `text` is the pattern the legend shows.
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

    /// The marker as written in the grid, or its pattern for wrapped symbols.
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Symbol::Tie => "L",
            Symbol::DeadNote => "x",
            Symbol::GhostNote => "(n)",
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
            Symbol::HarmonicNode => "[n]",
            Symbol::TappedHarmonic => "n(n)",
            Symbol::Accent => ">",
            Symbol::Staccato => ".",
            Symbol::FadeIn => "<",
            Symbol::WhammyBar => "w",
            Symbol::BrushUp => "v",
            Symbol::BrushDown => "^",
            Symbol::PickStrokeUp => "V",
            Symbol::PickStrokeDown => "n",
            Symbol::Slap => "S",
            Symbol::Pop => "P",
        }
    }

    /// What the symbol means, for `gscore-cli legend`.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Symbol::Tie => "tie (on the first note; the continuation is a plain fret)",
            Symbol::DeadNote => "dead note",
            Symbol::GhostNote => "ghost note",
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
            Symbol::Slap => "slap, or a shift slide when the target note is unknown",
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
    fn wrap(self, inner: &str) -> String {
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
    /// Every annotation row, in display order. Add a variant here and a `render_track`
    /// row appears wherever it occurs; the rows are independent, so palm mute, let ring
    /// and later up/down picking stack under the staff.
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
}

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

/// One measure: its string rows and the annotation rows under them.
struct RenderedMeasure {
    rows: Vec<String>,
    /// One row per annotation, as wide as the string rows, with spaces where absent.
    annotations: BTreeMap<Annotation, String>,
}

/// The rows of one measure between its barlines, one per string.
fn render_measure(
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

/// Adds the annotation rows a note starts a run of.
fn mark_note(
    marked: &mut BTreeMap<Annotation, BTreeSet<Duration>>,
    onset: Duration,
    note: &NoteView<'_>,
) {
    if note.is_palm_muted()
        && let Some(onsets) = marked.get_mut(&Annotation::PalmMute)
    {
        onsets.insert(onset);
    }
    if note.is_let_ring()
        && let Some(onsets) = marked.get_mut(&Annotation::LetRing)
    {
        onsets.insert(onset);
    }
}

/// One annotation row: its mark starts a run, dashes continue it and a `|` closes it.
fn annotation_row(
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
fn annotation_cell(
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
fn tie_origins(
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
fn next_note_pitches(
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

/// The note's sounding pitch: the open string plus the fret. `None` for dead notes and for
/// tracks without a tuning.
fn note_pitch(note: &NoteView<'_>, row: usize, tuning: Option<&[StringTuning]>) -> Option<i16> {
    if !matches!(note.kind(), NoteKind::Normal | NoteKind::Tie) {
        return None;
    }
    let fret = note.fret().ok()?;
    let open = tuning?.get(row)?.pitch.get();
    Some(i16::from(open).saturating_add(i16::from(fret.get())))
}

/// True if the note has a slide that ends on the next note (a shift or legato slide), so
/// the dump may write it right against that note.
fn is_direct_slide(note: &NoteView<'_>) -> bool {
    note.slides()
        .any(|slide| matches!(slide, SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo))
}

/// True if the length is at least an eighth note.
fn is_eighth_or_longer(length: Duration) -> bool {
    length.numerator().saturating_mul(8) >= length.denominator()
}

/// How a note is written: its fret (ghost notes in parentheses, `x` for dead notes), then
/// its effect symbols.
fn note_text(
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
            .and_then(|fret| bend_target(fret.get(), bend.semitones))
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

/// The target fret of a bend: the fretted note plus the distance the bend reaches, in
/// semitones (`15b17` for a whole-step bend at fret 15). Half steps print `.5`.
fn bend_target(fret: u8, semitones: i16) -> Option<String> {
    if semitones == 0 {
        return None;
    }
    let half_frets = i32::from(fret)
        .checked_mul(2)?
        .checked_add(i32::from(semitones))?;
    let whole = half_frets.div_euclid(2);
    Some(if half_frets.rem_euclid(2) == 1 {
        format!("{whole}.5")
    } else {
        whole.to_string()
    })
}

/// The node of a harmonic, or `?` when the file does not store one.
fn node_text(node: Option<u8>) -> String {
    node.map_or_else(|| "?".to_owned(), |node| node.to_string())
}

/// The slide marker for a note: `\` down and `/` up when the target pitch is known,
/// otherwise `S` (shift) or `s` (legato). Notes without a slide get no marker, even when
/// the next note on the string is higher or lower.
fn slide_symbol(note: &NoteView<'_>, pitch: Option<i16>, next: Option<i16>) -> Option<Symbol> {
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
fn beat_suffix(beat: &BeatView<'_>) -> String {
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

    #[test]
    fn bends_name_their_target_fret() {
        assert_eq!(bend_target(15, 4).as_deref(), Some("17"));
        assert_eq!(bend_target(15, 2).as_deref(), Some("16"));
        assert_eq!(bend_target(15, 3).as_deref(), Some("16.5"));
        assert_eq!(bend_target(15, 0), None);
    }

    #[test]
    fn symbols_wrap_numbers() {
        assert_eq!(Symbol::GhostNote.wrap("5"), "(5)");
        assert_eq!(Symbol::HarmonicNode.wrap("19"), "[19]");
        assert_eq!(Symbol::TappedHarmonic.wrap("19"), "(19)");
    }

    #[test]
    fn empty_song_has_no_tracks() {
        assert_eq!(render(&Song::default(), None, 80).unwrap(), "(no tracks)\n");
    }
}
