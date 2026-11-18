//! Pasting: the clipboard turned back into ordinary commands, which the caller runs as one
//! batch.

use gscore_model::{
    BeatEffect, BeatIndex, BeatPosition, MeasureIndex, NoteEffect, Song, StaffIndex, TrackIndex,
    VoiceIndex,
};

use super::clipboard::{BeatClip, MeasureClip};
use super::{BeatSpot, MAX_PASTE_COPIES, PasteOptions};
use crate::command::Command;
use crate::editor::EditError;

/// Whether a copied note effect is the state a new note has anyway (nothing to set).
fn note_effect_is_off(effect: &NoteEffect) -> bool {
    matches!(
        effect,
        NoteEffect::Hammer(false)
            | NoteEffect::Bend(None)
            | NoteEffect::Slide(_, false)
            | NoteEffect::Harmonic(None)
            | NoteEffect::Ghost(false)
            | NoteEffect::Accent(None)
            | NoteEffect::PalmMute(false)
            | NoteEffect::LetRing(false)
            | NoteEffect::Staccato(false)
            | NoteEffect::Trill(None)
            | NoteEffect::TremoloPicking(None)
            | NoteEffect::LeftHandTap(false)
            | NoteEffect::StringNumber(false)
            | NoteEffect::Finger(_, None)
            | NoteEffect::Vibrato(None)
    )
}

/// Whether a copied beat effect is the state a new beat has anyway.
fn beat_effect_is_off(effect: &BeatEffect) -> bool {
    matches!(
        effect,
        BeatEffect::PickStroke(None)
            | BeatEffect::BrushStroke(None)
            | BeatEffect::Slap(None)
            | BeatEffect::FadeIn(false)
            | BeatEffect::FadeOut(false)
            | BeatEffect::VolumeSwell(false)
            | BeatEffect::Octave(None)
            | BeatEffect::Arpeggio(None)
            | BeatEffect::DeadSlapped(false)
            | BeatEffect::Rasgueado(false)
            | BeatEffect::Whammy(None)
            | BeatEffect::Legato(None)
            | BeatEffect::Vibrato(None)
    )
}

/// Commands that make the beat at `at` match `clip`. `inserted` says the beat was just
/// inserted (a rest of the right length without effects); otherwise it is overwritten.
/// `text` says the beat's text comes along (bars pasted without their texts keep the
/// target's).
fn write_beat(
    at: BeatPosition,
    clip: &BeatClip,
    inserted: bool,
    text: bool,
    commands: &mut Vec<Command>,
) {
    if !inserted {
        commands.push(Command::SetDuration {
            at,
            duration: clip.duration,
        });
        // Removes the old notes; the notes below make it a normal beat again.
        commands.push(Command::SetBeatRest { at });
    }
    for note in &clip.notes {
        commands.push(Command::SetFret {
            at,
            string: note.string,
            fret: note.fret,
        });
        // The note is new (the beat's notes were removed), so only the effects it has
        // need setting.
        for effect in note.effects.iter().filter(|e| !note_effect_is_off(e)) {
            commands.push(Command::SetNoteEffect {
                at,
                string: note.string,
                effect: effect.clone(),
            });
        }
        if note.dead {
            commands.push(Command::SetDeadNote {
                at,
                string: note.string,
                dead: true,
            });
        }
        if note.tie {
            commands.push(Command::SetTie {
                at,
                string: note.string,
                tie: true,
            });
        }
    }
    // Pasted notes start with the dynamic in effect where they land; give them the copy's.
    if let Some(dynamic) = clip.dynamic
        && !clip.notes.is_empty()
    {
        commands.push(Command::SetDynamic { at, dynamic });
    }
    // The text above the beat; an inserted beat has none to clear.
    if text && (!inserted || !clip.text.is_empty()) {
        commands.push(Command::SetBeatText {
            at,
            text: clip.text.clone(),
        });
    }
    // An overwritten beat keeps its own effects unless every one is set.
    for effect in clip
        .effects
        .iter()
        .filter(|e| !inserted || !beat_effect_is_off(e))
    {
        commands.push(Command::SetBeatEffect {
            at,
            effect: effect.clone(),
        });
    }
}

