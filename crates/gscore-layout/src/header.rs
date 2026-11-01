//! The top of the first page: title block, credits, tuning and tempo.

use gscore_model::{Song, TrackView};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::error::LayoutError;
use crate::glyph::Glyph;

/// How a line of header text looks.
struct Style {
    /// Font size (em) in staff spaces.
    size: f32,
    font: TextFont,
    anchor: TextAnchor,
    color: ColorToken,
}

const TITLE: Style = Style {
    size: 5.0,
    font: TextFont::Regular,
    anchor: TextAnchor::Middle,
    color: ColorToken::Title,
};
const SUBTITLE: Style = Style { size: 2.2, ..TITLE };
const ARTIST: Style = Style {
    font: TextFont::Bold,
    ..SUBTITLE
};
const CREDIT: Style = Style {
    size: 1.8,
    anchor: TextAnchor::End,
    ..SUBTITLE
};
const INFO: Style = Style {
    size: 1.5,
    font: TextFont::Regular,
    anchor: TextAnchor::Start,
    color: ColorToken::Annotation,
};
const TEMPO: Style = Style {
    size: 1.7,
    font: TextFont::Bold,
    ..INFO
};

/// Baseline-to-baseline distance as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.3;
/// Extra space below the title, whose large descenders would touch the subtitle.
const TITLE_GAP: f32 = 1.2;
/// Scale of the note in the tempo mark.
const TEMPO_NOTE_SCALE: f32 = 0.8;
/// Gap between the tempo note and "= 120".
const TEMPO_NOTE_GAP: f32 = 0.4;

/// Header text, one line after another.
struct Lines<'o> {
    y: f32,
    ops: &'o mut Vec<DrawOp>,
}

impl Lines<'_> {
    /// Moves down one line of `style` and writes `text` at `x` there.
    fn push(&mut self, text: &str, x: f32, style: &Style) {
        self.y += style.size * LINE_HEIGHT;
        self.ops.push(DrawOp::Text {
            text: text.to_owned(),
            at: Point::new(x, self.y),
            size: style.size,
            font: style.font,
            anchor: style.anchor,
            color: style.color,
        });
    }
}

/// Draws the header for `track` of `song` and returns where it ends (y).
pub(crate) fn draw(
    song: &Song,
    track: TrackView<'_>,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> Result<f32, LayoutError> {
    let options = ctx.options;
    let center = options.page_width / 2.0;
    let right = options.page_width - options.margin_x;
    let left = options.margin_x;
    let mut lines = Lines {
        y: options.margin_y,
        ops,
    };

    if !song.title().is_empty() {
        lines.push(song.title(), center, &TITLE);
        lines.y += TITLE_GAP;
    }
    for (text, style) in [
        (song.subtitle(), &SUBTITLE),
        (song.artist(), &ARTIST),
        (song.album(), &SUBTITLE),
    ] {
        if !text.is_empty() {
            lines.push(text, center, style);
        }
    }
    for credit in credits(song.lyricist(), song.composer()) {
        lines.push(&credit, right, &CREDIT);
    }
    lines.y += INFO.size;
    if let Some(tuning) = tuning_text(track)? {
        lines.push(&tuning, left, &INFO);
    }

    // Tempo mark: a quarter note, then "= 120". The note's stem reaches above the text, so
    // the line is moved down by the stem's height, not the text's.
    let note = Glyph::MetronomeQuarter;
    let font = &ctx.fonts.music;
    let stem = font.bbox(note).y_max * TEMPO_NOTE_SCALE;
    lines.y += stem.max(TEMPO.size * LINE_HEIGHT) - TEMPO.size * LINE_HEIGHT + 0.5;
    let text_x = left + font.advance(note) * TEMPO_NOTE_SCALE + TEMPO_NOTE_GAP;
    lines.push(&format!("= {}", song.tempo()), text_x, &TEMPO);
    let y = lines.y;
    ops.push(DrawOp::Glyph {
        glyph: note,
        at: Point::new(left, y),
        scale: TEMPO_NOTE_SCALE,
        color: ColorToken::Annotation,
    });
    Ok(y)
}

/// "Words & Music by X" when one person wrote both, otherwise one line per role.
fn credits(lyricist: &str, composer: &str) -> Vec<String> {
    match (lyricist.is_empty(), composer.is_empty()) {
        (false, false) if lyricist == composer => vec![format!("Words & Music by {composer}")],
        (lyrics_missing, music_missing) => {
            let mut lines = Vec::new();
            if !lyrics_missing {
                lines.push(format!("Words by {lyricist}"));
            }
            if !music_missing {
                lines.push(format!("Music by {composer}"));
            }
            lines
        }
    }
}

/// "Tuning: DADGAD (D A D G A D)", lowest string first; `None` without a tuning.
fn tuning_text(track: TrackView<'_>) -> Result<Option<String>, LayoutError> {
    let Some(strings) = track.tuning()? else {
        return Ok(None);
    };
    let notes: Vec<&str> = strings.iter().rev().map(|s| s.pitch.note_name()).collect();
    let notes = notes.join(" ");
    Ok(Some(match track.tuning_name()? {
        Some(name) => format!("Tuning: {name} ({notes})"),
        None => format!("Tuning: {notes}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_combine_one_author_and_list_two() {
        assert_eq!(credits("A", "A"), ["Words & Music by A"]);
        assert_eq!(credits("A", "B"), ["Words by A", "Music by B"]);
        assert_eq!(credits("", "B"), ["Music by B"]);
        assert!(credits("", "").is_empty());
    }
}
