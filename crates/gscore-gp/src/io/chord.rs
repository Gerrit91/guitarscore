//! Guitar Pro reading and writing: chord.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub(crate) trait SongChordOps {
    fn read_chord(&self, data: &[u8], seek: &mut usize, string_count: u8) -> GpResult<Chord>;
    fn read_old_format_chord(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()>;
    fn read_new_format_chord_v3(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()>;
    fn read_new_format_chord_v4(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()>;
    fn write_chord(&self, data: &mut Vec<u8>, beat: &Beat);
    fn write_new_format_chord(&self, data: &mut Vec<u8>, chord: &Chord);
    fn write_old_format_chord(&self, data: &mut Vec<u8>, chord: &Chord);
    fn write_chord_v4(&self, data: &mut Vec<u8>, beat: &Beat);
}

/// How GP3 and GP4 store a new-format chord differently.
#[derive(Clone, Copy)]
struct Layout {
    /// Fret slots, one per string.
    strings: usize,
    /// Barre slots.
    barres: usize,
    /// GP4 adds fingerings and the "show" flag.
    fingerings: bool,
}

const GP3: Layout = Layout {
    strings: 6,
    barres: 2,
    fingerings: false,
};

const GP4: Layout = Layout {
    strings: 7,
    barres: 5,
    fingerings: true,
};

/// Fingering slots in a GP4 chord.
const FINGERINGS: usize = 7;
/// Omission slots in a chord.
const OMISSIONS: usize = 7;
/// The size of a chord's name field.
const NAME_FIELD: u8 = 22;

impl SongChordOps for Song {
    fn read_chord(&self, data: &[u8], seek: &mut usize, string_count: u8) -> GpResult<Chord> {
        let mut c = Chord {
            length: string_count,
            strings: Vec::new(),
            ..Default::default()
        };
        c.new_format = Some(read_bool(data, seek)?);
        if c.new_format == Some(true) {
            if self.version.number.0 == 3 {
                self.read_new_format_chord_v3(data, seek, &mut c)?;
            } else {
                self.read_new_format_chord_v4(data, seek, &mut c)?;
            }
        } else {
            if self.version.number.0 == 3 {
                read_byte(data, seek)?;
            }
            self.read_old_format_chord(data, seek, &mut c)?;
        }
        Ok(c)
    }

    fn read_old_format_chord(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()> {
        chord.name = read_int_byte_size_string(data, seek)?;
        // Ints that do not fit are truncated, as Guitar Pro does with this metadata.
        chord.first_fret = Some(low_u8(read_int(data, seek)?));
        for _ in 0..6 {
            chord.strings.push(low_i8(read_int(data, seek)?));
        }
        Ok(())
    }

    fn read_new_format_chord_v3(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()> {
        // GP3 uses the same "0x01"-marked chord layout as GP4 for the fields that share
        // names (root, kind, extension, fifth, ninth, eleventh, barre_*), stored as single
        // bytes — NOT as 4-byte ints. Only bass, tonality, first_fret and per-string frets
        // are ints. This matches FILE-STRUCTURE-CHORD-DIAGRAMS.md and how TuxGuitar/
        // Perlence's GP4 reader consumes GP3 clipboards. GP3-vs-GP4 delta: 6 strings (not
        // 7), 2 barres (not 5), and no trailing fingerings / show byte.
        read_new_format(data, seek, chord, GP3)
    }

    fn read_new_format_chord_v4(
        &self,
        data: &[u8],
        seek: &mut usize,
        chord: &mut Chord,
    ) -> GpResult<()> {
        read_new_format(data, seek, chord, GP4)
    }

    fn write_chord(&self, data: &mut Vec<u8>, beat: &Beat) {
        if let Some(c) = &beat.effect.chord {
            write_bool(data, c.new_format == Some(true));
            if c.new_format == Some(true) {
                self.write_new_format_chord(data, c);
            } else {
                self.write_old_format_chord(data, c);
            }
        }
    }

    fn write_new_format_chord(&self, data: &mut Vec<u8>, chord: &Chord) {
        // Symmetric with `read_new_format_chord_v3` (GP3).
        write_new_format(data, chord, GP3);
    }

    fn write_old_format_chord(&self, data: &mut Vec<u8>, chord: &Chord) {
        write_int_byte_size_string(data, &chord.name);
        write_i32(data, chord.first_fret.map_or(0, i32::from));
        write_frets(data, chord, 6);
    }

    fn write_chord_v4(&self, data: &mut Vec<u8>, beat: &Beat) {
        if let Some(c) = &beat.effect.chord {
            write_signed_byte(data, 1);
            write_new_format(data, c, GP4);
        }
    }
}

/// Reads a new-format chord after its format flag. Most fields are bytes; bass, tonality,
/// first fret and the frets are ints, truncated if they do not fit (some real-world GP3
/// exports put unused-slot sentinels and padding junk there; the diagram is only display
/// metadata, and Guitar Pro itself tolerates the noise).
fn read_new_format(
    data: &[u8],
    seek: &mut usize,
    chord: &mut Chord,
    layout: Layout,
) -> GpResult<()> {
    chord.sharp = Some(read_bool(data, seek)?);
    skip(seek, 3);
    chord.root = Some(PitchClass::from(
        byte_as_i8(read_byte(data, seek)?),
        None,
        chord.sharp,
    ));
    chord.kind = Some(get_chord_type(read_byte(data, seek)?));
    chord.extension = Some(get_chord_extension(read_byte(data, seek)?));
    chord.bass = Some(PitchClass::from(
        low_i8(read_int(data, seek)?),
        None,
        chord.sharp,
    ));
    chord.tonality = Some(get_chord_alteration(low_u8(read_int(data, seek)?))?);
    chord.add = Some(read_bool(data, seek)?);
    chord.name = read_byte_size_string(data, seek, usize::from(NAME_FIELD))?;
    chord.fifth = Some(get_chord_alteration(read_byte(data, seek)?)?);
    chord.ninth = Some(get_chord_alteration(read_byte(data, seek)?)?);
    chord.eleventh = Some(get_chord_alteration(read_byte(data, seek)?)?);
    chord.first_fret = Some(low_u8(read_int(data, seek)?));
    for _ in 0..layout.strings {
        chord.strings.push(low_i8(read_int(data, seek)?));
    }
    let barre_count = usize::from(read_byte(data, seek)?);
    let mut fields = [Vec::new(), Vec::new(), Vec::new()];
    for field in &mut fields {
        for _ in 0..layout.barres {
            field.push(byte_as_i8(read_byte(data, seek)?));
        }
    }
    let [frets, starts, ends] = fields;
    chord.barres.extend(
        frets
            .into_iter()
            .zip(starts)
            .zip(ends)
            .take(barre_count.min(layout.barres))
            .map(|((fret, start), end)| Barre { fret, start, end }),
    );
    for _ in 0..OMISSIONS {
        chord.omissions.push(read_bool(data, seek)?);
    }
    skip(seek, 1);
    if layout.fingerings {
        for _ in 0..FINGERINGS {
            chord
                .fingerings
                .push(get_fingering(read_signed_byte(data, seek)?));
        }
        chord.show = Some(read_bool(data, seek)?);
    }
    Ok(())
}

/// Writes a new-format chord after its format flag, as [`read_new_format`] reads it.
fn write_new_format(data: &mut Vec<u8>, chord: &Chord, layout: Layout) {
    write_bool(data, chord.sharp == Some(true));
    write_placeholder_default(data, 3);
    write_byte(data, chord.root.as_ref().map_or(0, |r| i8_as_byte(r.value)));
    write_byte(data, chord.kind.as_ref().map_or(0, from_chord_type));
    write_byte(
        data,
        chord.extension.as_ref().map_or(0, from_chord_extension),
    );
    write_i32(data, chord.bass.as_ref().map_or(0, |b| i32::from(b.value)));
    write_i32(
        data,
        chord
            .tonality
            .as_ref()
            .map_or(0, |t| i32::from(from_chord_alteration(t))),
    );
    write_bool(data, chord.add == Some(true));
    write_fixed_string(data, &chord.name, NAME_FIELD);
    for alteration in [&chord.fifth, &chord.ninth, &chord.eleventh] {
        write_byte(data, alteration.as_ref().map_or(0, from_chord_alteration));
    }
    write_i32(data, chord.first_fret.map_or(0, i32::from));
    write_frets(data, chord, layout.strings);
    let count = chord.barres.len().min(layout.barres);
    write_byte(data, u8::try_from(count).unwrap_or_default());
    let barre = |i: usize| chord.barres.get(i).filter(|_| i < count);
    for field in [|b: &Barre| b.fret, |b: &Barre| b.start, |b: &Barre| b.end] {
        for i in 0..layout.barres {
            write_byte(data, barre(i).map_or(0, |b| i8_as_byte(field(b))));
        }
    }
    for i in 0..OMISSIONS {
        write_bool(data, chord.omissions.get(i).copied().unwrap_or(true));
    }
    write_placeholder_default(data, 1);
    if layout.fingerings {
        for i in 0..FINGERINGS {
            write_signed_byte(data, chord.fingerings.get(i).map_or(-2, from_fingering));
        }
        write_bool(data, chord.show == Some(true));
    }
}

/// Writes `slots` frets, -1 for strings the chord does not have.
fn write_frets(data: &mut Vec<u8>, chord: &Chord, slots: usize) {
    for i in 0..slots {
        write_i32(
            data,
            chord.strings.get(i).map_or(-1, |&fret| i32::from(fret)),
        );
    }
}
