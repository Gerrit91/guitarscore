//! Guitar Pro reading and writing: beat.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod read;
mod write;
use crate::io::*;

pub trait SongBeatOps {
    fn read_beat(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: i64,
        track_index: usize,
    ) -> GpResult<i64>;
    fn read_beat_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: &mut i64,
        track_index: usize,
    ) -> GpResult<i64>;
    fn read_beat_effects_v3(
        &self,
        data: &[u8],
        seek: &mut usize,
        note_effect: &mut NoteEffect,
    ) -> GpResult<BeatEffects>;
    fn read_beat_effects_v4(&self, data: &[u8], seek: &mut usize) -> GpResult<BeatEffects>;
    fn read_beat_stroke(&self, data: &[u8], seek: &mut usize) -> GpResult<BeatStroke>;
    fn read_tremolo_bar(&self, data: &[u8], seek: &mut usize) -> GpResult<BendEffect>;
    fn write_beat_v3(&self, data: &mut Vec<u8>, beat: &Beat) -> GpResult<()>;
    fn write_beat(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_beat_effect_v3(&self, data: &mut Vec<u8>, beat: &Beat) -> GpResult<()>;
    fn write_beat_effect_v4(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_tremolo_bar(&self, data: &mut Vec<u8>, bar: &Option<BendEffect>) -> GpResult<()>;
    fn write_beat_stroke(
        &self,
        data: &mut Vec<u8>,
        stroke: &BeatStroke,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
}

impl SongBeatOps for Song {
    fn read_beat(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: i64,
        track_index: usize,
    ) -> GpResult<i64> {
        read::read_beat(self, data, seek, voice, start, track_index)
    }
    fn read_beat_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        voice: &mut Voice,
        start: &mut i64,
        track_index: usize,
    ) -> GpResult<i64> {
        read::read_beat_v5(self, data, seek, voice, start, track_index)
    }
    fn read_beat_effects_v3(
        &self,
        data: &[u8],
        seek: &mut usize,
        note_effect: &mut NoteEffect,
    ) -> GpResult<BeatEffects> {
        read::read_beat_effects_v3(self, data, seek, note_effect)
    }
    fn read_beat_effects_v4(&self, data: &[u8], seek: &mut usize) -> GpResult<BeatEffects> {
        read::read_beat_effects_v4(self, data, seek)
    }
    fn read_beat_stroke(&self, data: &[u8], seek: &mut usize) -> GpResult<BeatStroke> {
        read::read_beat_stroke(self, data, seek)
    }
    fn read_tremolo_bar(&self, data: &[u8], seek: &mut usize) -> GpResult<BendEffect> {
        read::read_tremolo_bar(self, data, seek)
    }
    fn write_beat_v3(&self, data: &mut Vec<u8>, beat: &Beat) -> GpResult<()> {
        write::write_beat_v3(self, data, beat)
    }
    fn write_beat(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_beat(self, data, beat, strings, version)
    }
    fn write_beat_effect_v3(&self, data: &mut Vec<u8>, beat: &Beat) -> GpResult<()> {
        write::write_beat_effect_v3(self, data, beat)
    }
    fn write_beat_effect_v4(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_beat_effect_v4(self, data, beat, version)
    }
    fn write_tremolo_bar(&self, data: &mut Vec<u8>, bar: &Option<BendEffect>) -> GpResult<()> {
        write::write_tremolo_bar(self, data, bar)
    }
    fn write_beat_stroke(
        &self,
        data: &mut Vec<u8>,
        stroke: &BeatStroke,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_beat_stroke(self, data, stroke, version)
    }
}
