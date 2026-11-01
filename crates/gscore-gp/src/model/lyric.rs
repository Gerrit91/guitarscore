//! Guitar Pro song data: lyric.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

pub const _MAX_LYRICS_LINE_COUNT: u8 = 5;

/// Struct to keep lyrics
/// On guitar pro files (gp4 or later), you can have 5 lines of lyrics.
/// It is store on a BTreeMap:
/// * the key is the mesure number. The start mesure is 1
/// * the value is the text. Syntax:
///   * " " (spaces or carry returns): separates the syllables of a word
///   * "+": merge two syllables for the same beat
///   * "\[lorem ipsum...\]": hidden text
#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub track_choice: u8,
    pub lines: Vec<(u8, u16, String)>,
}

//impl Default for Lyrics { fn default() -> Self { Lyrics { track_choice: 0, line1: BTreeMap::new(), line2: BTreeMap::new(), line3: BTreeMap::new(), line4: BTreeMap::new(), line5: BTreeMap::new(), }}}
impl std::fmt::Display for Lyrics {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(&l.2);
            s.push('\n');
        }
        write!(f, "{}", s.trim().replace(['\n', '\r'], " "))
    }
}
