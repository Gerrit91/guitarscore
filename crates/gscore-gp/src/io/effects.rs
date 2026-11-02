//! Guitar Pro reading and writing: effects.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;
use fraction::ToPrimitive;

/// Convert Guitar Pro dynamic value to raw MIDI velocity
pub(crate) fn unpack_velocity(v: i16) -> i16 {
    MIN_VELOCITY
        .saturating_add(VELOCITY_INCREMENT.saturating_mul(v))
        .saturating_sub(VELOCITY_INCREMENT)
}

/// Convert raw MIDI velocity to the Guitar Pro dynamic value.
pub(crate) fn pack_velocity(velocity: i16) -> GpResult<i8> {
    let steps = velocity
        .saturating_add(VELOCITY_INCREMENT)
        .saturating_sub(MIN_VELOCITY);
    (f32::from(steps) / f32::from(VELOCITY_INCREMENT))
        .ceil()
        .to_i8_gp("pack_velocity")
}

/// A grace note's length from its stored exponent: 128th (7) up to whole (0); anything
/// else counts as a 128th.
fn grace_duration(exponent: u8) -> u8 {
    7_u8.checked_sub(exponent)
        .and_then(|shift| 1_u8.checked_shl(u32::from(shift)))
        .unwrap_or(1)
}

/// The exponent [`grace_duration`] reads back.
fn grace_exponent(duration: u8) -> u8 {
    u8::try_from(duration.leading_zeros()).unwrap_or(8)
}

pub(crate) fn from_tremolo_value(value: i8) -> GpResult<u8> {
    match value {
        1 => Ok(DURATION_EIGHTH),
        2 => Ok(DURATION_SIXTEENTH),
        3 => Ok(DURATION_THIRTY_SECOND),
        _ => Ok(DURATION_SIXTEENTH),
    }
}

pub(crate) fn from_trill_period(period: i8) -> GpResult<u16> {
    match period {
        1 => Ok(u16::from(DURATION_SIXTEENTH)),
        2 => Ok(u16::from(DURATION_THIRTY_SECOND)),
        3 => Ok(u16::from(DURATION_SIXTY_FOURTH)),
        _ => Ok(u16::from(DURATION_SIXTEENTH)),
    }
}

pub(crate) trait SongEffectOps {
    fn read_bend_effect(&self, data: &[u8], seek: &mut usize) -> GpResult<Option<BendEffect>>;
    fn read_grace_effect(&self, data: &[u8], seek: &mut usize) -> GpResult<GraceEffect>;
    fn read_grace_effect_v5(&self, data: &[u8], seek: &mut usize) -> GpResult<GraceEffect>;
    fn read_tremolo_picking(&self, data: &[u8], seek: &mut usize)
    -> GpResult<TremoloPickingEffect>;
    fn read_slides_v5(&self, data: &[u8], seek: &mut usize) -> GpResult<Vec<SlideType>>;
    fn read_harmonic(&self, data: &[u8], seek: &mut usize, note: &Note)
    -> GpResult<HarmonicEffect>;
    fn read_harmonic_v5(&mut self, data: &[u8], seek: &mut usize) -> GpResult<HarmonicEffect>;
    fn read_trill(&self, data: &[u8], seek: &mut usize) -> GpResult<TrillEffect>;
    // write methods
    fn write_bend(&self, data: &mut Vec<u8>, bend: &Option<BendEffect>) -> GpResult<()>;
    fn write_grace(&self, data: &mut Vec<u8>, grace: &Option<GraceEffect>) -> GpResult<()>;
    fn write_grace_v5(&self, data: &mut Vec<u8>, grace: &Option<GraceEffect>) -> GpResult<()>;
    fn write_harmonic(&self, data: &mut Vec<u8>, note: &Note, strings: &[(i8, i8)])
    -> GpResult<()>;
    fn write_harmonic_v5(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
    ) -> GpResult<()>;
    fn write_slides_v5(&self, data: &mut Vec<u8>, slides: &[SlideType]);
}

