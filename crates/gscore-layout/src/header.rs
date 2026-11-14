//! The top of the first page: title block, credits, tuning and tempo.

use gscore_model::{MeasureIndex, Song, SongTextField, TrackView};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::score_mark::{ScoreMark, ScoreMarkKind};

/// A text of the score's header that can be clicked to edit it in place: which song field
/// it shows, and where it lies on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderText {
    /// The song text field the text shows.
    pub field: SongTextField,
    /// Where the text is, in staff spaces (the drawn text's box, a little roomy for a
    /// click).
    pub rect: Rect,
}

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
    /// The editable texts written so far, and where they are.
    texts: &'o mut Vec<HeaderText>,
    /// The marks a click opens an editor for (the tunings and the tempo).
    marks: &'o mut Vec<ScoreMark>,
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

    /// [`Self::push`] for a song text field's value, remembering where the text is (with
    /// `width` in staff spaces) so a click can edit that field in place.
    fn push_field(&mut self, field: SongTextField, text: &str, width: f32, x: f32, style: &Style) {
        self.push(text, x, style);
        let left = match style.anchor {
            TextAnchor::Start => x,
            TextAnchor::Middle => x - width / 2.0,
            TextAnchor::End => x - width,
        };
        self.texts.push(HeaderText {
            field,
            rect: Rect {
                origin: Point::new(left, self.y - style.size),
                width,
                height: style.size * LINE_HEIGHT,
            },
        });
    }
}

/// Gap between two tracks' blocks of name and tuning.
const BLOCK_GAP: f32 = 3.0;

