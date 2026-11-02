//! Guitar Pro reading and writing: note.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod read;
mod write;
use crate::io::*;

impl Note {
    pub(crate) fn real_value(&self, strings: &[(i8, i8)]) -> GpResult<i8> {
        use crate::io::error::ToPrimitiveGp;
        if self.string > 0 {
            let value = self.value.to_i8_gp("note value")?;
            let string_idx = self.string.to_usize_gp("string index")? - 1;
            return Ok(value + strings[string_idx].1);
        }
        Err(GpError::InvalidValue {
            context: "note real_value (string must be > 0)",
            value: self.string as i64,
        })
    }
}

pub(crate) trait SongNoteOps {
    fn read_notes(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        track_index: usize,
        beat: &mut Beat,
        duration: &Duration,
        note_effect: NoteEffect,
    ) -> GpResult<()>;
    fn read_note(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
        guitar_string: (i8, i8),
        track_index: usize,
    ) -> GpResult<()>;
    fn read_note_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
        guitar_string: (i8, i8),
        track_index: usize,
    ) -> GpResult<()>;
    fn read_note_effects_v3(&self, data: &[u8], seek: &mut usize, note: &mut Note) -> GpResult<()>;
    fn read_note_effects_v4(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
    ) -> GpResult<()>;
    fn get_tied_note_value(&self, string_index: i8, track_index: usize) -> i16;
    fn write_notes(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_note_v3(&self, data: &mut Vec<u8>, note: &Note) -> GpResult<()>;
    fn write_note_v4(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_note_v5(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_note_effects_v3(&self, data: &mut Vec<u8>, note: &Note) -> GpResult<()>;
    fn write_note_effects(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
}

impl SongNoteOps for Song {
    fn read_notes(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        track_index: usize,
        beat: &mut Beat,
        duration: &Duration,
        note_effect: NoteEffect,
    ) -> GpResult<()> {
        read::read_notes(self, data, seek, track_index, beat, duration, note_effect)
    }
    fn read_note(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
        guitar_string: (i8, i8),
        track_index: usize,
    ) -> GpResult<()> {
        read::read_note(self, data, seek, note, guitar_string, track_index)
    }
    fn read_note_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
        guitar_string: (i8, i8),
        track_index: usize,
    ) -> GpResult<()> {
        read::read_note_v5(self, data, seek, note, guitar_string, track_index)
    }
    fn read_note_effects_v3(&self, data: &[u8], seek: &mut usize, note: &mut Note) -> GpResult<()> {
        read::read_note_effects_v3(self, data, seek, note)
    }
    fn read_note_effects_v4(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        note: &mut Note,
    ) -> GpResult<()> {
        read::read_note_effects_v4(self, data, seek, note)
    }
    fn get_tied_note_value(&self, string_index: i8, track_index: usize) -> i16 {
        read::get_tied_note_value(self, string_index, track_index)
    }
    fn write_notes(
        &self,
        data: &mut Vec<u8>,
        beat: &Beat,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_notes(self, data, beat, strings, version)
    }
    fn write_note_v3(&self, data: &mut Vec<u8>, note: &Note) -> GpResult<()> {
        write::write_note_v3(self, data, note)
    }
    fn write_note_v4(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_note_v4(self, data, note, strings, version)
    }
    fn write_note_v5(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_note_v5(self, data, note, strings, version)
    }
    fn write_note_effects_v3(&self, data: &mut Vec<u8>, note: &Note) -> GpResult<()> {
        write::write_note_effects_v3(self, data, note)
    }
    fn write_note_effects(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        write::write_note_effects(self, data, note, strings, version)
    }
}
