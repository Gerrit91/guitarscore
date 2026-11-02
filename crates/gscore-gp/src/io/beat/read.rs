//! Guitar Pro reading and writing: beat read.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;
use crate::io::*;

pub(super) fn read_beat(
    song: &mut Song,
    data: &[u8],
    seek: &mut usize,
    voice: &mut Voice,
    start: i64,
    track_index: usize,
) -> GpResult<i64> {
    let flags = read_byte(data, seek)?;
    // GP3/4 put the beats of a voice that start together into one beat.
    let existing = if song.version.number < (5, 0, 0) {
        voice
            .beats
            .iter()
            .rposition(|beat| beat.start == Some(start))
    } else {
        None
    };
    let index = match existing {
        Some(index) => index,
        None => {
            voice.beats.push(Beat {
                start: Some(start),
                ..Default::default()
            });
            voice.beats.len().saturating_sub(1)
        }
    };
    let beat = item_mut(&mut voice.beats, index, "beat")?;

    if (flags & 0x40) == 0x40 {
        beat.status = get_beat_status(read_byte(data, seek)?);
    }
    let duration = read_duration(data, seek, flags)?;
    let mut note_effect = NoteEffect::default();
    if (flags & 0x02) == 0x02 {
        use crate::io::SongChordOps;
        let strings = item(&song.tracks, track_index, "track")?.strings.len();
        beat.effect.chord = Some(song.read_chord(data, seek, strings.to_u8_gp("string count")?)?);
    }
    if (flags & 0x04) == 0x04 {
        beat.text = read_int_byte_size_string(data, seek)?;
    }
    if (flags & 0x08) == 0x08 {
        let chord = beat.effect.chord.clone();
        if song.version.number.0 == 3 {
            beat.effect = song.read_beat_effects_v3(data, seek, &mut note_effect)?;
        } else {
            beat.effect = song.read_beat_effects_v4(data, seek)?;
        }
        beat.effect.chord = chord;
    }
    if (flags & 0x10) == 0x10 {
        use crate::io::SongMixTableOps;
        let mtc = song.read_mix_table_change(data, seek)?;
        beat.effect.mix_table_change = Some(mtc);
    }
    use crate::io::SongNoteOps;
    song.read_notes(data, seek, track_index, beat, &duration, note_effect)?;
    Ok(if beat.status == BeatStatus::Empty {
        0
    } else {
        duration.time()?.to_i64_gp("beat duration time")?
    })
}

pub(super) fn read_beat_v5(
    song: &mut Song,
    data: &[u8],
    seek: &mut usize,
    voice: &mut Voice,
    start: &mut i64,
    track_index: usize,
) -> GpResult<i64> {
    let duration = read_beat(song, data, seek, voice, *start, track_index)?;
    let beat = voice
        .beats
        .last_mut()
        .ok_or(GpError::MissingState { field: "beat" })?;

    let flags2 = read_short(data, seek)?;
    for (bit, octave) in [
        (0x0010, Octave::Ottava),
        (0x0020, Octave::OttavaBassa),
        (0x0040, Octave::Quindicesima),
        (0x0100, Octave::QuindicesimaBassa),
    ] {
        if (flags2 & bit) == bit {
            beat.octave = octave;
        }
    }

    let display = &mut beat.display;
    display.break_beam = (flags2 & 0x0001) == 0x0001;
    display.force_beam = (flags2 & 0x0004) == 0x0004;
    display.force_bracket = (flags2 & 0x2000) == 0x2000;
    display.break_secondary_tuplet = (flags2 & 0x1000) == 0x1000;
    if (flags2 & 0x0002) == 0x0002 {
        display.beam_direction = VoiceDirection::Down;
    }
    if (flags2 & 0x0008) == 0x0008 {
        display.beam_direction = VoiceDirection::Up;
    }
    if (flags2 & 0x0200) == 0x0200 {
        display.tuplet_bracket = TupletBracket::Start;
    }
    if (flags2 & 0x0400) == 0x0400 {
        display.tuplet_bracket = TupletBracket::End;
    }
    if (flags2 & 0x0800) == 0x0800 {
        display.break_secondary = read_byte(data, seek)?;
    }

    Ok(duration)
}

