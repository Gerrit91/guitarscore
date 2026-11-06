//! The performance's part of the wire form (see `wire`): its tracks and notes, so an
//! engine in the browser's AudioWorklet can play strings, not only MIDI.

use gscore_model::{Fade, HarmonicKind, InstrumentKind, StrokeDirection, TrackIndex, TrackSound};

use crate::curve::Curve;
use crate::performance::{
    Attack, Performance, PerformanceTrack, PlayedNote, StringKind, TrackEngine,
};
use crate::pitch::{PitchShape, Wave};
use crate::tempo::TempoMap;
use crate::wire::{Reader, put_u16, put_u32, put_u64};

/// Caps sized by the input, so a damaged message cannot allocate without bound.
const MAX_TRACKS: usize = 1024;
const MAX_NOTES: usize = 4_000_000;
const MAX_NAME_BYTES: usize = 4096;
/// The smallest a note can be on the wire, to check a count against what is left.
const MIN_NOTE_BYTES: usize = 39;

/// Flags of a note, one bit each.
const PALM_MUTE: u8 = 1;
const DEAD: u8 = 2;
const LET_RING: u8 = 4;
const DAMPED: u8 = 8;
const GRACE: u8 = 16;

/// Appends the tracks and notes of `performance` (its skipped-note count too).
pub(crate) fn encode_performance(out: &mut Vec<u8>, performance: &Performance) {
    put_u32(
        out,
        u32::try_from(performance.tracks.len()).unwrap_or(u32::MAX),
    );
    for track in &performance.tracks {
        put_u32(out, u32::try_from(track.track.get()).unwrap_or(u32::MAX));
        let name = track.name.as_bytes();
        let name = name
            .get(..name.len().min(MAX_NAME_BYTES))
            .unwrap_or_default();
        put_u32(out, u32::try_from(name.len()).unwrap_or(0));
        out.extend_from_slice(name);
        let sound = track.sound;
        out.extend_from_slice(&[
            sound.program,
            sound.volume,
            sound.pan,
            sound.reverb,
            sound.chorus,
            u8::from(sound.percussion),
            match track.kind {
                InstrumentKind::Strings => 0,
                InstrumentKind::Drums => 1,
                InstrumentKind::Other => 2,
            },
            engine_code(track.engine),
            u8::try_from(track.strings.len()).unwrap_or(0),
        ]);
        out.extend(track.strings.iter().take(usize::from(u8::MAX)));
    }
    put_u64(
        out,
        u64::try_from(performance.skipped_notes).unwrap_or(u64::MAX),
    );
    put_u32(
        out,
        u32::try_from(performance.notes.len()).unwrap_or(u32::MAX),
    );
    for note in &performance.notes {
        encode_note(out, note);
    }
}

/// Reads what [`encode_performance`] wrote; `tempo` and `end` come from the sequence.
pub(crate) fn decode_performance(
    reader: &mut Reader<'_>,
    tempo: &TempoMap,
    end: u64,
) -> Result<Performance, String> {
    let damaged = |what: &str| format!("the playback sequence is damaged ({what})");
    let track_count = reader.u32()? as usize;
    if track_count > MAX_TRACKS || track_count > reader.remaining() {
        return Err(damaged("too many tracks"));
    }
    let mut tracks = Vec::with_capacity(track_count);
    for _ in 0..track_count {
        let track = TrackIndex::new(reader.u32()? as usize);
        let name_length = reader.u32()? as usize;
        if name_length > MAX_NAME_BYTES {
            return Err(damaged("track name"));
        }
        let name = String::from_utf8_lossy(reader.take(name_length)?).into_owned();
        let sound = TrackSound {
            program: reader.u8()?,
            volume: reader.u8()?,
            pan: reader.u8()?,
            reverb: reader.u8()?,
            chorus: reader.u8()?,
            percussion: reader.u8()? != 0,
        };
        let kind = match reader.u8()? {
            0 => InstrumentKind::Strings,
            1 => InstrumentKind::Drums,
            2 => InstrumentKind::Other,
            _ => return Err(damaged("instrument")),
        };
        let engine = engine_from(reader.u8()?).ok_or_else(|| damaged("engine"))?;
        let string_count = usize::from(reader.u8()?);
        let strings = reader.take(string_count)?.to_vec();
        tracks.push(PerformanceTrack {
            track,
            name,
            sound,
            kind,
            strings,
            engine,
        });
    }
    let skipped_notes = usize::try_from(reader.u64()?).unwrap_or(usize::MAX);
    let note_count = reader.u32()? as usize;
    if note_count > MAX_NOTES || note_count > reader.remaining() / MIN_NOTE_BYTES {
        return Err(damaged("too many notes"));
    }
    let mut notes = Vec::with_capacity(note_count);
    for _ in 0..note_count {
        let note = decode_note(reader)?;
        if note.track >= tracks.len() {
            return Err(damaged("a note's track"));
        }
        notes.push(note);
    }
    let mut performance = Performance {
        tracks,
        notes,
        tempo: tempo.clone(),
        measures: Vec::new(),
        end,
        skipped_notes,
        by_start: Vec::new(),
    };
    performance.index_by_start();
    Ok(performance)
}

