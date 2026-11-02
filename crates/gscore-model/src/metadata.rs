//! The song's text information and tempo: what the settings window edits.
//!
//! Each field is one [`SongTextField`], so a settings form can apply the fields that
//! changed as separate, undoable edits. Values are validated before they are written
//! ([`SongTextField::check`]), keeping hostile or accidental huge text out of the model.

use crate::error::ModelError;
use crate::song::Song;

/// Longest text a song field other than the notes may hold (characters).
pub const MAX_SONG_TEXT_CHARS: usize = 4096;
/// Longest notes text, lines included (characters).
pub const MAX_SONG_NOTES_CHARS: usize = 65_536;
/// Highest tempo accepted (beats per minute).
pub const MAX_TEMPO: u32 = 1000;

/// The song's information, as the settings window shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongInfo {
    /// The title (may be empty).
    pub title: String,
    /// The subtitle (may be empty).
    pub subtitle: String,
    /// The artist (may be empty).
    pub artist: String,
    /// The album (may be empty).
    pub album: String,
    /// Who wrote the lyrics (may be empty).
    pub lyricist: String,
    /// Who wrote the music (may be empty).
    pub composer: String,
    /// The copyright notice (may be empty).
    pub copyright: String,
    /// Who wrote the tablature (may be empty).
    pub tab_author: String,
    /// Performance instructions (may be empty).
    pub instructions: String,
    /// Free-text notes, one entry per line; empty if none.
    pub notes: Vec<String>,
    /// The initial tempo in quarter notes per minute (always positive).
    pub tempo: u32,
}

/// One text field of [`SongInfo`] that can be changed on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SongTextField {
    /// The title.
    Title,
    /// The subtitle.
    Subtitle,
    /// The artist.
    Artist,
    /// The album.
    Album,
    /// Who wrote the lyrics.
    Lyricist,
    /// Who wrote the music.
    Composer,
    /// The copyright notice.
    Copyright,
    /// Who wrote the tablature.
    TabAuthor,
    /// Performance instructions.
    Instructions,
    /// The free-text notes, one per line.
    Notes,
}

/// Every field, in the order the settings window shows them and `gscore-cli commands`
/// documents them.
pub const SONG_TEXT_FIELDS: &[SongTextField] = &[
    SongTextField::Title,
    SongTextField::Subtitle,
    SongTextField::Artist,
    SongTextField::Album,
    SongTextField::Lyricist,
    SongTextField::Composer,
    SongTextField::Copyright,
    SongTextField::TabAuthor,
    SongTextField::Instructions,
    SongTextField::Notes,
];

