//! `render`: lay out one or more tracks and write them as SVG (one endless page) or PDF
//! (on pages, with the song's page setup).

use std::path::Path;

use clap::ValueEnum;
use gscore_layout::{Fonts, LayoutOptions, PdfFonts, Theme, layout_tracks, to_pdf, to_svg};
use gscore_model::TrackIndex;

use crate::CliError;

/// A color theme, as given on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ThemeArg {
    /// Dark ink on white paper
    Light,
    /// Light ink on a dark background
    Dark,
}

impl ThemeArg {
    fn theme(self) -> &'static Theme {
        match self {
            ThemeArg::Light => &Theme::LIGHT,
            ThemeArg::Dark => &Theme::DARK,
        }
    }
}

/// Reads `input`, lays out `tracks` (1-based; every track with `all`, track 1 when none is
/// given) and writes them to `output`, as SVG or PDF by the extension.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    tracks: &[usize],
    all: bool,
    theme: ThemeArg,
) -> Result<String, CliError> {
    // Only ever write an .svg or a .pdf, so a typo cannot overwrite the song itself.
    let extension = output
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    let pdf = match extension.as_deref() {
        Some("svg") => false,
        Some("pdf") => true,
        _ => {
            return Err(CliError::Usage(format!(
                "{} must end in .svg or .pdf\n",
                output.display()
            )));
        }
    };
    let imported = crate::input::read_song(input)?;
    let numbers: Vec<usize> = if all {
        (1..=imported.song.tracks().len()).collect()
    } else if tracks.is_empty() {
        vec![1]
    } else {
        tracks.to_vec()
    };
    let mut indices = Vec::with_capacity(numbers.len());
    for &number in &numbers {
        let Some(index) = number.checked_sub(1).map(TrackIndex::new) else {
            return Err(CliError::Usage("tracks count from 1\n".to_owned()));
        };
        if indices.contains(&index) {
            return Err(CliError::Usage(format!("track {number} is given twice\n")));
        }
        indices.push(index);
    }
    let first = *indices
        .first()
        .ok_or_else(|| CliError::Failed(format!("{}: the song has no tracks", input.display())))?;
    let fonts = Fonts::bundled().map_err(|e| CliError::Failed(e.to_string()))?;
    let options = if pdf {
        LayoutOptions::default()
            .with_page_setup(&imported.song.page_setup())
            .on_pages()
    } else {
        LayoutOptions::default()
    };
    let layout = layout_tracks(&imported.song, &indices, first, &options, &fonts)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let bytes = if pdf {
        to_pdf(
            &layout,
            theme.theme(),
            options.staff_space_mm,
            &PdfFonts::bundled(),
            imported.song.title(),
        )
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?
    } else {
        to_svg(&layout, theme.theme(), options.staff_space_mm).into_bytes()
    };
    std::fs::write(output, bytes)
        .map_err(|e| CliError::Failed(format!("cannot write {}: {e}", output.display())))?;
    let what = match numbers.as_slice() {
        [number] => format!("track {number}"),
        _ if all => "all tracks".to_owned(),
        _ => format!(
            "tracks {}",
            numbers
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    Ok(format!(
        "Rendered {what} of {} to {}.\n",
        input.display(),
        output.display()
    ))
}