impl SongEffectOps for Song {
    /// Read a bend. It is encoded as:
    /// - Bend type: `signed-byte`. See BendType.
    /// - Bend value: `int`.
    /// - Number of bend points: `int`.
    /// - List of points. Each point consists of:
    ///   * Position: `int`. Shows where point is set along *x*-axis.
    ///   * Value: `int`. Shows where point is set along *y*-axis.
    ///   * Vibrato: `bool`.
    fn read_bend_effect(&self, data: &[u8], seek: &mut usize) -> GpResult<Option<BendEffect>> {
        let mut be = BendEffect {
            kind: get_bend_type(read_signed_byte(data, seek)?)?,
            ..Default::default()
        };
        be.value = read_int(data, seek)?.to_i16().unwrap_or(0);
        let count: u8 = read_int(data, seek)?.to_u8().unwrap_or(0);
        for _ in 0..count {
            let mut bp = BendPoint {
                position: (f32::from(read_int(data, seek)?.to_i16().unwrap_or(0))
                    * f32::from(BEND_EFFECT_MAX_POSITION)
                    / GP_BEND_POSITION)
                    .round()
                    .to_u8()
                    .unwrap_or(0),
                ..Default::default()
            };
            bp.value = (f32::from(read_int(data, seek)?.to_i16().unwrap_or(0))
                * f32::from(be.semitone_length)
                / GP_BEND_SEMITONE)
                .round()
                .to_i8()
                .unwrap_or(0);
            bp.vibrato = read_bool(data, seek)?;
            be.points.push(bp);
        }
        if count > 0 { Ok(Some(be)) } else { Ok(None) }
    }

    fn read_grace_effect(&self, data: &[u8], seek: &mut usize) -> GpResult<GraceEffect> {
        let mut g = GraceEffect {
            fret: read_signed_byte(data, seek)?,
            ..Default::default()
        };
        g.velocity = unpack_velocity(i16::from(read_byte(data, seek)?));
        // Duration byte is expected in `0..=7`; garbage above that used to underflow `u8`
        // in `7 - byte`.
        g.duration = grace_duration(read_byte(data, seek)?);
        g.is_dead = g.fret == -1;
        g.transition = get_grace_effect_transition(read_signed_byte(data, seek)?)?;
        Ok(g)
    }

    fn read_grace_effect_v5(&self, data: &[u8], seek: &mut usize) -> GpResult<GraceEffect> {
        let mut g = GraceEffect {
            fret: byte_as_i8(read_byte(data, seek)?),
            ..Default::default()
        };
        g.velocity = unpack_velocity(i16::from(read_byte(data, seek)?));
        g.transition = get_grace_effect_transition(byte_as_i8(read_byte(data, seek)?))?;
        g.duration = grace_duration(read_byte(data, seek)?);
        let flags = read_byte(data, seek)?;
        g.is_dead = (flags & 0x01) == 0x01;
        g.is_on_beat = (flags & 0x02) == 0x02;
        Ok(g)
    }

    fn read_tremolo_picking(
        &self,
        data: &[u8],
        seek: &mut usize,
    ) -> GpResult<TremoloPickingEffect> {
        let mut tp = TremoloPickingEffect::default();
        tp.duration.value = u16::from(from_tremolo_value(read_signed_byte(data, seek)?)?);
        Ok(tp)
    }

    fn read_slides_v5(&self, data: &[u8], seek: &mut usize) -> GpResult<Vec<SlideType>> {
        let t = read_byte(data, seek)?;
        let mut v: Vec<SlideType> = Vec::with_capacity(6);
        if (t & 0x01) == 0x01 {
            v.push(SlideType::ShiftSlideTo);
        }
        if (t & 0x02) == 0x02 {
            v.push(SlideType::LegatoSlideTo);
        }
        if (t & 0x04) == 0x04 {
            v.push(SlideType::OutDownwards);
        }
        if (t & 0x08) == 0x08 {
            v.push(SlideType::OutUpWards);
        }
        if (t & 0x10) == 0x10 {
            v.push(SlideType::IntoFromBelow);
        }
        if (t & 0x20) == 0x20 {
            v.push(SlideType::IntoFromAbove);
        }
        Ok(v)
    }

