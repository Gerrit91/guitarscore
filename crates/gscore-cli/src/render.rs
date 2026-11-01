//! `render`: lay out one track's tablature and write it as SVG.

use std::path::Path;

use clap::ValueEnum;
use gscore_layout::{Fonts, LayoutOptions, Theme, layout_track, to_svg};
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

/// Reads `input`, lays out `track` (1-based, default 1) and writes the SVG to `output`.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    track: Option<usize>,
    theme: ThemeArg,
) -> Result<String, CliError> {
    // Only ever write an .svg, so a typo cannot overwrite the song itself.
    if !output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        return Err(CliError::Usage(format!(
            "{} must end in .svg (only SVG output is supported for now)\n",
            output.display()
        )));
    }
    let number = track.unwrap_or(1);
    let Some(index) = number.checked_sub(1).map(TrackIndex::new) else {
        return Err(CliError::Usage("tracks count from 1\n".to_owned()));
    };
    let imported = crate::input::read_song(input)?;
    let fonts = Fonts::bundled().map_err(|e| CliError::Failed(e.to_string()))?;
    let options = LayoutOptions::default();
    let layout = layout_track(&imported.song, index, &options, &fonts)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let svg = to_svg(&layout, theme.theme(), options.staff_space_mm);
    std::fs::write(output, svg)
        .map_err(|e| CliError::Failed(format!("cannot write {}: {e}", output.display())))?;
    Ok(format!(
        "Rendered track {number} of {} to {}.\n",
        input.display(),
        output.display()
    ))
}