/// Draws the header for `tracks` of `song` and returns where it ends (y), the texts a
/// click can edit and the marks a click opens an editor for (each tuning, the tempo). One track gets its tuning on one line; several get a block each (name,
/// then tuning), side by side.
pub(crate) fn draw(
    song: &Song,
    tracks: &[TrackView<'_>],
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> Result<(f32, Vec<HeaderText>, Vec<ScoreMark>), LayoutError> {
    let options = ctx.options;
    let left = options.margin_left;
    let right = options.page_width - options.margin_right;
    let center = (left + right) / 2.0;
    let mut texts = Vec::new();
    let mut marks = Vec::new();
    let mut lines = Lines {
        y: options.margin_top,
        ops,
        texts: &mut texts,
        marks: &mut marks,
    };

    if !song.title().is_empty() {
        let width = ctx.fonts.text.width(song.title(), TITLE.size);
        lines.push_field(SongTextField::Title, song.title(), width, center, &TITLE);
        lines.y += TITLE_GAP;
    }
    for (field, text, style) in [
        (SongTextField::Subtitle, song.subtitle(), &SUBTITLE),
        (SongTextField::Artist, song.artist(), &ARTIST),
        (SongTextField::Album, song.album(), &SUBTITLE),
    ] {
        if !text.is_empty() {
            let width = ctx.fonts.text.width(text, style.size);
            lines.push_field(field, text, width, center, style);
        }
    }
    for (field, credit) in credits(song.lyricist(), song.composer()) {
        let width = ctx.fonts.text.width(&credit, CREDIT.size);
        lines.push_field(field, &credit, width, right, &CREDIT);
    }
    lines.y += INFO.size;
    match tracks {
        [track] => {
            if let Some(tuning) = tuning_text(*track)? {
                lines.push(&tuning, left, &INFO);
                let width = ctx.fonts.text.width(&tuning, INFO.size);
                lines.marks.push(ScoreMark::new(
                    ScoreMarkKind::Tuning {
                        track: track.index(),
                    },
                    left,
                    lines.y - INFO.size,
                    left + width,
                    lines.y - INFO.size + INFO.size * LINE_HEIGHT,
                ));
            }
        }
        _ => track_blocks(tracks, &mut lines, left, right, ctx)?,
    }

    // Tempo mark: a quarter note, then "= 120". The note's stem reaches above the text, so
    // the line is moved down by the stem's height, not the text's.
    let note = Glyph::MetronomeQuarter;
    let font = &ctx.fonts.music;
    let stem = font.bbox(note).y_max * TEMPO_NOTE_SCALE;
    lines.y += stem.max(TEMPO.size * LINE_HEIGHT) - TEMPO.size * LINE_HEIGHT + 0.5;
    let text_x = left + font.advance(note) * TEMPO_NOTE_SCALE + TEMPO_NOTE_GAP;
    let tempo = format!("= {}", song.tempo());
    lines.push(&tempo, text_x, &TEMPO);
    let y = lines.y;
    // The header's tempo is the one the first bar starts at.
    lines.marks.push(ScoreMark::new(
        ScoreMarkKind::Tempo {
            measure: MeasureIndex::new(0),
        },
        left,
        y - stem.max(TEMPO.size),
        text_x + ctx.fonts.text.width(&tempo, TEMPO.size) * 1.08,
        y + 0.3,
    ));
    ops.push(DrawOp::Glyph {
        glyph: note,
        at: Point::new(left, y),
        scale: TEMPO_NOTE_SCALE,
        color: ColorToken::Annotation,
    });
    Ok((y, texts, marks))
}

/// Each track's name with its tuning below, in blocks from left to right that wrap onto
/// another row when the line is full.
fn track_blocks(
    tracks: &[TrackView<'_>],
    lines: &mut Lines<'_>,
    left: f32,
    right: f32,
    ctx: &Context<'_>,
) -> Result<(), LayoutError> {
    let mut blocks = Vec::with_capacity(tracks.len());
    for track in tracks {
        let mut block = vec![track.name().trim().to_owned()];
        let tuned = match block_tuning(*track)? {
            Some(tuning) => {
                block.push(tuning);
                true
            }
            None => false,
        };
        let width = block
            .iter()
            .map(|line| ctx.fonts.text.width(line, INFO.size))
            .fold(0.0_f32, f32::max);
        blocks.push((block, width, tuned.then(|| track.index())));
    }
    let line_height = INFO.size * LINE_HEIGHT;
    let row_top = lines.y;
    let mut x = left;
    let mut top = row_top;
    let mut tallest = 0_usize;
    for (block, width, tuned) in blocks {
        if x > left && x + width > right {
            top += tallest as f32 * line_height;
            x = left;
            tallest = 0;
        }
        for (row, text) in block.iter().enumerate() {
            lines.ops.push(DrawOp::Text {
                text: text.clone(),
                at: Point::new(x, top + (row as f32 + 1.0) * line_height),
                size: INFO.size,
                font: INFO.font,
                anchor: INFO.anchor,
                color: INFO.color,
            });
        }
        // The tuning is the block's second line.
        if let Some(track) = tuned {
            lines.marks.push(ScoreMark::new(
                ScoreMarkKind::Tuning { track },
                x,
                top + 2.0 * line_height - INFO.size,
                x + width,
                top + 2.0 * line_height + 0.3,
            ));
        }
        tallest = tallest.max(block.len());
        x += width + BLOCK_GAP;
    }
    lines.y = top + tallest as f32 * line_height;
    Ok(())
}

/// A tuning for a track's block: its name ("Drop D"), or its notes lowest string first;
/// `None` for a track without one (drums).
fn block_tuning(track: TrackView<'_>) -> Result<Option<String>, LayoutError> {
    if track.is_percussion() {
        return Ok(None);
    }
    let Some(strings) = track.tuning()? else {
        return Ok(None);
    };
    Ok(Some(match track.tuning_name()? {
        Some(name) => name,
        None => {
            let notes: Vec<&str> = strings.iter().rev().map(|s| s.pitch.note_name()).collect();
            notes.join(" ")
        }
    }))
}

/// "Words & Music by X" when one person wrote both, otherwise one line per role; with the
/// song field each line shows (the combined line shows the composer's name).
fn credits(lyricist: &str, composer: &str) -> Vec<(SongTextField, String)> {
    match (lyricist.is_empty(), composer.is_empty()) {
        (false, false) if lyricist == composer => vec![(
            SongTextField::Composer,
            format!("Words & Music by {composer}"),
        )],
        (lyrics_missing, music_missing) => {
            let mut lines = Vec::new();
            if !lyrics_missing {
                lines.push((SongTextField::Lyricist, format!("Words by {lyricist}")));
            }
            if !music_missing {
                lines.push((SongTextField::Composer, format!("Music by {composer}")));
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
        let texts =
            |lyricist, composer| credits(lyricist, composer).into_iter().collect::<Vec<_>>();
        assert_eq!(
            texts("A", "A"),
            [(SongTextField::Composer, "Words & Music by A".to_owned())]
        );
        assert_eq!(
            texts("A", "B"),
            [
                (SongTextField::Lyricist, "Words by A".to_owned()),
                (SongTextField::Composer, "Music by B".to_owned()),
            ]
        );
        assert_eq!(
            texts("", "B"),
            [(SongTextField::Composer, "Music by B".to_owned())]
        );
        assert!(credits("", "").is_empty());
    }
}
