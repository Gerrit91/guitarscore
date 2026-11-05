//! `bench`: how long the work behind one keystroke takes on real files (opening, layout,
//! an edit and the layout after it, sequencing). Numbers are medians of several runs; build
//! with `--release` (`cargo xtask bench` does) for numbers that mean anything.

use std::path::Path;
use std::time::{Duration, Instant};

use gscore_edit::{Command, Editor};
use gscore_layout::{Fonts, LayoutCache, LayoutOptions, layout_track, layout_tracks};
use gscore_model::{
    BeatIndex, BeatKind, BeatPosition, Fret, MeasureIndex, Song, StringNumber, TrackIndex,
    VoiceIndex,
};
use gscore_play::Sequence;

use crate::CliError;

/// Runs per measurement; the median is reported.
const RUNS: usize = 9;

/// Performance budgets (AGENTS.md section 9, M10), checked with `--check` in release builds.
/// From a key press to the new layout: within one frame at 60 Hz.
const KEY_TO_SCREEN: Duration = Duration::from_millis(16);
/// Opening a file.
const OPEN: Duration = Duration::from_secs(2);

/// The median time of `RUNS` calls of `work`.
fn median(mut work: impl FnMut()) -> Duration {
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed()
        })
        .collect();
    times.sort();
    times.get(RUNS / 2).copied().unwrap_or_default()
}

fn ms(duration: Duration) -> String {
    format!("{:8.3} ms", duration.as_secs_f64() * 1000.0)
}

/// What one file measured, for the budget check.
struct Measured {
    open: Duration,
    key_to_screen: Option<Duration>,
}

/// Times the work behind a keystroke on each file; with `check`, fails if a release build
/// misses a budget.
pub(crate) fn run(files: &[impl AsRef<Path>], check: bool) -> Result<String, CliError> {
    if files.is_empty() {
        return Err(CliError::Usage(
            "bench needs at least one file\n".to_owned(),
        ));
    }
    let fonts = Fonts::bundled().map_err(|e| CliError::Failed(e.to_string()))?;
    let options = LayoutOptions::default();
    let mut out = String::new();
    if cfg!(debug_assertions) {
        out.push_str("Note: a debug build; use --release (cargo xtask bench) for real numbers.\n");
    }
    let mut missed = Vec::new();
    for file in files {
        let file = file.as_ref();
        let (text, measured) = bench_file(file, &fonts, &options)?;
        out.push_str(&text);
        if measured.open > OPEN {
            missed.push(format!(
                "{}: opening took over {}",
                file.display(),
                ms(OPEN).trim()
            ));
        }
        if measured.key_to_screen.is_some_and(|t| t > KEY_TO_SCREEN) {
            missed.push(format!(
                "{}: edit and layout took over {}",
                file.display(),
                ms(KEY_TO_SCREEN).trim()
            ));
        }
    }
    if check {
        if cfg!(debug_assertions) {
            return Err(CliError::Failed(format!(
                "{out}the budgets apply to release builds; run `cargo xtask bench`"
            )));
        }
        if !missed.is_empty() {
            return Err(CliError::Failed(format!(
                "{out}missed the performance budget:\n  {}",
                missed.join("\n  ")
            )));
        }
        out.push_str(&format!(
            "Within budget: edit and layout under {}, opening under {}.\n",
            ms(KEY_TO_SCREEN).trim(),
            ms(OPEN).trim()
        ));
    }
    Ok(out)
}

