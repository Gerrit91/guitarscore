//! Pasting: the clipboard turned back into ordinary commands, which the caller runs as one
//! batch.

use gscore_model::{
    BeatEffect, BeatIndex, BeatPosition, MeasureIndex, NoteEffect, Song, TrackIndex, VoiceIndex,
};

use super::BeatSpot;
use super::clipboard::{BeatClip, MeasureClip};
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
fn write_beat(at: BeatPosition, clip: &BeatClip, inserted: bool, commands: &mut Vec<Command>) {
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

/// Commands that write copied beats from `at` on, and the spot of the last one written.
/// The first group goes into the cursor's measure from the cursor's beat, each further group
/// into the next measure from its first beat (so the copy's bar lines stay where they were).
/// Existing beats are overwritten; beats past a voice's end are added.
pub(super) fn paste_beats(
    song: &Song,
    at: BeatPosition,
    groups: &[Vec<BeatClip>],
) -> Result<(Vec<Command>, BeatSpot), EditError> {
    let needed = at.measure.get().saturating_add(groups.len());
    if needed > song.measure_count() {
        return Err(EditError::Unsupported(format!(
            "the copied beats need {} measures from here; insert measures first",
            groups.len()
        )));
    }
    let mut commands = Vec::new();
    let mut last = BeatSpot {
        measure: at.measure,
        beat: at.beat,
    };
    for (offset, beats) in groups.iter().enumerate() {
        let measure = MeasureIndex::new(at.measure.get().saturating_add(offset));
        let first = if offset == 0 { at.beat.get() } else { 0 };
        let mut existing = song
            .track(at.track)
            .and_then(|track| track.measure(measure))
            .and_then(|measure| measure.voice(at.voice))
            .map_or(0, |voice| voice.beats().len());
        for (index, clip) in beats.iter().enumerate() {
            let index = first.saturating_add(index);
            let target = BeatPosition {
                measure,
                beat: BeatIndex::new(index),
                ..at
            };
            // Only a beat past the voice's end is inserted; an existing beat is overwritten.
            let inserted = index >= existing;
            if inserted {
                commands.push(Command::InsertBeat {
                    at: target,
                    duration: clip.duration,
                });
                existing = index.saturating_add(1);
            }
            write_beat(target, clip, inserted, &mut commands);
            last = BeatSpot {
                measure,
                beat: target.beat,
            };
        }
    }
    Ok((commands, last))
}

/// Most voices a measure can have edits in (as Guitar Pro 7: four).
const MAX_VOICES: usize = 4;

/// Commands that write the copied measures at `start`, inside the existing measures. The
/// target measures are replaced: tempo, time signature and section once per measure (they
/// are shared by all tracks), and in every track each voice's beats.
pub(super) fn paste_measures(
    song: &Song,
    start: MeasureIndex,
    tracks: &[(TrackIndex, Vec<MeasureClip>)],
) -> Result<Vec<Command>, EditError> {
    let count = tracks.first().map_or(0, |(_, measures)| measures.len());
    if count == 0 {
        return Err(EditError::NothingToPaste);
    }
    let end = start.get().saturating_add(count);
    if end > song.measure_count() {
        return Err(EditError::Unsupported(
            "paste measures inside the song; insert empty measures first".to_owned(),
        ));
    }
    let mut commands = Vec::new();
    // The header data is the same in every track's copy: take the first track's.
    for (offset, clip) in tracks
        .first()
        .into_iter()
        .flat_map(|(_, measures)| measures)
        .enumerate()
    {
        let measure = MeasureIndex::new(start.get().saturating_add(offset));
        commands.push(Command::SetTimeSignature {
            measure,
            numerator: clip.time_signature.numerator(),
            denominator: clip.time_signature.denominator(),
        });
        if let Some(tempo) = clip.tempo {
            commands.push(Command::SetMeasureTempo { measure, tempo });
        }
        match &clip.section {
            Some(section) => commands.push(Command::SetSection {
                measure,
                title: section.title.clone(),
                color: section.color,
            }),
            None => commands.push(Command::ClearSection { measure }),
        }
    }
    for (track_at, measures) in tracks {
        let Some(track) = song.track(*track_at) else {
            continue;
        };
        for (offset, clip) in measures.iter().enumerate() {
            let measure = MeasureIndex::new(start.get().saturating_add(offset));
            let Some(target) = track.measure(measure) else {
                continue;
            };
            let mut voices_now = target.voices().len();
            let voice_count = clip.voices.len().max(voices_now).min(MAX_VOICES);
            for voice_index in 0..voice_count {
                let voice = VoiceIndex::new(voice_index);
                let beats = clip.voices.get(voice_index).map_or(&[][..], Vec::as_slice);
                let position = |beat: usize| BeatPosition {
                    track: *track_at,
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
                    write_beat(at, clip, true, &mut commands);
                }
            }
        }
    }
    Ok(commands)
}
