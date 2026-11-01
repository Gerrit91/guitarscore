//! Guitar Pro reading and writing: lyric.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub trait SongLyricOps {
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
        for i in 0..5 {
            write_i32(
                data,
                self.lyrics.lines[i].1.to_i32_gp("lyrics line measure")?,
            );
            write_int_size_string(data, &self.lyrics.lines[i].2);
        }
        Ok(())
    }
}
