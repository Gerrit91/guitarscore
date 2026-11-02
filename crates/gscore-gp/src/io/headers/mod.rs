//! Guitar Pro reading and writing: measure headers, markers, repeats, directions and the
//! clipboard header. Writing is in [`write`].

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod write;

use std::collections::HashMap;

use crate::io::*;

pub(crate) fn read_marker(data: &[u8], seek: &mut usize) -> GpResult<Marker> {
    use crate::io::primitive::*;
    let mut marker = Marker {
        title: read_int_size_string(data, seek)?,
        ..Default::default()
    };
    marker.color = read_color(data, seek)?;
    Ok(marker)
}

pub(crate) trait SongHeaderOps {
    fn read_clipboard(&mut self, data: &[u8], seek: &mut usize) -> GpResult<Option<Clipboard>>;
    fn read_measure_headers(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure_count: usize,
    ) -> GpResult<()>;
    fn read_measure_headers_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure_count: usize,
        directions: &(HashMap<DirectionSign, i16>, HashMap<DirectionSign, i16>),
    ) -> GpResult<()>;
    fn read_measure_header(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        number: usize,
        previous: Option<MeasureHeader>,
    ) -> GpResult<(MeasureHeader, u8)>;
    fn read_measure_header_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        number: usize,
        previous: Option<MeasureHeader>,
    ) -> GpResult<(MeasureHeader, u8)>;
    fn read_repeat_alternative(&mut self, data: &[u8], seek: &mut usize) -> GpResult<u8>;
    fn read_repeat_alternative_v5(&mut self, data: &[u8], seek: &mut usize) -> GpResult<u8>;
    fn read_directions(
        &self,
        data: &[u8],
        seek: &mut usize,
    ) -> GpResult<(HashMap<DirectionSign, i16>, HashMap<DirectionSign, i16>)>;
    fn write_measure_headers(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()>;
    fn write_measure_header(
        &self,
        data: &mut Vec<u8>,
        header: usize,
        previous: Option<usize>,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
    fn write_clipboard(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()>;
    fn write_directions(&self, data: &mut Vec<u8>) -> GpResult<()>;
}

impl SongHeaderOps for Song {
    fn read_clipboard(&mut self, data: &[u8], seek: &mut usize) -> GpResult<Option<Clipboard>> {
        if !self.version.clipboard {
            return Ok(None);
        }
        let mut c = Clipboard {
            start_measure: read_int(data, seek)?,
            ..Default::default()
        };
        c.stop_measure = read_int(data, seek)?;
        c.start_track = read_int(data, seek)?;
        c.stop_track = read_int(data, seek)?;
        if self.version.number.0 == 5 {
            c.start_beat = read_int(data, seek)?;
            c.stop_beat = read_int(data, seek)?;
            c.sub_bar_copy = read_int(data, seek)? != 0;
        }
        Ok(Some(c))
    }

    fn read_measure_headers(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure_count: usize,
    ) -> GpResult<()> {
        let mut previous: Option<MeasureHeader> = None;
        for i in 1..=measure_count {
            let r: (MeasureHeader, u8) = self.read_measure_header(data, seek, i, previous)?;
            previous = Some(r.0.clone());
            self.measure_headers.push(r.0);
        }
        Ok(())
    }

    fn read_measure_headers_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        measure_count: usize,
        directions: &(HashMap<DirectionSign, i16>, HashMap<DirectionSign, i16>),
    ) -> GpResult<()> {
        let mut previous: Option<MeasureHeader> = None;
        for i in 1..=measure_count {
            let r: (MeasureHeader, u8) = self.read_measure_header_v5(data, seek, i, previous)?;
            previous = Some(r.0.clone());
            self.measure_headers.push(r.0);
        }
        // GuitarScore fix: the measure number comes from the file; an out-of-range number
        // (or 0, which underflowed) used to panic. It is now a format error.
        for s in directions.0.iter().chain(&directions.1) {
            if s.1 > &-1 {
                let number = s.1.to_usize_gp("direction measure index")?;
                let header = number
                    .checked_sub(1)
                    .and_then(|index| self.measure_headers.get_mut(index))
                    .ok_or_else(|| {
                        GpError::FormatError(format!(
                            "direction points to measure {number}, but there are {} measures",
                            measure_count
                        ))
                    })?;
                header.direction = Some(s.0.clone());
            }
        }
        Ok(())
    }

    fn read_measure_header(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        number: usize,
        previous: Option<MeasureHeader>,
    ) -> GpResult<(MeasureHeader, u8)> {
        let flag = read_byte(data, seek)?;
        let mut mh = MeasureHeader {
            number: number.to_u16_gp("measure number")?,
            ..Default::default()
        };
        mh.start = 0;
        mh.triplet_feel = self.triplet_feel.clone();
        if (flag & 0x01) == 0x01 {
            mh.time_signature.numerator = read_signed_byte(data, seek)?;
        } else if number > 1 {
            mh.time_signature.numerator = previous
                .clone()
                .ok_or(GpError::MissingState {
                    field: "previous_measure_header",
                })?
                .time_signature
                .numerator;
        }
        if (flag & 0x02) == 0x02 {
            mh.time_signature.denominator.value =
                read_signed_byte(data, seek)?.to_u16_gp("time_signature denominator value")?;
        } else if number > 1 {
            mh.time_signature.denominator = previous
                .clone()
                .ok_or(GpError::MissingState {
                    field: "previous_measure_header",
                })?
                .time_signature
                .denominator;
        }

        mh.repeat_open = (flag & 0x04) == 0x04;
        if (flag & 0x08) == 0x08 {
            mh.repeat_close = read_signed_byte(data, seek)?;
        }
        if (flag & 0x10) == 0x10 {
            mh.repeat_alternative = if self.version.number.0 == 5 {
                self.read_repeat_alternative_v5(data, seek)?
            } else {
                self.read_repeat_alternative(data, seek)?
            };
        }
        if (flag & 0x20) == 0x20 {
            mh.marker = Some(read_marker(data, seek)?);
        }
        if (flag & 0x40) == 0x40 {
            mh.key_signature.key = read_signed_byte(data, seek)?;
            mh.key_signature.is_minor = read_signed_byte(data, seek)? != 0;
        } else if mh.number > 1 {
            mh.key_signature = previous
                .ok_or(GpError::MissingState {
                    field: "previous_measure_header",
                })?
                .key_signature;
        }
        mh.double_bar = (flag & 0x80) == 0x80;
        Ok((mh, flag))
    }

    fn read_measure_header_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        number: usize,
        previous: Option<MeasureHeader>,
    ) -> GpResult<(MeasureHeader, u8)> {
        if previous.is_some() {
            skip(seek, 1);
        }
        let r = self.read_measure_header(data, seek, number, previous.clone())?;
        let mut mh = r.0;
        let flags = r.1;
        if mh.repeat_close > -1 {
            mh.repeat_close = mh.repeat_close.saturating_sub(1);
        }
        // GP5 stores beams whenever the time signature changes at all (either the
        // numerator OR the denominator bit is set), not only when both are set.
        // Matches the Python `guitarpro` reference and TuxGuitar.
        if (flags & 0x03) != 0 {
            for beam in &mut mh.time_signature.beams {
                *beam = read_byte(data, seek)?;
            }
        } else {
            mh.time_signature.beams = previous
                .ok_or(GpError::MissingState {
                    field: "previous_measure_header",
                })?
                .time_signature
                .beams;
        };
        if (flags & 0x10) == 0 {
            skip(seek, 1);
        }
        mh.triplet_feel = get_triplet_feel(read_byte(data, seek)?);
        Ok((mh, flags))
    }

    fn read_repeat_alternative(&mut self, data: &[u8], seek: &mut usize) -> GpResult<u8> {
        // Guitar Pro encodes repeat alternatives as a bitmask in a single byte,
        // so `value` should stay in `1..=8`. Some exports emit larger numbers
        // (spec says "1st/2nd/…" but the file may carry a raw ordinal). Clamp
        // to 8 so `(1 << value) - 1` doesn't overflow a u16 and so the result
        // still fits `u8`.
        let raw = read_byte(data, seek)?;
        let value = u16::from(raw).min(8);
        // The endings of the repeat so far: back to its start.
        let existing_alternative = self
            .measure_headers
            .iter()
            .rev()
            .take_while(|header| !header.repeat_open)
            .fold(0u16, |bits, header| {
                bits | u16::from(header.repeat_alternative)
            });
        let endings = (1u16 << value).saturating_sub(1) ^ existing_alternative;
        Ok(u8::try_from(endings).unwrap_or(u8::MAX))
    }

    fn read_repeat_alternative_v5(&mut self, data: &[u8], seek: &mut usize) -> GpResult<u8> {
        read_byte(data, seek)
    }

    fn read_directions(
        &self,
        data: &[u8],
        seek: &mut usize,
    ) -> GpResult<(HashMap<DirectionSign, i16>, HashMap<DirectionSign, i16>)> {
        let mut signs: HashMap<DirectionSign, i16> = HashMap::with_capacity(4);
        let mut from_signs: HashMap<DirectionSign, i16> = HashMap::with_capacity(15);
        signs.insert(DirectionSign::Coda, read_short(data, seek)?);
        signs.insert(DirectionSign::DoubleCoda, read_short(data, seek)?);
        signs.insert(DirectionSign::Segno, read_short(data, seek)?);
        signs.insert(DirectionSign::SegnoSegno, read_short(data, seek)?);
        signs.insert(DirectionSign::Fine, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaCapo, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaCapoAlCoda, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaCapoAlDoubleCoda, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaCapoAlFine, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegno, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegnoAlCoda, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegnoAlDoubleCoda, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegnoAlFine, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegnoSegno, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaSegnoSegnoAlCoda, read_short(data, seek)?);
        from_signs.insert(
            DirectionSign::DaSegnoSegnoAlDoubleCoda,
            read_short(data, seek)?,
        );
        from_signs.insert(DirectionSign::DaSegnoSegnoAlFine, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaCoda, read_short(data, seek)?);
        from_signs.insert(DirectionSign::DaDoubleCoda, read_short(data, seek)?);
        Ok((signs, from_signs))
    }

    fn write_measure_headers(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()> {
        let mut previous: Option<usize> = None;
        for i in 0..self.measure_headers.len() {
            self.write_measure_header(data, i, previous, version)?;
            previous = Some(i);
        }
        Ok(())
    }

    fn write_measure_header(
        &self,
        data: &mut Vec<u8>,
        header: usize,
        previous: Option<usize>,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        let current = item(&self.measure_headers, header, "measure header")?;
        let previous = previous
            .map(|p| item(&self.measure_headers, p, "measure header"))
            .transpose()?;
        write::measure_header(data, current, previous, version)
    }

    fn write_clipboard(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()> {
        write::clipboard(data, self.clipboard.as_ref(), version)
    }

    fn write_directions(&self, data: &mut Vec<u8>) -> GpResult<()> {
        write::directions(data, &self.measure_headers)
    }
}
