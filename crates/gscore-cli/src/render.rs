//! `render`: lay out one or more tracks and write them as SVG (one endless page) or PDF
//! (on pages, with the song's page setup).

use std::path::Path;

use clap::ValueEnum;
use gscore_layout::{
    EmptyStaves, Fonts, LayoutOptions, MusicFontChoice, PdfFonts, Theme, layout_tracks, to_pdf,
    to_svg_with_music,
};
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

/// The music font, as given on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum MusicFontArg {
    /// Steinberg's engraved reference font
    Bravura,
    /// Steinberg's handwritten jazz font
    Petaluma,
    /// MuseScore's engraved font
    Leland,
}

impl MusicFontArg {
    fn choice(self) -> MusicFontChoice {
        match self {
            MusicFontArg::Bravura => MusicFontChoice::Bravura,
            MusicFontArg::Petaluma => MusicFontChoice::Petaluma,
            MusicFontArg::Leland => MusicFontChoice::Leland,
        }
    }
}

/// How the score's staves are written.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Staves {
    /// With several tracks, a track is left out of a line where it has no note.
    pub(crate) hide_empty: bool,
    /// A run of bars where every track rests is one multi-bar rest.
    pub(crate) multi_bar_rests: bool,
}

/// Reads `input`, lays out `tracks` (1-based; every track with `all`, track 1 when none is
/// given) and writes them to `output`, as SVG or PDF by the extension.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    tracks: &[usize],
    all: bool,
    staves: Staves,
    theme: ThemeArg,
    music: MusicFontArg,
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
    let music = music.choice();
    let fonts = Fonts::with_music(music).map_err(|e| CliError::Failed(e.to_string()))?;
    let options = if pdf {
        LayoutOptions::default()
            .with_page_setup(&imported.song.page_setup())
            .on_pages()
    } else {
        LayoutOptions::default()
    };
    let options = LayoutOptions {
        empty_staves: if staves.hide_empty {
            EmptyStaves::Hidden
        } else {
            EmptyStaves::Shown
        },
        multi_bar_rests: staves.multi_bar_rests,
        ..options
    };
    let layout = layout_tracks(&imported.song, &indices, first, &options, &fonts)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let bytes = if pdf {
        to_pdf(
            &layout,
            theme.theme(),
            options.staff_space_mm,
            &PdfFonts::with_music(music),
            imported.song.title(),
        )
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?
    } else {
        to_svg_with_music(&layout, theme.theme(), options.staff_space_mm, music).into_bytes()
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