    fn read_harmonic(
        &self,
        data: &[u8],
        seek: &mut usize,
        note: &Note,
    ) -> GpResult<HarmonicEffect> {
        let mut he = HarmonicEffect::default();
        match read_signed_byte(data, seek)? {
            1 => he.kind = HarmonicType::Natural,
            3 => he.kind = HarmonicType::Tapped,
            4 => he.kind = HarmonicType::Pinch,
            5 => he.kind = HarmonicType::Semi,
            15 => {
                let pitch = note.value.saturating_add(7).wrapping_rem(12);
                he.pitch = Some(PitchClass::from(
                    i8::try_from(pitch).unwrap_or_default(),
                    None,
                    None,
                ));
                he.octave = Some(Octave::Ottava);
                he.kind = HarmonicType::Artificial;
            }
            17 => {
                let track_idx = self.current_track.ok_or(GpError::MissingState {
                    field: "current_track",
                })?;
                he.pitch = Some(PitchClass::from(
                    note.real_value(&item(&self.tracks, track_idx, "track")?.strings)?,
                    None,
                    None,
                ));
                he.octave = Some(Octave::Quindicesima);
                he.kind = HarmonicType::Artificial;
            }
            22 => {
                let track_idx = self.current_track.ok_or(GpError::MissingState {
                    field: "current_track",
                })?;
                he.pitch = Some(PitchClass::from(
                    note.real_value(&item(&self.tracks, track_idx, "track")?.strings)?,
                    None,
                    None,
                ));
                he.octave = Some(Octave::Ottava);
                he.kind = HarmonicType::Artificial;
            }
            _ => he.kind = HarmonicType::Natural,
        };
        Ok(he)
    }

    fn read_harmonic_v5(&mut self, data: &[u8], seek: &mut usize) -> GpResult<HarmonicEffect> {
        let mut he = HarmonicEffect::default();
        match read_signed_byte(data, seek)? {
            1 => he.kind = HarmonicType::Natural,
            2 => {
                he.kind = HarmonicType::Artificial;
                let semitone = byte_as_i8(read_byte(data, seek)?);
                let accidental = read_signed_byte(data, seek)?;
                he.pitch = Some(PitchClass::from(semitone, Some(accidental), None));
                he.octave = Some(get_octave(read_byte(data, seek)?)?);
            }
            3 => {
                he.kind = HarmonicType::Tapped;
                he.fret = Some(byte_as_i8(read_byte(data, seek)?));
            }
            4 => he.kind = HarmonicType::Pinch,
            5 => he.kind = HarmonicType::Semi,
            _ => he.kind = HarmonicType::Natural,
        };
        Ok(he)
    }

    fn read_trill(&self, data: &[u8], seek: &mut usize) -> GpResult<TrillEffect> {
        let mut t = TrillEffect {
            fret: read_signed_byte(data, seek)?,
            ..Default::default()
        };
        t.duration.value = from_trill_period(read_signed_byte(data, seek)?)?;
        Ok(t)
    }

    fn write_bend(&self, data: &mut Vec<u8>, bend: &Option<BendEffect>) -> GpResult<()> {
        if let Some(b) = bend {
            write_signed_byte(data, from_bend_type(&b.kind));
            write_i32(data, i32::from(b.value));
            write_i32(data, b.points.len().to_i32_gp("bend points count")?);
            for point in &b.points {
                write_i32(
                    data,
                    (f32::from(point.position) * GP_BEND_POSITION
                        / f32::from(BEND_EFFECT_MAX_POSITION))
                    .round()
                    .to_i32_gp("bend point position")?,
                );
                write_i32(
                    data,
                    (f32::from(point.value) * GP_BEND_SEMITONE / GP_BEND_SEMITONE_LENGTH)
                        .round()
                        .to_i32_gp("bend point value")?,
                );
                write_bool(data, point.vibrato);
            }
        }
        Ok(())
    }

