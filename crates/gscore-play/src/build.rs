//! Building a performance: [`collect`] walks the played measures of each track and gathers
//! the notes with their timing, velocity and effects. The whole song and a one-beat excerpt
//! share it; `midi` turns the notes into MIDI events.

use std::collections::{HashMap, HashSet};

use gscore_model::{
    BeatKind, BeatView, BendKind, Duration, Fret, ModelError, NoteKind, SlideKind, Song,
    StringNumber, StrokeDirection, TrackIndex,
};

use crate::curve::Curve;
use crate::drums;
use crate::effects;
use crate::graces;
use crate::performance::{Attack, PerformanceTrack, PlayedNote};
use crate::sequence::{MeasureSpan, ticks};
use crate::sounding::{self, Sounding};
use crate::strokes;
use crate::swing;
use crate::tempo::PPQ;

/// Ticks in a whole note.
const WHOLE: u64 = 4 * PPQ as u64;
/// Dead (muted) notes sound this long at most: a 32nd note.
const DEAD_NOTE_TICKS: u64 = WHOLE / 32;
/// Dead notes are played softer, at this share of their velocity (percent).
const DEAD_NOTE_VELOCITY_PERCENT: u16 = 60;
/// A dead slapped beat's synthesized hits: their velocity and how long they sound.
const DEAD_SLAP_VELOCITY: u8 = 70;
const DEAD_SLAP_TICKS: u64 = WHOLE / 64;
/// A tied note continues the previous one if that ends within this many ticks of its start
/// (rounding of tuplets).
const TIE_TOLERANCE: u64 = 2;

/// What [`collect`] gathers.
pub(crate) struct Collected {
    /// The tracks that play.
    pub(crate) tracks: Vec<PerformanceTrack>,
    /// The notes, with effects applied.
    pub(crate) notes: Vec<PlayedNote>,
    /// Tempo changes on beats.
    pub(crate) tempo_changes: Vec<(u64, u32)>,
    /// Notes left out because their pitch could not be worked out.
    pub(crate) skipped_notes: usize,
}

