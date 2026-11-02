//! Guitar Pro reading and writing: page.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub(crate) trait SongPageOps {
    fn read_page_setup(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()>;
    fn write_page_setup(&self, data: &mut Vec<u8>) -> GpResult<()>;
}

impl SongPageOps for Song {
    /// Read page setup. Page setup is read as follows:
    /// - Page size: 2 `Ints <int>`. Width and height of the page.
    /// - Page padding: 4 `Ints <int>`. Left, right, top, bottom padding of the page.
    /// - Score size proportion: `int`.
    /// - Header and footer elements: `short`. See `HeaderFooterElements` for value mapping.
    /// - List of placeholders:
    ///   * title
    ///   * subtitle
    ///   * artist
    ///   * album
    ///   * words
    ///   * music
    ///   * wordsAndMusic
    ///   * copyright1, e.g. *"Copyright %copyright%"*
    ///   * copyright2, e.g. *"All Rights Reserved - International Copyright Secured"*
    ///   * pageNumber
    fn read_page_setup(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()> {
        self.page_setup.page_size.x = read_int(data, seek)?.to_u16_gp("page width")?;
        self.page_setup.page_size.y = read_int(data, seek)?.to_u16_gp("page height")?;
        self.page_setup.page_margin.left = read_int(data, seek)?.to_u16_gp("margin left")?;
        self.page_setup.page_margin.right = read_int(data, seek)?.to_u16_gp("margin right")?;
        self.page_setup.page_margin.top = read_int(data, seek)?.to_u16_gp("margin top")?;
        self.page_setup.page_margin.bottom = read_int(data, seek)?.to_u16_gp("margin bottom")?;
        self.page_setup.score_size_proportion =
            read_int(data, seek)?.to_f32_gp("score size proportion")? / 100.0;
        self.page_setup.header_and_footer =
            read_short(data, seek)?.to_u16_gp("header and footer")?;
        self.page_setup.title = read_int_size_string(data, seek)?;
        self.page_setup.subtitle = read_int_size_string(data, seek)?;
        self.page_setup.artist = read_int_size_string(data, seek)?;
        self.page_setup.album = read_int_size_string(data, seek)?;
        self.page_setup.words = read_int_size_string(data, seek)?;
        self.page_setup.music = read_int_size_string(data, seek)?;
        self.page_setup.word_and_music = read_int_size_string(data, seek)?;
        let mut c = read_int_size_string(data, seek)?;
        c.push('\n');
        c.push_str(&read_int_size_string(data, seek)?);
        self.page_setup.copyright = c;
        self.page_setup.page_number = read_int_size_string(data, seek)?;
        Ok(())
    }

    fn write_page_setup(&self, data: &mut Vec<u8>) -> GpResult<()> {
        write_i32(data, self.page_setup.page_size.x.to_i32_gp("page width")?);
        write_i32(data, self.page_setup.page_size.y.to_i32_gp("page height")?);

        write_i32(
            data,
            self.page_setup.page_margin.left.to_i32_gp("margin left")?,
        );
        write_i32(
            data,
            self.page_setup
                .page_margin
                .right
                .to_i32_gp("margin right")?,
        );
        write_i32(
            data,
            self.page_setup.page_margin.top.to_i32_gp("margin top")?,
        );
        write_i32(
            data,
            self.page_setup
                .page_margin
                .bottom
                .to_i32_gp("margin bottom")?,
        );
        write_i32(
            data,
            (self.page_setup.score_size_proportion * 100f32)
                .ceil()
                .to_i32_gp("score size proportion")?,
        );

        write_i16(
            data,
            i16::from_le_bytes(self.page_setup.header_and_footer.to_le_bytes()),
        );
        write_int_size_string(data, &self.page_setup.title);
        write_int_size_string(data, &self.page_setup.subtitle);
        write_int_size_string(data, &self.page_setup.artist);
        write_int_size_string(data, &self.page_setup.album);
        write_int_size_string(data, &self.page_setup.words);
        write_int_size_string(data, &self.page_setup.music);
        write_int_size_string(data, &self.page_setup.word_and_music);
        // Two lines; GuitarScore fix: a copyright without a line break used to panic.
        let mut lines = self.page_setup.copyright.split('\n');
        write_int_size_string(data, lines.next().unwrap_or_default());
        write_int_size_string(data, lines.next().unwrap_or_default());
        write_int_size_string(data, &self.page_setup.page_number);
        Ok(())
    }
}