    fn write_grace(&self, data: &mut Vec<u8>, grace: &Option<GraceEffect>) -> GpResult<()> {
        let g = grace.as_ref().ok_or(GpError::MissingState {
            field: "grace effect",
        })?;
        write_signed_byte(data, g.fret);
        write_byte(data, pack_velocity(g.velocity)?.to_u8_gp("grace velocity")?);
        write_byte(data, grace_exponent(g.duration));
        write_signed_byte(data, from_grace_effect_transition(&g.transition));
        Ok(())
    }

    fn write_grace_v5(&self, data: &mut Vec<u8>, grace: &Option<GraceEffect>) -> GpResult<()> {
        let g = grace.as_ref().ok_or(GpError::MissingState {
            field: "grace effect",
        })?;
        write_byte(data, g.fret.to_u8_gp("grace fret")?);
        write_byte(data, pack_velocity(g.velocity)?.to_u8_gp("grace velocity")?);
        write_byte(
            data,
            from_grace_effect_transition(&g.transition).to_u8_gp("grace transition")?,
        );
        write_byte(data, grace_exponent(g.duration));
        let mut flags = 0u8;
        if g.is_dead {
            flags |= 0x01;
        }
        if g.is_on_beat {
            flags |= 0x02;
        }
        write_byte(data, flags);
        Ok(())
    }

    fn write_harmonic(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
    ) -> GpResult<()> {
        if let Some(h) = &note.effect.harmonic {
            let mut byte = from_harmonic_type(&h.kind);
            if h.kind == HarmonicType::Artificial {
                byte = 22;
                if let (Some(p), Some(o)) = (&h.pitch, &h.octave) {
                    let real_val = note.real_value(strings)?;
                    if p.value == real_val.saturating_add(7).wrapping_rem(12)
                        && *o == Octave::Ottava
                    {
                        byte = 15;
                    } else if p.value == real_val.wrapping_rem(12) && *o == Octave::Quindicesima {
                        byte = 17;
                    }
                } else {
                    byte = 22;
                }
            }
            write_signed_byte(data, byte);
        }
        Ok(())
    }

    fn write_harmonic_v5(
        &self,
        data: &mut Vec<u8>,
        note: &Note,
        strings: &[(i8, i8)],
    ) -> GpResult<()> {
        if let Some(h) = &note.effect.harmonic {
            write_signed_byte(data, from_harmonic_type(&h.kind));
            if h.kind == HarmonicType::Artificial {
                if let (Some(p), Some(o)) = (&h.pitch, &h.octave) {
                    write_byte(data, p.just.to_u8_gp("pitch class just")?);
                    write_signed_byte(data, p.accidental);
                    write_byte(data, from_octave(o));
                } else {
                    let p = PitchClass::from(note.real_value(strings)? % 12, None, None);
                    let o = Octave::Ottava;
                    write_byte(data, p.just.to_u8_gp("pitch class just")?);
                    write_signed_byte(data, p.accidental);
                    write_byte(data, from_octave(&o));
                }
            } else if h.kind == HarmonicType::Tapped {
                let fret = h.fret.ok_or(GpError::MissingState {
                    field: "harmonic fret",
                })?;
                write_byte(data, fret.to_u8_gp("harmonic fret")?);
            }
        }
        Ok(())
    }

    fn write_slides_v5(&self, data: &mut Vec<u8>, slides: &[SlideType]) {
        let mut st = 0u8;
        for s in slides {
            if s == &SlideType::ShiftSlideTo {
                st |= 0x01;
            } else if s == &SlideType::LegatoSlideTo {
                st |= 0x02;
            } else if s == &SlideType::OutDownwards {
                st |= 0x04;
            } else if s == &SlideType::OutUpWards {
                st |= 0x08;
            } else if s == &SlideType::IntoFromBelow {
                st |= 0x10;
            } else if s == &SlideType::IntoFromAbove {
                st |= 0x20;
            }
        }
        write_byte(data, st);
    }
}
