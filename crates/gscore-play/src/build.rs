//! Building a sequence in two passes: [`collect`] walks the played measures of each track
//! and gathers the notes with their timing, velocity and effects; [`emit`] turns them into
//! MIDI events (borrowed channels for pitch movement, note-on and note-off). The whole
//! song and a one-beat excerpt share both passes.

use std::collections::HashMap;

use gscore_model::{
    BeatKind, BendKind, Duration, ModelError, NoteKind, SlideKind, Song, TrackIndex,
};

use crate::channels::ChannelAllocator;
use crate::curve::Curve;
use crate::effects;
use crate::event::{MidiEvent, controller};
use crate::pitch;
use crate::sequence::{MeasureSpan, SequenceTrack, TimedEvent, sound_events, ticks};
use crate::sounding::{self, Sounding};
use crate::swing;
use crate::tempo::PPQ;

/// Ticks in a whole note.
const WHOLE: u64 = 4 * PPQ as u64;
/// Dead (muted) notes sound this long at most: a 32nd note.
const DEAD_NOTE_TICKS: u64 = WHOLE / 32;
/// Dead notes are played softer, at this share of their velocity (percent).
const DEAD_NOTE_VELOCITY_PERCENT: u16 = 60;
/// A tied note continues the previous one if that ends within this many ticks of its start
/// (rounding of tuplets).
const TIE_TOLERANCE: u64 = 2;

/// What [`collect`] gathers.
pub(crate) struct Collected {
    /// The tracks that play, with their channels.
    pub(crate) tracks: Vec<SequenceTrack>,
    /// Each track's sound setup at tick 0.
    pub(crate) setup: Vec<TimedEvent>,
    /// The notes, with effects applied and pitch channels assigned.
    pub(crate) notes: Vec<Sounding>,
    /// Tempo changes on beats.
    pub(crate) tempo_changes: Vec<(u64, u32)>,
    /// Notes left out because their pitch could not be worked out.
    pub(crate) skipped_notes: usize,
    /// Notes that play without their pitch movement (no free channel).
    pub(crate) degraded_pitch_bends: usize,
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
    let mut channels = ChannelAllocator::default();
    let mut tracks = Vec::with_capacity(selected.len());
    let mut setup = Vec::new();
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
        let channel = channels.allocate(sound.percussion);
        let number = tracks.len();
        tracks.push(SequenceTrack {
            track: track_index,
            name: view.name().to_owned(),
            channel,
            sound,
        });
        for event in sound_events(channel, sound) {
            setup.push(TimedEvent {
                tick: 0,
                track: number,
                event,
            });
        }

        // The note currently sounding on each string, to extend it on a tie.
        let mut on_string: HashMap<u8, usize> = HashMap::new();
        for span in measures {
            let Some(measure) = view.measure(span.measure) else {
                continue;
            };
            let feel = song.feel(span.measure).unwrap_or_default();
            let measure_ticks = span.end.saturating_sub(span.start);
            for voice in measure.voices() {
                let mut onset = Duration::new(0, 1)?;
                for beat in voice.beats() {
                    if beat.kind() == BeatKind::Empty {
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
                        continue;
                    }
                    let first_note = notes.len();
                    for note in beat.notes() {
                        let kind = note.kind();
                        let string = note.string().map(|s| s.get()).unwrap_or(0);
                        if kind == NoteKind::Tie && fresh_at != Some(start) {
                            let tied = on_string
                                .get(&string)
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
                        let mut sounding = Sounding {
                            track: number,
                            channel,
                            play_channel: channel,
                            key: pitch.get(),
                            velocity,
                            start,
                            end,
                            string,
                            slide: sounding::choose_slide(&slides),
                            bend,
                            shape: None,
                            vibrato: note.vibrato().or(beat.vibrato()).filter(|_| pitched),
                            let_ring: note.is_let_ring(),
                            tremolo: note.has_tremolo_picking(),
                            onset: start,
                            damp: false,
                            bend_curve: note.bend_shape().filter(|_| pitched).and_then(|shape| {
                                Curve::new(shape.points.iter().map(|p| (p.position, p.value)))
                            }),
                            whammy: beat.whammy_shape().filter(|_| pitched).and_then(|shape| {
                                Curve::new(shape.points.iter().map(|p| (p.position, p.value)))
                            }),
                        };
                        effects::shape_note(&mut sounding, &note);
                        on_string.insert(string, notes.len());
                        notes.push(sounding);
                    }
                    if let Some(direction) = beat.brush_stroke()
                        && let Some(chord) = notes.get_mut(first_note..)
                    {
                        effects::brush(chord, direction);
                    }
                }
            }
        }
    }

    sounding::apply_slide_targets(&mut notes);
    effects::let_ring(&mut notes);
    if let Some(keep) = keep {
        notes.retain(|note| keep(note));
    }
    let mut notes = effects::tremolo_picking(notes);
    sounding::apply_bends(&mut notes);
    effects::apply_vibrato(&mut notes);
    let degraded_pitch_bends =
        sounding::assign_pitch_channels(&mut notes, channels.free_channels());
    Ok(Collected {
        tracks,
        setup,
        notes,
        tempo_changes,
        skipped_notes,
        degraded_pitch_bends,
    })
}

/// The MIDI events of `notes`: a borrowed channel's setup and pitch movement, the note-on
/// and note-off (and the cut of a damped note).
pub(crate) fn emit(notes: &[Sounding], tracks: &[SequenceTrack], events: &mut Vec<TimedEvent>) {
    for note in notes {
        let borrowed = note.play_channel != note.channel;
        if borrowed && let Some(track) = tracks.get(note.track) {
            for event in sound_events(note.play_channel, track.sound) {
                events.push(TimedEvent {
                    tick: note.start,
                    track: note.track,
                    event,
                });
            }
            let range = note
                .shape
                .map_or(pitch::BEND_RANGE_SEMITONES, |shape| shape.range());
            for event in pitch::bend_range_setup(note.play_channel, range) {
                events.push(TimedEvent {
                    tick: note.start,
                    track: note.track,
                    event,
                });
            }
        }
        if borrowed && let Some(shape) = note.shape {
            for (tick, event) in shape.events(note.play_channel, note.start) {
                events.push(TimedEvent {
                    tick,
                    track: note.track,
                    event,
                });
            }
        }
        events.push(TimedEvent {
            tick: note.start,
            track: note.track,
            event: MidiEvent::NoteOn {
                channel: note.play_channel,
                key: note.key,
                velocity: note.velocity,
            },
        });
        let end = note.end.max(note.start.saturating_add(1));
        events.push(TimedEvent {
            tick: end,
            track: note.track,
            event: MidiEvent::NoteOff {
                channel: note.play_channel,
                key: note.key,
            },
        });
        if borrowed && note.damp {
            // Cut the release: the note stops at its end.
            events.push(TimedEvent {
                tick: end,
                track: note.track,
                event: MidiEvent::Control {
                    channel: note.play_channel,
                    controller: controller::EXPRESSION,
                    value: 0,
                },
            });
        }
        // No pitch reset at the note's end: its release still sounds and would be pulled
        // back. The channel is set again when it is next borrowed (see
        // `sounding::assign_pitch_channels`, which waits for the release first).
    }
}
