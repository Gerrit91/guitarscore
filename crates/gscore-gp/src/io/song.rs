//! Guitar Pro reading and writing: song.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

/// GP3-5 files store 64 MIDI channels: 4 ports of 16.
const MIDI_CHANNEL_COUNT: usize = 64;

/// Parse a GPIF `<GPVersion>` string like `"8.1.3"` into a `(major, minor,
/// patch)` tuple. Falls back to `(7, 0, 0)` for unparseable values (the `.gp`
/// container format starts at GP7).
fn parse_gpif_version(s: &str) -> (u8, u8, u8) {
    let parts: Vec<&str> = s.split('.').collect();
    let num = |i: usize| parts.get(i).and_then(|p| p.parse().ok()).unwrap_or(0);
    (num(0).max(7), num(1), num(2))
}

/// The most measures or tracks a binary file may declare: a corrupt count is clamped to
/// this generous ceiling instead of allocating absurd amounts.
const MAX_COUNT: i32 = 100_000;

impl Song {
    /// Read the song. A song consists of score information, triplet feel, tempo, song key, MIDI channels, measure and track count, measure headers, tracks, measures.
    /// - Version: `byte-size-string` of size 30.
    /// - Score information. See `readInfo`.
    /// - Triplet feel: `bool`. If value is true, then triplet feel is set to eigth.
    /// - Tempo: `int`.
    /// - Key: `int`. Key signature of the song.
    /// - MIDI channels. See `readMidiChannels`.
    /// - Number of measures: `int`.
    /// - Number of tracks: `int`.
    /// - Measure headers. See `readMeasureHeaders`.
    /// - Tracks. See `read_tracks()`.
    /// - Measures. See `read_measures()`.
    pub fn read_gp3(&mut self, data: &[u8]) -> GpResult<()> {
        let mut seek: usize = 0;
        self.version = read_version_string(data, &mut seek)?;
        self.read_info(data, &mut seek)?;
        self.triplet_feel = if read_bool(data, &mut seek)? {
            TripletFeel::Eighth
        } else {
            TripletFeel::None
        };
        // Tempo is written as `int` but semantically fits `i16` (real BPM).
        // Garbage in this slot (e.g. GP2 files loaded via the GP3 reader) is
        // truncated rather than rejected.
        self.tempo = clamp_i16(read_int(data, &mut seek)?);
        // Key signature is `-7..=7`; garbage values mean the file was written
        // with an unusual editor. Truncate rather than fail.
        self.key.key = low_i8(read_int(data, &mut seek)?);
        self.read_midi_channels(data, &mut seek)?;
        // Guard against negative counts in malformed files. Cap at a very
        // generous ceiling (100k) so we don't try to allocate absurd amounts
        // for a garbage-in-garbage-out header.
        let measure_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        let track_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        // Read measure headers. The *measures* are written one after another, their number have been specified previously.
        self.read_measure_headers(data, &mut seek, measure_count)?;
        self.current_measure_number = Some(0);
        self.read_tracks(data, &mut seek, track_count)?;
        self.read_measures(data, &mut seek)?;
        Ok(())
    }
    /// Read the song. A song consists of score information, triplet feel, tempo, song key, MIDI channels, measure and track count, measure headers, tracks, measures.
    /// - Version: `byte-size-string` of size 30.
    /// - Score information. See `readInfo`.
    /// - Triplet feel: `bool`. If value is true, then triplet feel is set to eigth.
    /// - Lyrics. See `read_lyrics()`.
    /// - Tempo: `int`.
    /// - Key: `int`. Key signature of the song.
    /// - Octave: `signed-byte`. Reserved for future uses.
    /// - MIDI channels. See `readMidiChannels`.
    /// - Number of measures: `int`.
    /// - Number of tracks: `int`.
    /// - Measure headers. See `readMeasureHeaders`.
    /// - Tracks. See `read_tracks()`.
    /// - Measures. See `read_measures()`.
    pub fn read_gp4(&mut self, data: &[u8]) -> GpResult<()> {
        let mut seek: usize = 0;
        self.version = read_version_string(data, &mut seek)?;
        self.read_clipboard(data, &mut seek)?;
        self.read_info(data, &mut seek)?;
        self.triplet_feel = if read_bool(data, &mut seek)? {
            TripletFeel::Eighth
        } else {
            TripletFeel::None
        };
        self.lyrics = self.read_lyrics(data, &mut seek)?; //read lyrics
        // Tempo is written as `int` but semantically fits `i16` (real BPM).
        // Garbage in this slot (e.g. GP2 files loaded via the GP3 reader) is
        // truncated rather than rejected.
        self.tempo = clamp_i16(read_int(data, &mut seek)?);
        // Key signature is `-7..=7`; garbage values mean the file was written
        // with an unusual editor. Truncate rather than fail.
        self.key.key = low_i8(read_int(data, &mut seek)?);
        read_signed_byte(data, &mut seek)?; //octave
        self.read_midi_channels(data, &mut seek)?;
        // Guard against negative counts in malformed files. Cap at a very
        // generous ceiling (100k) so we don't try to allocate absurd amounts
        // for a garbage-in-garbage-out header.
        let measure_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        let track_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        // Read measure headers. The *measures* are written one after another, their number have been specified previously.
        self.read_measure_headers(data, &mut seek, measure_count)?;
        self.read_tracks(data, &mut seek, track_count)?;
        self.read_measures(data, &mut seek)?;
        Ok(())
    }
    pub fn read_gp5(&mut self, data: &[u8]) -> GpResult<()> {
        let mut seek: usize = 0;
        self.version = read_version_string(data, &mut seek)?;
        self.read_clipboard(data, &mut seek)?;
        self.read_info(data, &mut seek)?;
        self.lyrics = self.read_lyrics(data, &mut seek)?; //read lyrics
        self.master_effect = self.read_rse_master_effect(data, &mut seek)?;
        self.read_page_setup(data, &mut seek)?;
        self.tempo_name = read_int_size_string(data, &mut seek)?;
        // Tempo is written as `int` but semantically fits `i16` (real BPM).
        // Garbage in this slot (e.g. GP2 files loaded via the GP3 reader) is
        // truncated rather than rejected.
        self.tempo = clamp_i16(read_int(data, &mut seek)?);
        self.hide_tempo = if self.version.number > (5, 0, 0) {
            read_bool(data, &mut seek)?
        } else {
            false
        };
        self.key.key = read_signed_byte(data, &mut seek)?;
        read_int(data, &mut seek)?; //octave
        self.read_midi_channels(data, &mut seek)?;
        let directions = self.read_directions(data, &mut seek)?;
        self.master_effect.reverb = read_int(data, &mut seek)?.to_f32_gp("reverb")?;
        // Guard against negative counts in malformed files. Cap at a very
        // generous ceiling (100k) so we don't try to allocate absurd amounts
        // for a garbage-in-garbage-out header.
        let measure_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        let track_count = clamp_count(read_int(data, &mut seek)?, MAX_COUNT);
        self.read_measure_headers_v5(data, &mut seek, measure_count, &directions)?;
        self.read_tracks_v5(data, &mut seek, track_count)?;
        self.read_measures(data, &mut seek)?;
        Ok(())
    }
    /// Read Guitar Pro 7+ file (.gp)
    pub fn read_gp(&mut self, data: &[u8]) -> GpResult<()> {
        use crate::io::gpx::read_gp_with_audio;
        let (gpif, audio) = read_gp_with_audio(data)?;
        self.backing_track_audio = audio;
        // The ZIP `VERSION` marker stays "7.0" for both GP7 and GP8; the GPIF
        // declares the true format version (e.g. "8.1.3" for GP8).
        self.version = match gpif.version.as_deref().map(parse_gpif_version) {
            Some(n) => Version {
                data: gpif.version.clone().unwrap_or_default(),
                number: n,
                clipboard: false,
            },
            None => Version {
                data: String::from("7.0"),
                number: (7, 0, 0),
                clipboard: false,
            },
        };
        self.read_gpif(&gpif);
        Ok(())
    }
    /// Read Guitar Pro 6 file (.gpx)
    pub fn read_gpx(&mut self, data: &[u8]) -> GpResult<()> {
        use crate::io::gpx::read_gpx;
        let gpif = read_gpx(data)?;
        self.version.number = (6, 0, 0);
        self.read_gpif(&gpif);
        Ok(())
    }
    /// Write this song as a Guitar Pro 6 (.gpx) binary.
    pub fn write_gpx(&self) -> GpResult<Vec<u8>> {
        gpx::write_gpx_bytes(self)
    }
    /// Write this song as a Guitar Pro 7+ (.gp) binary.
    pub fn write_gp(&self) -> GpResult<Vec<u8>> {
        gpx::write_gp_bytes(self)
    }
    /// Read information (name, artist, ...)
    pub(crate) fn read_info(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()> {
        self.name = read_int_byte_size_string(data, seek)?; //.replace("\r", " ").replace("\n", " ").trim().to_owned();
        self.subtitle = read_int_byte_size_string(data, seek)?;
        self.artist = read_int_byte_size_string(data, seek)?;
        self.album = read_int_byte_size_string(data, seek)?;
        self.words = read_int_byte_size_string(data, seek)?; //music
        self.author = if self.version.number.0 < 5 {
            self.words.clone()
        } else {
            read_int_byte_size_string(data, seek)?
        };
        self.copyright = read_int_byte_size_string(data, seek)?;
        self.writer = read_int_byte_size_string(data, seek)?; //tabbed by
        self.instructions = read_int_byte_size_string(data, seek)?; //instructions
        //notices
        // Guard against negative / absurd counts in malformed files. Cap to a
        // sensible upper bound so a corrupt count doesn't allocate wildly or
        // panic.
        let nc = clamp_count(read_int(data, seek)?, 10_000);
        for _ in 0..nc {
            self.notice.push(read_int_byte_size_string(data, seek)?);
        }
        Ok(())
    }

    /// Write data to a Vec<u8>, you are free to use the encoded data to write it in a file or in a database or do something else.
    ///
    /// GuitarScore fix: GP3-5 files need a table of 64 MIDI channels, which songs read from
    /// GP6/GP7 do not have (writing used to panic, or would have produced an unreadable
    /// file). Such songs are written from a copy with a generated table.
    pub fn write(&self, version: (u8, u8, u8), clipboard: Option<bool>) -> GpResult<Vec<u8>> {
        if self.channels.len() >= MIDI_CHANNEL_COUNT {
            return self.write_with_channel_table(version, clipboard);
        }
        let mut song = self.clone();
        song.generate_channel_table();
        song.write_with_channel_table(version, clipboard)
    }

    /// GuitarScore addition: replaces the channel table with 64 channels (4 ports of 16)
    /// and assigns one channel per track, skipping channel 10 (index 9), which drum tracks
    /// get. Instruments come from the GP6/GP7 MIDI program where known.
    pub(crate) fn generate_channel_table(&mut self) {
        self.channels = (0..MIDI_CHANNEL_COUNT)
            .map(|i| {
                let channel =
                    u8::try_from(i.checked_rem(16).unwrap_or_default()).unwrap_or_default();
                MidiChannel {
                    channel,
                    effect_channel: channel,
                    ..MidiChannel::default()
                }
            })
            .collect();
        let mut next = 0usize;
        for track in &mut self.tracks {
            let percussion = usize::from(DEFAULT_PERCUSSION_CHANNEL);
            let index = if track.percussion_track {
                percussion
            } else {
                if next.checked_rem(16) == Some(percussion) {
                    next = next.saturating_add(1);
                }
                let index = next.min(MIDI_CHANNEL_COUNT.saturating_sub(1));
                next = next.saturating_add(1);
                index
            };
            track.channel_index = index;
            if let Some(channel) = self.channels.get_mut(index) {
                if track.percussion_track {
                    channel.instrument = 0;
                } else if let Some(program) = track.midi_program_gpif {
                    channel.instrument = program;
                }
            }
        }
    }

    fn write_with_channel_table(
        &self,
        version: (u8, u8, u8),
        clipboard: Option<bool>,
    ) -> GpResult<Vec<u8>> {
        let mut data: Vec<u8> = Vec::with_capacity(8388608); //capacity of 8MB, should be sufficient
        write_version(&mut data, version);
        if clipboard.is_some_and(|c| c) && version.0 >= 4 {
            self.write_clipboard(&mut data, &version)?;
        }
        self.write_info(&mut data, version)?;
        if version.0 < 5 {
            write_bool(&mut data, self.triplet_feel != TripletFeel::None);
        }
        if version.0 >= 4 {
            self.write_lyrics(&mut data)?;
        }
        if version > (5, 0, 0) {
            self.write_rse_master_effect(&mut data);
        }
        if version.0 >= 5 {
            self.write_page_setup(&mut data)?;
            write_int_size_string(&mut data, &self.tempo_name);
        }
        write_i32(&mut data, self.tempo.to_i32_gp("tempo")?);
        if version > (5, 0, 0) {
            write_bool(&mut data, self.hide_tempo);
        }
        write_i32(&mut data, self.key.key.to_i32_gp("key signature")?);

        if version.0 >= 4 {
            write_signed_byte(&mut data, 0);
        } //octave
        self.write_midi_channels(&mut data);

        if version.0 == 5 {
            self.write_directions(&mut data)?;
            self.write_master_reverb(&mut data);
        }

        let measures = self.tracks.first().map_or(0, |track| track.measures.len());
        write_i32(&mut data, measures.to_i32_gp("measures count")?);
        write_i32(&mut data, self.tracks.len().to_i32_gp("tracks count")?);
        self.write_measure_headers(&mut data, &version)?;
        self.write_tracks(&mut data, &version)?;
        self.write_measures(&mut data, &version)?;
        write_i32(&mut data, 0);
        Ok(data)
    }
    pub(crate) fn write_info(&self, data: &mut Vec<u8>, version: (u8, u8, u8)) -> GpResult<()> {
        write_int_byte_size_string(data, &self.name);
        write_int_byte_size_string(data, &self.subtitle);
        write_int_byte_size_string(data, &self.artist);
        write_int_byte_size_string(data, &self.album);
        if version.0 < 5 {
            write_int_byte_size_string(data, &self.pack_author());
        } else {
            write_int_byte_size_string(data, &self.words);
            write_int_byte_size_string(data, &self.author);
        }
        write_int_byte_size_string(data, &self.copyright);
        write_int_byte_size_string(data, &self.writer);
        write_int_byte_size_string(data, &self.instructions);
        write_i32(data, self.notice.len().to_i32_gp("notice count")?);
        for notice in &self.notice {
            write_int_byte_size_string(data, notice);
        }
        Ok(())
    }
    pub(crate) fn pack_author(&self) -> String {
        if !self.words.is_empty() && !self.author.is_empty() {
            if self.words != self.author {
                let mut s = self.words.clone();
                s.push_str(", ");
                s.push_str(&self.author);
                s
            } else {
                self.words.clone()
            }
        } else {
            let mut s = self.words.clone();
            s.push_str(&self.author);
            s
        }
    }
}