const ENGINES: [TrackEngine; 5] = [
    TrackEngine::SoundFont,
    TrackEngine::Modelled(StringKind::Nylon),
    TrackEngine::Modelled(StringKind::Steel),
    TrackEngine::Modelled(StringKind::Electric),
    TrackEngine::Modelled(StringKind::Bass),
];

fn engine_code(engine: TrackEngine) -> u8 {
    ENGINES
        .iter()
        .position(|&known| known == engine)
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0)
}

fn engine_from(code: u8) -> Option<TrackEngine> {
    ENGINES.get(usize::from(code)).copied()
}

fn encode_note(out: &mut Vec<u8>, note: &PlayedNote) {
    put_u32(out, u32::try_from(note.track).unwrap_or(u32::MAX));
    out.extend_from_slice(&[
        note.string,
        u8::from(note.fret.is_some()),
        note.fret.unwrap_or(0),
        note.key,
        note.velocity,
    ]);
    put_u64(out, note.start);
    put_u64(out, note.end);
    put_u64(out, note.onset);
    let flags = [
        (note.palm_mute, PALM_MUTE),
        (note.dead, DEAD),
        (note.let_ring, LET_RING),
        (note.damped, DAMPED),
        (note.grace, GRACE),
    ]
    .iter()
    .filter(|(set, _)| *set)
    .fold(0, |flags, (_, bit)| flags | bit);
    out.extend_from_slice(&[
        attack_code(note.attack),
        match note.stroke {
            None => 0,
            Some(StrokeDirection::Up) => 1,
            Some(StrokeDirection::Down) => 2,
        },
        flags,
        harmonic_code(note.harmonic),
        match note.fade {
            None => 0,
            Some(Fade::In) => 1,
            Some(Fade::Out) => 2,
            Some(Fade::Swell) => 3,
        },
    ]);
    match note.pitch {
        None => out.push(0),
        Some(shape) => {
            out.push(1);
            encode_shape(out, &shape);
        }
    }
}

fn decode_note(reader: &mut Reader<'_>) -> Result<PlayedNote, String> {
    let bad = |what: &str| format!("the playback sequence is damaged (a note's {what})");
    let track = reader.u32()? as usize;
    let string = reader.u8()?;
    let has_fret = reader.u8()? != 0;
    let fret = reader.u8()?;
    let key = reader.u8()?;
    let velocity = reader.u8()?;
    let start = reader.u64()?;
    let end = reader.u64()?;
    let onset = reader.u64()?;
    let attack = attack_from(reader.u8()?).ok_or_else(|| bad("attack"))?;
    let stroke = match reader.u8()? {
        0 => None,
        1 => Some(StrokeDirection::Up),
        2 => Some(StrokeDirection::Down),
        _ => return Err(bad("stroke")),
    };
    let flags = reader.u8()?;
    let harmonic = harmonic_from(reader.u8()?).ok_or_else(|| bad("harmonic"))?;
    let fade = match reader.u8()? {
        0 => None,
        1 => Some(Fade::In),
        2 => Some(Fade::Out),
        3 => Some(Fade::Swell),
        _ => return Err(bad("fade")),
    };
    let pitch = match reader.u8()? {
        0 => None,
        1 => Some(decode_shape(reader)?),
        _ => return Err(bad("pitch")),
    };
    Ok(PlayedNote {
        track,
        string,
        fret: has_fret.then_some(fret),
        key,
        velocity,
        start,
        end,
        onset,
        attack,
        stroke,
        palm_mute: flags & PALM_MUTE != 0,
        dead: flags & DEAD != 0,
        harmonic,
        let_ring: flags & LET_RING != 0,
        damped: flags & DAMPED != 0,
        grace: flags & GRACE != 0,
        pitch,
        fade,
    })
}

