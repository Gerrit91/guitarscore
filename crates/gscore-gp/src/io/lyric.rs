//! Guitar Pro reading and writing: lyric.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub(crate) trait SongLyricOps {
    fn read_lyrics(&self, data: &[u8], seek: &mut usize) -> GpResult<Lyrics>;
    fn write_lyrics(&self, data: &mut Vec<u8>) -> GpResult<()>;
}

impl SongLyricOps for Song {
    /// Read lyrics.
    ///
    /// First, read an `i32` that points to the track lyrics are bound to. Then it is followed by 5 lyric lines. Each one consists of
    /// number of starting measure encoded in`i32` and`int-size-string` holding text of the lyric line.
    fn read_lyrics(&self, data: &[u8], seek: &mut usize) -> GpResult<Lyrics> {
        // Some files store `-1` here to mean "no lyric track"; the field is
        // purely metadata about which track carries the lyric, so accept any
        // int and clamp into u8.
        let mut lyrics = Lyrics {
            track_choice: read_int(data, seek)? as u8,
            ..Default::default()
        };
        for i in 0..5u8 {
            let starting_measure = read_int(data, seek)?.max(0) as u16;
            lyrics
                .lines
                .push((i, starting_measure, read_int_size_string(data, seek)?));
        }
        Ok(lyrics)
    }
    fn write_lyrics(&self, data: &mut Vec<u8>) -> GpResult<()> {
        write_i32(
            data,
            self.lyrics.track_choice.to_i32_gp("lyrics track_choice")?,
        );
        // GuitarScore fix: always write the 5 lines the format has; songs read from GP6/GP7
        // have none, and indexing them panicked. Missing lines start at measure 1, empty.
        for i in 0..5 {
            let (measure, text) = self
                .lyrics
                .lines
                .get(i)
                .map_or((1, ""), |(_, measure, text)| (*measure, text.as_str()));
            write_i32(data, measure.to_i32_gp("lyrics line measure")?);
            write_int_size_string(data, text);
        }
        Ok(())
    }
}

// GuitarScore regression test (see docs/guitarpro.md).
#[cfg(test)]
mod tests {
    use crate::io::Song;

    #[test]
    fn songs_without_lyric_lines_can_be_written_as_gp5() {
        // Songs read from GP6/GP7 have no lyric lines; the GP5 writer used to panic.
        let mut song = Song::default();
        song.tracks.push(Default::default());
        song.measure_headers.push(Default::default());
        song.tracks[0].measures.push(Default::default());
        assert!(song.lyrics.lines.is_empty());
        let bytes = song.write((5, 1, 0), None).expect("written");
        let mut reread = Song::default();
        reread.read_gp5(&bytes).expect("read back");
        assert_eq!(reread.lyrics.lines.len(), 5);
    }
}
