//! Guitar Pro reading and writing: measure.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;
use fraction::ToPrimitive;

pub(crate) trait SongMeasureOps {
    fn read_measures(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()>;
    fn read_measure(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure: &mut Measure,
        track_index: usize,
    ) -> GpResult<()>;
    fn read_measure_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure: &mut Measure,
        track_index: usize,
    ) -> GpResult<()>;
    fn read_voice(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: &mut i64,
        track_index: usize,
    ) -> GpResult<()>;
    fn write_measures(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()>;
    fn write_measure(
        &self,
        data: &mut Vec<u8>,
        track: usize,
        measure: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_voice(
        &self,
        data: &mut Vec<u8>,
        track: usize,
        measure: usize,
        voice: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
}

impl SongMeasureOps for Song {
    /// Read measures. Measures are written in the following order:
    /// - measure 1/track 1
    /// - measure 1/track 2
    /// - ...
    /// - measure 1/track m
    /// - measure 2/track 1
    /// - measure 2/track 2
    /// - ...
    /// - measure 2/track m
    /// - ...
    /// - measure n/track 1
    /// - measure n/track 2
    /// - ...
    /// - measure n/track m
    fn read_measures(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()> {
        for h in 0..self.measure_headers.len() {
            for t in 0..self.tracks.len() {
                self.current_track = Some(t);
                let mut m = Measure {
                    track_index: t,
                    header_index: h,
                    ..Default::default()
                };
                self.current_measure_number = Some(m.number);
                if self.version.number < (5, 0, 0) {
                    self.read_measure(data, seek, &mut m, t)?;
                } else {
                    self.read_measure_v5(data, seek, &mut m, t)?;
                }
                item_mut(&mut self.tracks, t, "track")?.measures.push(m);
            }
        }
        self.current_track = None;
        self.current_measure_number = None;
        Ok(())
    }

    /// Read measure. The measure is written as number of beats followed by sequence of beats.
    fn read_measure(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure: &mut Measure,
        track_index: usize,
    ) -> GpResult<()> {
        let mut voice = Voice::default();
        self.current_voice_number = Some(1);
        self.read_voice(data, seek, &mut voice, &mut measure.start, track_index)?;
        self.current_voice_number = None;
        measure.voices.push(voice);
        Ok(())
    }
    /// Read measure. Guitar Pro 5 stores twice more measures compared to Guitar Pro 3. One measure consists of two sub-measures for each of two voices.
    ///
    /// Sub-measures are followed by a  `LineBreak` stored in `byte`.
    fn read_measure_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure: &mut Measure,
        track_index: usize,
    ) -> GpResult<()> {
        let mut start = measure.start;
        for number in 0..MAX_VOICES {
            self.current_voice_number = Some(number.saturating_add(1));
            let mut voice = Voice::default();
            self.read_voice(data, seek, &mut voice, &mut start, track_index)?;
            measure.voices.push(voice);
        }
        self.current_voice_number = None;
        if *seek < data.len() {
            measure.line_break = get_line_break(read_byte(data, seek)?);
        } else {
            measure.line_break = get_line_break(0);
        }
        Ok(())
    }

    fn read_voice(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: &mut i64,
        track_index: usize,
    ) -> GpResult<()> {
        // Too little data left for the beat count: no beats.
        if seek.saturating_add(4) > data.len() {
            return Ok(());
        }
        let beats = read_int(data, seek)?.to_usize().unwrap_or(0);
        //Sanity check
        if beats > 256 {
            return Ok(());
        }
        for i in 0..beats {
            if seek.saturating_add(5) > data.len() {
                break;
            }
            self.current_beat_number = Some(i.saturating_add(1));
            let length = if self.version.number < (5, 0, 0) {
                self.read_beat(data, seek, voice, *start, track_index)?
            } else {
                self.read_beat_v5(data, seek, voice, &mut *start, track_index)?
            };
            *start = start.saturating_add(length);
        }
        self.current_beat_number = None;
        Ok(())
    }

    fn write_measures(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()> {
        for h in 0..self.measure_headers.len() {
            for t in 0..self.tracks.len() {
                self.write_measure(data, t, h, version)?;
            }
        }
        Ok(())
    }
    fn write_measure(
        &self,
        data: &mut Vec<u8>,
        track: usize,
        measure: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        let the_measure = item(
            &item(&self.tracks, track, "track")?.measures,
            measure,
            "measure",
        )?;
        if version.0 < 5 {
            self.write_voice(data, track, measure, 0, version)?;
        } else {
            for v in 0..the_measure.voices.len() {
                self.write_voice(data, track, measure, v, version)?;
            }
            if version.0 == 5 {
                write_byte(data, from_line_break(&the_measure.line_break));
            }
        }
        Ok(())
    }
    fn write_voice(
        &self,
        data: &mut Vec<u8>,
        track: usize,
        measure: usize,
        voice: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        let the_track = item(&self.tracks, track, "track")?;
        let beats = &item(
            &item(&the_track.measures, measure, "measure")?.voices,
            voice,
            "voice",
        )?
        .beats;
        write_i32(data, beats.len().to_i32_gp("beats count")?);
        for beat in beats {
            if version.0 == 3 {
                self.write_beat_v3(data, beat)?;
            } else {
                self.write_beat(data, beat, &the_track.strings, version)?;
            }
        }
        Ok(())
    }
}