const ATTACKS: [Attack; 7] = [
    Attack::Picked,
    Attack::Legato,
    Attack::LeftHandTap,
    Attack::Tapped,
    Attack::Slapped,
    Attack::Popped,
    Attack::Scrape,
];

fn attack_code(attack: Attack) -> u8 {
    ATTACKS
        .iter()
        .position(|&known| known == attack)
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0)
}

fn attack_from(code: u8) -> Option<Attack> {
    ATTACKS.get(usize::from(code)).copied()
}

const HARMONICS: [HarmonicKind; 5] = [
    HarmonicKind::Natural,
    HarmonicKind::Artificial,
    HarmonicKind::Tapped,
    HarmonicKind::Pinch,
    HarmonicKind::Semi,
];

/// 0 for none, then 1 more than the kind's place in [`HARMONICS`].
fn harmonic_code(harmonic: Option<HarmonicKind>) -> u8 {
    harmonic
        .and_then(|kind| HARMONICS.iter().position(|&known| known == kind))
        .and_then(|index| u8::try_from(index.saturating_add(1)).ok())
        .unwrap_or(0)
}

fn harmonic_from(code: u8) -> Option<Option<HarmonicKind>> {
    match code.checked_sub(1) {
        None => Some(None),
        Some(index) => HARMONICS.get(usize::from(index)).copied().map(Some),
    }
}

fn encode_shape(out: &mut Vec<u8>, shape: &PitchShape) {
    put_u32(out, shape.start.cast_unsigned());
    put_u32(out, shape.target.cast_unsigned());
    put_u64(out, shape.ramp);
    put_u64(out, shape.length);
    match shape.vibrato {
        None => out.push(0),
        Some(wave) => {
            out.push(1);
            put_u16(out, wave.depth_cents);
            put_u64(out, wave.period);
            put_u64(out, wave.length);
        }
    }
    for curve in [shape.bend, shape.whammy] {
        match curve {
            None => out.push(0),
            Some(curve) => {
                let points = curve.points();
                out.push(u8::try_from(points.len()).unwrap_or(0));
                for &(position, value) in points {
                    out.push(position);
                    out.push(value.cast_unsigned());
                }
            }
        }
    }
}

fn decode_shape(reader: &mut Reader<'_>) -> Result<PitchShape, String> {
    let start = reader.u32()?.cast_signed();
    let target = reader.u32()?.cast_signed();
    let ramp = reader.u64()?;
    let length = reader.u64()?;
    let vibrato = match reader.u8()? {
        0 => None,
        1 => Some(Wave {
            depth_cents: reader.u16()?,
            period: reader.u64()?,
            length: reader.u64()?,
        }),
        _ => return Err("the playback sequence is damaged (a vibrato)".to_owned()),
    };
    let mut curves = [None, None];
    for slot in &mut curves {
        let count = usize::from(reader.u8()?);
        let bytes = reader.take(count.saturating_mul(2))?;
        *slot = Curve::new(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[position, value]| (position, value.cast_signed())),
        );
    }
    let [bend, whammy] = curves;
    Ok(PitchShape {
        start,
        target,
        ramp,
        vibrato,
        bend,
        whammy,
        length,
    })
}
