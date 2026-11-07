//! Song-wide edits: one text field and the initial tempo.
//!
//! These do not touch any measure, so they use [`Song::edit_metadata`] and leave every
//! measure's layout stamp alone; the document's revision still makes the page draw the new
//! title.

use super::{Change, ChangeKind};
use crate::error::ModelError;
use crate::metadata::{MAX_TEMPO, SongTextField};
use crate::song::Song;

/// Longest song-wide sound-state payload accepted. Ours is a few kilobytes (the mixer's
/// buses); this bounds a hostile file or a hand-edited notes block.
const MAX_SONG_SOUND_STATE_BYTES: usize = 256 * 1024;

impl Song {
    /// Sets GuitarScore's own song-wide sound state (the mixer's buses, ...), an opaque
    /// payload the app serializes, or `None` for the program's own settings. It has no
    /// field in the Guitar Pro format and is saved in the song's notes.
    pub fn set_sound_state(&mut self, state: Option<String>) -> Result<Change, ModelError> {
        if state
            .as_ref()
            .is_some_and(|state| state.len() > MAX_SONG_SOUND_STATE_BYTES)
        {
            return Err(ModelError::Invalid(format!(
                "the song's sound state is longer than {MAX_SONG_SOUND_STATE_BYTES} bytes"
            )));
        }
        let previous = self.replace_sound_state(state);
        Ok(Change {
            kind: ChangeKind::SongSoundState { previous },
        })
    }

    /// Sets one text field of the song, after checking its length.
    pub fn set_text(&mut self, field: SongTextField, text: &str) -> Result<Change, ModelError> {
        field.check(text)?;
        let previous = field.get(&self.info());
        self.edit_metadata(|inner| {
            let replace = |slot: &mut String| *slot = text.to_owned();
            match field {
                SongTextField::Title => replace(&mut inner.name),
                SongTextField::Subtitle => replace(&mut inner.subtitle),
                SongTextField::Artist => replace(&mut inner.artist),
                SongTextField::Album => replace(&mut inner.album),
                SongTextField::Lyricist => replace(&mut inner.words),
                SongTextField::Composer => replace(&mut inner.author),
                SongTextField::Copyright => replace(&mut inner.copyright),
                SongTextField::TabAuthor => replace(&mut inner.writer),
                SongTextField::Instructions => replace(&mut inner.instructions),
                SongTextField::Notes => {
                    inner.notice = if text.is_empty() {
                        Vec::new()
                    } else {
                        text.split('\n').map(str::to_owned).collect()
                    };
                }
            }
            Ok(Change {
                kind: ChangeKind::SongText { field, previous },
            })
        })
    }

    /// Sets the initial tempo, in quarter notes per minute. When measure 1 carries a tempo
    /// of its own (GP6 and later files store the initial tempo there), it changes too, so
    /// playback and the file agree with the new value.
    pub fn set_tempo(&mut self, tempo: u32) -> Result<Change, ModelError> {
        if !(1..=MAX_TEMPO).contains(&tempo) {
            return Err(ModelError::Invalid(format!(
                "tempo {tempo} (supported: 1 to {MAX_TEMPO} BPM)"
            )));
        }
        let value = i16::try_from(tempo)
            .map_err(|_| ModelError::Invalid(format!("tempo {tempo} does not fit")))?;
        self.edit_metadata(|inner| {
            let previous = inner.tempo;
            inner.tempo = value;
            let previous_first = inner
                .measure_headers
                .first_mut()
                .filter(|header| header.tempo > 0)
                .map(|header| std::mem::replace(&mut header.tempo, i32::from(value)));
            Ok(Change {
                kind: ChangeKind::SongTempo {
                    previous,
                    previous_first,
                },
            })
        })
    }
}