/// Commands that put copied beats into the voice of `voice` (its track, staff and voice):
/// first the beats from `remove`'s first spot to its last are taken out, then the copied
/// beats go in from `start`, the first group into `start`'s measure from `start`'s beat and
/// each further group into the next measure from its first beat (so the copy's bar lines
/// stay where they were). Nothing is overwritten: a bar can end up with more or less than
/// fits, as in Guitar Pro. Returns the commands and the spots of the first and last beat
/// put in.
pub(super) fn replace_beats(
    song: &Song,
    voice: BeatPosition,
    start: BeatSpot,
    remove: Option<(BeatSpot, BeatSpot)>,
    groups: &[Vec<BeatClip>],
) -> Result<(Vec<Command>, BeatSpot, BeatSpot), EditError> {
    let needed = start.measure.get().saturating_add(groups.len());
    if needed > song.measure_count() {
        return Err(EditError::Unsupported(format!(
            "the copied beats need {} measures from here; insert measures first",
            groups.len()
        )));
    }
    let at = |measure: MeasureIndex, beat: usize| BeatPosition {
        measure,
        beat: BeatIndex::new(beat),
        ..voice
    };
    let beats_in = |measure: MeasureIndex| {
        song.staff(voice.track, voice.staff)
            .and_then(|staff| staff.measure(measure))
            .and_then(|measure| measure.voice(voice.voice))
            .map_or(0, |voice| voice.beats().len())
    };
    let mut commands = Vec::new();
    if let Some((first, last)) = remove {
        for index in first.measure.get()..=last.measure.get() {
            let measure = MeasureIndex::new(index);
            let count = beats_in(measure);
            let from = if measure == first.measure {
                first.beat.get()
            } else {
                0
            };
            let to = if measure == last.measure {
                last.beat.get().min(count.saturating_sub(1))
            } else {
                count.saturating_sub(1)
            };
            if count == 0 || from > to {
                continue;
            }
            // The beats after the removed ones move up, so the same index goes each time.
            for _ in from..=to {
                commands.push(Command::DeleteBeat {
                    at: at(measure, from),
                });
            }
        }
    }
    let mut first = None;
    let mut last = start;
    for (offset, beats) in groups.iter().enumerate() {
        let measure = MeasureIndex::new(start.measure.get().saturating_add(offset));
        let from = if offset == 0 { start.beat.get() } else { 0 };
        for (index, clip) in beats.iter().enumerate() {
            let target = at(measure, from.saturating_add(index));
            commands.push(Command::InsertBeat {
                at: target,
                duration: clip.duration,
            });
            write_beat(target, clip, true, true, &mut commands);
            let spot = BeatSpot {
                measure,
                beat: target.beat,
            };
            first.get_or_insert(spot);
            last = spot;
        }
    }
    let first = first.ok_or(EditError::NothingToPaste)?;
    Ok((commands, first, last))
}

/// Most voices a measure can have edits in (as Guitar Pro 7: four).
const MAX_VOICES: usize = 4;

/// Commands that write the copied measures at `start`, inside the existing measures. The
/// target measures are replaced: tempo, time signature and section once per measure (they
/// are shared by all tracks), and in every track each voice's beats.
pub(super) fn paste_measures(
    song: &Song,
    start: MeasureIndex,
    staves: &[(TrackIndex, StaffIndex, Vec<MeasureClip>)],
    options: PasteOptions,
) -> Result<Vec<Command>, EditError> {
    let count = staves.first().map_or(0, |(_, _, measures)| measures.len());
    if count == 0 {
        return Err(EditError::NothingToPaste);
    }
    let copies = options.copies.clamp(1, MAX_PASTE_COPIES);
    let total = count.saturating_mul(copies);
    let end = start.get().saturating_add(total);
    if end > song.measure_count() {
        return Err(EditError::Unsupported(
            "paste measures inside the song; insert empty measures first".to_owned(),
        ));
    }
    let mut commands = Vec::new();
    // The header data is the same in every track's and staff's copy: take the first's. The
    // copied bars repeat `copies` times.
    for (offset, clip) in staves
        .first()
        .into_iter()
        .flat_map(|(_, _, measures)| measures.iter().cycle().take(total))
        .enumerate()
    {
        let measure = MeasureIndex::new(start.get().saturating_add(offset));
        if options.time_signatures {
            commands.push(Command::SetTimeSignature {
                measure,
                numerator: clip.time_signature.numerator(),
                denominator: clip.time_signature.denominator(),
            });
        }
        if options.tempo
            && let Some(tempo) = clip.tempo
        {
            commands.push(Command::SetMeasureTempo { measure, tempo });
        }
        if options.sections {
            match &clip.section {
                Some(section) => commands.push(Command::SetSection {
                    measure,
                    title: section.title.clone(),
                    color: section.color,
                }),
                None => commands.push(Command::ClearSection { measure }),
            }
        }
    }
    for (track_at, staff_at, measures) in staves {
        let Some(staff) = song.staff(*track_at, *staff_at) else {
            continue;
        };
        for (offset, clip) in measures.iter().cycle().take(total).enumerate() {
            let measure = MeasureIndex::new(start.get().saturating_add(offset));
            let Some(target) = staff.measure(measure) else {
                continue;
            };
            let mut voices_now = target.voices().len();
            let voice_count = clip.voices.len().max(voices_now).min(MAX_VOICES);
            for voice_index in 0..voice_count {
                let voice = VoiceIndex::new(voice_index);
                let beats = clip.voices.get(voice_index).map_or(&[][..], Vec::as_slice);
                let position = |beat: usize| BeatPosition {
                    track: *track_at,
                    staff: *staff_at,
                    measure,
                    voice,
                    beat: BeatIndex::new(beat),
                };
                // Clear the voice (including placeholders), then write the copied beats.
                let existing = target.voice(voice).map_or(0, |voice| voice.beats().len());
                for _ in 0..existing {
                    commands.push(Command::DeleteBeat { at: position(0) });
                }
                if beats.is_empty() {
                    continue;
                }
                if voice_index > voices_now {
                    // The voice before it does not exist, so this one cannot be created.
                    continue;
                }
                voices_now = voices_now.max(voice_index.saturating_add(1));
                for (index, clip) in beats.iter().enumerate() {
                    let at = position(index);
                    commands.push(Command::InsertBeat {
                        at,
                        duration: clip.duration,
                    });
                    write_beat(at, clip, true, options.texts, &mut commands);
                }
            }
        }
    }
    Ok(commands)
}