fn bench_file(
    file: &Path,
    fonts: &Fonts,
    options: &LayoutOptions,
) -> Result<(String, Measured), CliError> {
    let failed = |e: &dyn std::fmt::Display| CliError::Failed(format!("{}: {e}", file.display()));
    let open = median(|| {
        let _ = crate::input::read_song(file);
    });
    let song = crate::input::read_song(file)?.song;
    let mut out = format!(
        "{}: {} measures, {} tracks\n",
        file.display(),
        song.measure_count(),
        song.track_count()
    );
    out.push_str(&format!("  open                     {}\n", ms(open)));
    let mut measured = Measured {
        open,
        key_to_screen: None,
    };

    // The track the app would show while editing: the one with the most notes.
    let Some(track) = busiest_track(&song, fonts, options) else {
        out.push_str("  (no track can be laid out)\n");
        return Ok((out, measured));
    };
    let layout = median(|| {
        let _ = layout_track(&song, track, options, fonts);
    });
    out.push_str(&format!(
        "  layout {:<17} {}\n",
        format!("({track})"),
        ms(layout)
    ));

    let sequence = median(|| {
        let _ = Sequence::new(&song, None);
    });
    out.push_str(&format!("  sequence                 {}\n", ms(sequence)));

    let Some(at) = beat_in_middle(&song, track) else {
        out.push_str("  (no beat to edit)\n");
        return Ok((out, measured));
    };
    let string = StringNumber::new(1).map_err(|e| failed(&e))?;
    let mut editor = Editor::new(song);
    let mut fret = 0_u8;
    // One edit and its undo per run; half of it is one edit.
    let edit = median(|| {
        fret = (fret + 1) % 24;
        let Ok(fret) = Fret::new(fret) else { return };
        if editor
            .execute(Command::SetFret { at, string, fret })
            .is_ok()
        {
            let _ = editor.undo();
        }
    }) / 2;
    out.push_str(&format!("  edit a note              {}\n", ms(edit)));
    let insert = median(|| {
        if editor
            .execute(Command::InsertMeasure {
                measure: at.measure,
            })
            .is_ok()
        {
            let _ = editor.undo();
        }
    }) / 2;
    out.push_str(&format!("  insert a measure         {}\n", ms(insert)));

    // The layout after an edit, as the app does it: through the measure cache.
    let mut cache = LayoutCache::default();
    let _ = cache.layout(editor.song(), track, options, fonts);
    let relayout = median(|| {
        fret = (fret + 1) % 24;
        let Ok(fret) = Fret::new(fret) else { return };
        let _ = editor.execute(Command::SetFret { at, string, fret });
        let _ = cache.layout(editor.song(), track, options, fonts);
    });
    out.push_str(&format!(
        "  edit and layout          {}  (key to screen, without painting)\n",
        ms(relayout)
    ));
    measured.key_to_screen = Some(relayout);

    // Every track shown together (the multitrack view): the full layout, and an edit with
    // the layout after it, which must keep the same budget.
    let tracks: Vec<TrackIndex> = editor.song().tracks().map(|t| t.index()).collect();
    if tracks.len() > 1 {
        let all = median(|| {
            let _ = layout_tracks(editor.song(), &tracks, track, options, fonts);
        });
        out.push_str(&format!(
            "  layout (all {} tracks)    {}\n",
            tracks.len(),
            ms(all)
        ));
        let mut cache = LayoutCache::default();
        let _ = cache.layout_tracks(editor.song(), &tracks, track, options, fonts);
        let relayout_all = median(|| {
            fret = (fret + 1) % 24;
            let Ok(fret) = Fret::new(fret) else { return };
            let _ = editor.execute(Command::SetFret { at, string, fret });
            let _ = cache.layout_tracks(editor.song(), &tracks, track, options, fonts);
        });
        out.push_str(&format!(
            "  edit and layout, all     {}  (key to screen, every track shown)\n",
            ms(relayout_all)
        ));
        measured.key_to_screen = Some(relayout.max(relayout_all));
    }
    Ok((out, measured))
}

/// The track with the most notes that can be laid out.
fn busiest_track(song: &Song, fonts: &Fonts, options: &LayoutOptions) -> Option<TrackIndex> {
    song.tracks()
        .filter(|t| !t.is_percussion())
        .map(|t| {
            let notes: usize = t
                .measures()
                .flat_map(|m| m.voices())
                .flat_map(|v| v.beats())
                .map(|b| b.notes().len())
                .sum();
            (t.index(), notes)
        })
        .filter(|&(index, _)| layout_track(song, index, options, fonts).is_ok())
        .max_by_key(|&(_, notes)| notes)
        .map(|(index, _)| index)
}

/// The first normal or rest beat of voice 1 from the middle measure on.
fn beat_in_middle(song: &Song, track: TrackIndex) -> Option<BeatPosition> {
    let view = song.track(track)?;
    let middle = song.measure_count() / 2;
    (middle..song.measure_count())
        .chain(0..middle)
        .find_map(|measure| {
            let measure = MeasureIndex::new(measure);
            let voice = view.measure(measure)?.voice(VoiceIndex::new(0))?;
            let beat = voice.beats().find(|b| b.kind() != BeatKind::Empty)?;
            Some(BeatPosition {
                track,
                measure,
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(beat.index().get()),
            })
        })
}