pub(super) fn read_beat_effects_v3(
    song: &Song,
    data: &[u8],
    seek: &mut usize,
    note_effect: &mut NoteEffect,
) -> GpResult<BeatEffects> {
    let mut be = BeatEffects::default();
    let flags = read_byte(data, seek)?;
    note_effect.vibrato = (flags & 0x01) == 0x01 || note_effect.vibrato;
    be.vibrato = (flags & 0x02) == 0x02 || be.vibrato;
    be.fade_in = (flags & 0x10) == 0x10;
    if (flags & 0x20) == 0x20 {
        be.slap_effect = get_slap_effect(read_byte(data, seek)?)?;
        if be.slap_effect == SlapEffect::None {
            be.tremolo_bar = Some(song.read_tremolo_bar(data, seek)?);
        } else {
            read_int(data, seek)?;
        }
    }
    if (flags & 0x40) == 0x40 {
        be.stroke = song.read_beat_stroke(data, seek)?;
    }
    if (flags & 0x04) == 0x04 {
        note_effect.harmonic = Some(HarmonicEffect::default());
    }
    if (flags & 0x08) == 0x08 {
        note_effect.harmonic = Some(HarmonicEffect {
            kind: HarmonicType::Artificial,
            ..Default::default()
        });
    }
    Ok(be)
}

pub(super) fn read_beat_effects_v4(
    song: &Song,
    data: &[u8],
    seek: &mut usize,
) -> GpResult<BeatEffects> {
    let mut be = BeatEffects::default();
    let flags1 = read_signed_byte(data, seek)?;
    let flags2 = read_signed_byte(data, seek)?;
    be.vibrato = (flags1 & 0x02) == 0x02 || be.vibrato;
    be.fade_in = (flags1 & 0x10) == 0x10;
    if (flags1 & 0x20) == 0x20 {
        let v = read_signed_byte(data, seek)?;
        be.slap_effect = match u8::try_from(v) {
            Ok(v) => get_slap_effect(v)?,
            Err(_) => SlapEffect::None,
        };
    }
    if (flags2 & 0x04) == 0x04 {
        use crate::io::SongEffectOps;
        be.tremolo_bar = song.read_bend_effect(data, seek)?;
    }
    if (flags1 & 0x40) == 0x40 {
        be.stroke = song.read_beat_stroke(data, seek)?;
    }
    be.has_rasgueado = (flags2 & 0x01) == 0x01;
    if (flags2 & 0x02) == 0x02 {
        be.pick_stroke = get_beat_stroke_direction(read_signed_byte(data, seek)?)?;
    }
    Ok(be)
}

pub(super) fn read_beat_stroke(song: &Song, data: &[u8], seek: &mut usize) -> GpResult<BeatStroke> {
    let mut bs = BeatStroke::default();
    let down = read_signed_byte(data, seek)?;
    let up = read_signed_byte(data, seek)?;
    if up > 0 {
        bs.direction = BeatStrokeDirection::Up;
        bs.value = stroke_value(song, up).to_u16_gp("stroke value")?;
    }
    if down > 0 {
        bs.direction = BeatStrokeDirection::Down;
        bs.value = stroke_value(song, down).to_u16_gp("stroke value")?;
    }
    if song.version.number >= (5, 0, 0) {
        bs.swap_direction();
    }
    Ok(bs)
}

pub(super) fn stroke_value(_song: &Song, value: i8) -> u8 {
    match value {
        1 => DURATION_HUNDRED_TWENTY_EIGHTH,
        2 => DURATION_SIXTY_FOURTH,
        3 => DURATION_THIRTY_SECOND,
        4 => DURATION_SIXTEENTH,
        5 => DURATION_EIGHTH,
        6 => DURATION_QUARTER,
        _ => DURATION_SIXTY_FOURTH,
    }
}

pub(super) fn read_tremolo_bar(
    _song: &Song,
    data: &[u8],
    seek: &mut usize,
) -> GpResult<BendEffect> {
    let mut be = BendEffect {
        kind: BendType::Dip,
        ..Default::default()
    };
    // Tremolo bar value is display-only; some editors write junk in the field
    // when the effect is unused. Truncate to `i16` instead of erroring.
    be.value = low_i16(read_int(data, seek)?);
    be.points.push(BendPoint {
        position: 0,
        value: 0,
        ..Default::default()
    });
    // Derived from `be.value`, which we now accept as-is; clamp to `i8` here
    // so a garbage tremolo value does not fail the whole load.
    let midpoint_value = round_to_i8(-f32::from(be.value) / GP_BEND_SEMITONE);
    be.points.push(BendPoint {
        position: BEND_EFFECT_MAX_POSITION / 2,
        value: midpoint_value,
        ..Default::default()
    });
    be.points.push(BendPoint {
        position: BEND_EFFECT_MAX_POSITION,
        value: 0,
        ..Default::default()
    });
    Ok(be)
}
