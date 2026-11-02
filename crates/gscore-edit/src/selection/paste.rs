//! Pasting: the clipboard turned back into ordinary commands, which the caller runs as one
//! batch.

use gscore_model::{
    BeatEffect, BeatIndex, BeatPosition, MeasureIndex, NoteEffect, Song, TrackIndex, VoiceIndex,
};

use super::{BeatClip, MeasureClip};
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
            | NoteEffect::Trill(false)
            | NoteEffect::TremoloPicking(false)
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

/// Commands that write `beats` at `at`, adding beats at the end when the voice is shorter.
pub(super) fn paste_beats(
    song: &Song,
    at: BeatPosition,
    beats: &[BeatClip],
) -> Result<Vec<Command>, EditError> {
    let mut existing = song
        .track(at.track)
        .and_then(|track| track.measure(at.measure))
        .and_then(|measure| measure.voice(at.voice))
        .map_or(0, |voice| voice.beats().len());
    let mut commands = Vec::new();
    for (offset, clip) in beats.iter().enumerate() {
        let index = at.beat.get().saturating_add(offset);
        let target = BeatPosition {
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
    }
    Ok(commands)
}

/// Most voices a measure can have edits in (voice 1 and 2).
const MAX_VOICES: usize = 2;

/// Commands that write the copied measures at `start`, inside the existing measures. The
/// target measures are replaced: tempo, time signature and section once per measure (they
/// are shared by all tracks), and in every track each voice's beats.
pub(super) fn paste_measures(
    song: &Song,
    start: MeasureIndex,
    tracks: &[Vec<MeasureClip>],
) -> Result<Vec<Command>, EditError> {
    let count = tracks.first().map_or(0, Vec::len);
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
    for (offset, clip) in tracks.first().into_iter().flatten().enumerate() {
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
    for (track_index, measures) in tracks.iter().enumerate() {
        let track_at = TrackIndex::new(track_index);
        let Some(track) = song.track(track_at) else {
            break;
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
                    track: track_at,
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