impl SongTextField {
    /// The id used in command lines and settings.
    pub fn id(self) -> &'static str {
        match self {
            SongTextField::Title => "title",
            SongTextField::Subtitle => "subtitle",
            SongTextField::Artist => "artist",
            SongTextField::Album => "album",
            SongTextField::Lyricist => "lyricist",
            SongTextField::Composer => "composer",
            SongTextField::Copyright => "copyright",
            SongTextField::TabAuthor => "tab_author",
            SongTextField::Instructions => "instructions",
            SongTextField::Notes => "notes",
        }
    }

    /// The field called `id`, if any.
    pub fn from_id(id: &str) -> Option<Self> {
        SONG_TEXT_FIELDS.iter().copied().find(|f| f.id() == id)
    }

    /// A short label for the settings window.
    pub fn label(self) -> &'static str {
        match self {
            SongTextField::Title => "Title",
            SongTextField::Subtitle => "Subtitle",
            SongTextField::Artist => "Artist",
            SongTextField::Album => "Album",
            SongTextField::Lyricist => "Lyrics by",
            SongTextField::Composer => "Music by",
            SongTextField::Copyright => "Copyright",
            SongTextField::TabAuthor => "Tab by",
            SongTextField::Instructions => "Instructions",
            SongTextField::Notes => "Notes",
        }
    }

    /// This field of `info`, with notes joined by line breaks.
    pub fn get(self, info: &SongInfo) -> String {
        match self {
            SongTextField::Title => info.title.clone(),
            SongTextField::Subtitle => info.subtitle.clone(),
            SongTextField::Artist => info.artist.clone(),
            SongTextField::Album => info.album.clone(),
            SongTextField::Lyricist => info.lyricist.clone(),
            SongTextField::Composer => info.composer.clone(),
            SongTextField::Copyright => info.copyright.clone(),
            SongTextField::TabAuthor => info.tab_author.clone(),
            SongTextField::Instructions => info.instructions.clone(),
            SongTextField::Notes => info.notes.join("\n"),
        }
    }

    /// Writes `text` into this field of `info` (notes are split into lines).
    pub fn set(self, info: &mut SongInfo, text: &str) {
        let text = text.to_owned();
        match self {
            SongTextField::Title => info.title = text,
            SongTextField::Subtitle => info.subtitle = text,
            SongTextField::Artist => info.artist = text,
            SongTextField::Album => info.album = text,
            SongTextField::Lyricist => info.lyricist = text,
            SongTextField::Composer => info.composer = text,
            SongTextField::Copyright => info.copyright = text,
            SongTextField::TabAuthor => info.tab_author = text,
            SongTextField::Instructions => info.instructions = text,
            SongTextField::Notes => {
                info.notes = if text.is_empty() {
                    Vec::new()
                } else {
                    text.split('\n').map(str::to_owned).collect()
                };
            }
        }
    }

    /// Whether `text` may be written into this field.
    pub fn check(self, text: &str) -> Result<(), ModelError> {
        let max = match self {
            SongTextField::Notes => MAX_SONG_NOTES_CHARS,
            _ => MAX_SONG_TEXT_CHARS,
        };
        let chars = text.chars().count();
        if chars > max {
            return Err(ModelError::Invalid(format!(
                "the {} is longer than {max} characters",
                self.id().replace('_', " ")
            )));
        }
        Ok(())
    }
}

impl Song {
    /// The song's information and tempo.
    pub fn info(&self) -> SongInfo {
        SongInfo {
            title: self.title().to_owned(),
            subtitle: self.subtitle().to_owned(),
            artist: self.artist().to_owned(),
            album: self.album().to_owned(),
            lyricist: self.lyricist().to_owned(),
            composer: self.composer().to_owned(),
            copyright: self.copyright().to_owned(),
            tab_author: self.tab_author().to_owned(),
            instructions: self.instructions().to_owned(),
            notes: self.notes().map(str::to_owned).collect(),
            tempo: self.tempo(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_round_trip_through_info() {
        let mut info = SongInfo {
            tempo: 120,
            ..SongInfo::default()
        };
        for field in SONG_TEXT_FIELDS {
            assert_eq!(SongTextField::from_id(field.id()), Some(*field));
            field.set(&mut info, &format!("value of {}", field.id()));
            assert_eq!(field.get(&info), format!("value of {}", field.id()));
        }
        SongTextField::Notes.set(&mut info, "first\nsecond");
        assert_eq!(info.notes, ["first", "second"]);
        SongTextField::Notes.set(&mut info, "");
        assert!(info.notes.is_empty());
    }

    #[test]
    fn text_limits_are_checked_in_characters() {
        let long = "ü".repeat(MAX_SONG_TEXT_CHARS + 1);
        assert!(SongTextField::Title.check(&long).is_err());
        assert!(
            SongTextField::Title
                .check(&"ü".repeat(MAX_SONG_TEXT_CHARS))
                .is_ok()
        );
        assert!(SongTextField::Notes.check(&long).is_ok());
        assert!(
            SongTextField::Notes
                .check(&"x".repeat(MAX_SONG_NOTES_CHARS + 1))
                .is_err()
        );
    }
}