/// Gathers the notes of `selected` over the played `measures`. With `keep`, only the notes
/// it accepts stay (an excerpt); they still get the effects worked out from their
/// neighbours (slides, let ring) first. A tied note starting at `fresh_at` is struck
/// anew instead of continuing the note before (an excerpt of a tied beat must sound).
pub(crate) fn collect(
    song: &Song,
    selected: &[TrackIndex],
    measures: &[MeasureSpan],
    keep: Option<&dyn Fn(&Sounding) -> bool>,
    fresh_at: Option<u64>,
) -> Result<Collected, ModelError> {
    let mut tracks = Vec::with_capacity(selected.len());
    let mut notes: Vec<Sounding> = Vec::new();
    let mut tempo_changes = Vec::new();
    let mut skipped_notes = 0_usize;

    for &track_index in selected {
        let (Some(view), Some(sound)) = (song.track(track_index), song.played_sound(track_index))
        else {
            return Err(ModelError::Invalid(format!(
                "the song has no {track_index}"
            )));
        };
        let number = tracks.len();
        tracks.push(PerformanceTrack::of(song, track_index)?);

        // The note currently sounding on each (voice, string), to extend it on a tie.
        // Voices are independent: a tie in one must not extend a note of another on the
        // same string.
        let mut on_string: HashMap<(usize, u8), usize> = HashMap::new();
        // The (voice, string) pairs whose last note hammers on or pulls off to the next.
        let mut hammering: HashSet<(usize, u8)> = HashSet::new();
        for span in measures {
            // A bar-repetition mark plays the bar it repeats.
            let Some(measure) = view.measure(view.sounding_measure(span.measure)) else {
                continue;
            };
            let feel = song.feel(span.measure).unwrap_or_default();
            let measure_ticks = span.end.saturating_sub(span.start);
            for voice in measure.voices() {
                let voice_index = voice.index().get();
                let mut onset = Duration::new(0, 1)?;
                // Grace beats do not take time; their notes play just before the next beat.
                let mut pending_graces: Vec<BeatView<'_>> = Vec::new();
                for beat in voice.beats() {
                    if beat.kind() == BeatKind::Empty {
                        continue;
                    }
                    if beat.is_grace() {
                        pending_graces.push(beat);
                        continue;
                    }
                    let length = beat.duration()?;
                    let next = onset.checked_add(length)?;
                    let start = span.start.saturating_add(swing::swing_offset(
                        ticks(onset),
                        measure_ticks,
                        feel,
                    ));
                    let end = span.start.saturating_add(swing::swing_offset(
                        ticks(next),
                        measure_ticks,
                        feel,
                    ));
                    onset = next;
                    if let Some(bpm) = beat.tempo_change() {
                        tempo_changes.push((start, bpm));
                    }
                    if beat.kind() != BeatKind::Normal {
                        pending_graces.clear();
                        hammering.retain(|(voice, _)| *voice != voice_index);
                        continue;
                    }
                    let legato_beat = beat.legato().is_some_and(|legato| legato.destination);
                    let graces = graces::gather(&view, &pending_graces, &beat, &mut skipped_notes);
                    pending_graces.clear();
                    // A grace note whose transition is a hammer-on means the main note on
                    // its string is reached without a pick.
                    let grace_hammers: HashSet<u8> = graces
                        .iter()
                        .filter(|grace| grace.hammer)
                        .map(|grace| grace.string)
                        .collect();
                    let (grace_notes, delays) = graces::place(
                        &graces,
                        start,
                        crate::timeline::tempo_at(song, span.measure),
                        number,
                    );
                    notes.extend(grace_notes);
                    let first_note = notes.len();
                    for note in beat.notes() {
                        // A dead slapped beat is written as a cross instead of its notes.
                        if beat.is_dead_slapped() {
                            break;
                        }
                        let kind = note.kind();
                        let string = note.string().map(|s| s.get()).unwrap_or(0);
                        let hammered = hammering.remove(&(voice_index, string));
                        if note.is_hammer() {
                            hammering.insert((voice_index, string));
                        }
                        if kind == NoteKind::Tie && fresh_at != Some(start) {
                            let tied = on_string
                                .get(&(voice_index, string))
                                .and_then(|&i| notes.get_mut(i))
                                .filter(|n| n.end.abs_diff(start) <= TIE_TOLERANCE);
                            if let Some(previous) = tied {
                                previous.end = end;
                                continue;
                            }
                        }
                        if !matches!(kind, NoteKind::Normal | NoteKind::Tie | NoteKind::Dead) {
                            continue;
                        }
                        let Ok(pitch) = view.sounding_pitch(&note) else {
                            skipped_notes = skipped_notes.saturating_add(1);
                            continue;
                        };
                        let (end, velocity) = if kind == NoteKind::Dead {
                            let soft =
                                u16::from(note.velocity()) * DEAD_NOTE_VELOCITY_PERCENT / 100;
                            (
                                end.min(start.saturating_add(DEAD_NOTE_TICKS)),
                                u8::try_from(soft).unwrap_or(1).max(1),
                            )
                        } else {
                            (end, note.velocity())
                        };
                        // Drums have no pitch to bend, slide or shake.
                        let pitched = !sound.percussion;
                        let bend = note
                            .bend()
                            .filter(|bend| pitched && bend.quarter_tones != 0)
                            .map(|bend| (bend.quarter_tones, bend.kind == BendKind::Release));
                        let slides: Vec<SlideKind> = if pitched {
                            note.slides().collect()
                        } else {
                            Vec::new()
                        };
                        let trill = note.trill().and_then(|trill| {
                            let fret = trill.fret?;
                            let string = StringNumber::new(string).ok()?;
                            let pitch = view.pitch_at(string, Fret::new(fret).ok()?).ok()?;
                            Some((pitch.get(), fret, trill.speed))
                        });
                        let pick_slide = note.slides().find_map(|slide| match slide {
                            SlideKind::PickSlideUp => Some(StrokeDirection::Up),
                            SlideKind::PickSlideDown => Some(StrokeDirection::Down),
                            _ => None,
                        });
                        // A drum note's number is Guitar Pro's articulation; play the
                        // General MIDI sound it stands for.
                        let key = if sound.percussion {
                            drums::drum_key(pitch.get())
                        } else {
                            pitch.get()
                        };
                        let mut sounding = Sounding::new(number, string, key, velocity, start, end);
                        sounding.fret = note.fret().ok().map(|fret| fret.get()).filter(|_| pitched);
                        sounding.slide = sounding::choose_slide(&slides);
                        sounding.bend = bend;
                        sounding.vibrato = note.vibrato().or(beat.vibrato()).filter(|_| pitched);
                        sounding.let_ring = note.is_let_ring();
                        sounding.tremolo = note.tremolo_speed();
                        sounding.trill = trill.filter(|_| pitched);
                        sounding.pick_slide = pick_slide.filter(|_| pitched);
                        sounding.bend_curve =
                            note.bend_shape().filter(|_| pitched).and_then(|shape| {
                                Curve::new(shape.points.iter().map(|p| (p.position, p.value)))
                            });
                        sounding.whammy =
                            beat.whammy_shape().filter(|_| pitched).and_then(|shape| {
                                Curve::new(shape.points.iter().map(|p| (p.position, p.value)))
                            });
                        sounding.dead = kind == NoteKind::Dead;
                        sounding.palm_mute = note.is_palm_muted();
                        sounding.harmonic = note
                            .harmonic()
                            .map(|harmonic| harmonic.kind)
                            .filter(|_| pitched);
                        effects::shape_note(&mut sounding, &note);
                        if note.is_left_hand_tapped() {
                            strokes::legato(&mut sounding, Attack::LeftHandTap);
                        } else if hammered || legato_beat || grace_hammers.contains(&string) {
                            strokes::legato(&mut sounding, Attack::Legato);
                        }
                        on_string.insert((voice_index, string), notes.len());
                        notes.push(sounding);
                    }
                    // A dead slapped beat writes no notes: play a short muted hit on every
                    // string instead.
                    if beat.is_dead_slapped() && !sound.percussion {
                        for string_index in 1..=view.string_count() {
                            let Some(string_number) = u8::try_from(string_index).ok() else {
                                continue;
                            };
                            let Ok(string) = StringNumber::new(string_number) else {
                                continue;
                            };
                            let Ok(pitch) = view.pitch_at(string, Fret::OPEN) else {
                                continue;
                            };
                            let mut hit = Sounding::new(
                                number,
                                string_number,
                                pitch.get(),
                                DEAD_SLAP_VELOCITY,
                                start,
                                end.min(start.saturating_add(DEAD_SLAP_TICKS)),
                            );
                            hit.fret = Some(0);
                            hit.attack = Attack::Slapped;
                            hit.dead = true;
                            hit.damp = true;
                            notes.push(hit);
                        }
                    }
                    // An appoggiatura takes the start of the beat on its string.
                    for note in notes.get_mut(first_note..).unwrap_or_default() {
                        if let Some(&delay) = delays.get(&note.string) {
                            note.start = note
                                .start
                                .saturating_add(delay)
                                .min(note.end.saturating_sub(1));
                        }
                    }
                    if let Some(chord) = notes.get_mut(first_note..) {
                        strokes::shape_beat(chord, &beat);
                    }
                }
            }
        }
    }

    effects::one_note_per_string(&mut notes);
    sounding::apply_slide_targets(&mut notes);
    effects::let_ring(&mut notes);
    if let Some(keep) = keep {
        notes.retain(|note| keep(note));
    }
    let mut notes = effects::tremolo_picking(notes);
    notes = effects::trills(notes);
    notes = effects::pick_slides(notes);
    notes = effects::rasgueados(notes);
    sounding::apply_bends(&mut notes);
    effects::apply_vibrato(&mut notes);
    Ok(Collected {
        tracks,
        notes: notes.iter().map(Sounding::played).collect(),
        tempo_changes,
        skipped_notes,
    })
}
