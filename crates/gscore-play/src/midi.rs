//! The MIDI form of a [`Performance`]: a channel per track, borrowed channels for notes
//! whose pitch moves, that stop dead or fade (AGENTS.md section 5, decision 10), and the
//! timed messages that play every note on a General MIDI synthesizer.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::channels::ChannelAllocator;
use crate::event::{Channel, MidiEvent, controller};
use crate::performance::{Performance, PlayedNote};
use crate::pitch;
use crate::sequence::{SequenceTrack, TimedEvent, sound_events};
use crate::strokes;
use crate::tempo::PPQ;

/// How long a borrowed channel stays reserved after its note ends, for the release that
/// still sounds (and would follow the next note's bend): a quarter note.
const RELEASE_TICKS: u64 = PPQ as u64;
/// A General MIDI guitar has no palm-muted samples: a palm-muted note stands in as a
/// softer note (this share of its velocity, percent) cut dead after an eighth note at most.
const PALM_MUTE_PERCENT: u32 = 85;
const PALM_MUTE_TICKS: u64 = PPQ as u64 / 2;

/// What [`derive`] makes of a performance.
pub(crate) struct Midi {
    /// The tracks with their channels.
    pub(crate) tracks: Vec<SequenceTrack>,
    /// Each track's setup at tick 0, then every note's messages (not yet sorted).
    pub(crate) events: Vec<TimedEvent>,
    /// The channel each note of the performance plays on, in the order of its notes.
    pub(crate) channels: Vec<Channel>,
    /// Notes that play without their pitch movement (no free channel).
    pub(crate) degraded_pitch_bends: usize,
}

/// The MIDI messages of `performance`: the tracks' channels and sounds, then the notes.
pub(crate) fn derive(performance: &Performance) -> Midi {
    let mut allocator = ChannelAllocator::default();
    let mut tracks = Vec::with_capacity(performance.tracks.len());
    let mut events = Vec::new();
    for (number, track) in performance.tracks.iter().enumerate() {
        let channel = allocator.allocate(track.sound.percussion);
        for event in sound_events(channel, track.sound) {
            events.push(TimedEvent {
                tick: 0,
                track: number,
                event,
            });
        }
        tracks.push(SequenceTrack {
            track: track.track,
            name: track.name.clone(),
            channel,
            sound: track.sound,
        });
    }
    let drums: Vec<bool> = performance
        .tracks
        .iter()
        .map(|track| track.sound.percussion)
        .collect();
    let notes: Vec<PlayedNote> = performance
        .notes
        .iter()
        .map(|note| {
            let note = soundfont_note(note);
            if drums.get(note.track).copied().unwrap_or(false) {
                drum_note(note)
            } else {
                note
            }
        })
        .collect();
    let base: Vec<Channel> = notes
        .iter()
        .map(|note| {
            tracks
                .get(note.track)
                .map_or(Channel::PERCUSSION, |track| track.channel)
        })
        .collect();
    let mut channels = base.clone();
    let degraded_pitch_bends =
        assign_pitch_channels(&notes, &mut channels, allocator.free_channels());
    emit(&notes, &base, &channels, &tracks, &mut events);
    Midi {
        tracks,
        events,
        channels,
        degraded_pitch_bends,
    }
}

/// `note` as a SoundFont plays it: a palm mute shortened and softened (the modelled strings
/// damp the string instead and play the performance's note as it is).
fn soundfont_note(note: &PlayedNote) -> PlayedNote {
    let mut note = *note;
    if note.palm_mute && !note.dead {
        note.velocity = crate::effects::scale(note.velocity, PALM_MUTE_PERCENT);
        let length = note.end.saturating_sub(note.start);
        note.end = note.start.saturating_add(length.min(PALM_MUTE_TICKS));
        note.damped = true;
    }
    note
}

/// A drum track's `note` as MIDI plays it: always on the kit's own channel. A borrowed
/// channel is a melodic one (no drums there), and the mixer hears a drum track only on its
/// own channel, so a borrowed hit was lost. Drums need no borrowed channel: they never bend,
/// a drum rings out whatever its note-off, and a fade is left out.
fn drum_note(mut note: PlayedNote) -> PlayedNote {
    note.pitch = None;
    note.damped = false;
    note.fade = None;
    note
}

/// Gives every note with a pitch shape, a damped end or a fade a borrowed channel in
/// `channels`, reusing channels as notes end. Returns how many notes with a pitch shape
/// had to go without one.
fn assign_pitch_channels(
    notes: &[PlayedNote],
    channels: &mut [Channel],
    free: Vec<Channel>,
) -> usize {
    let mut free: VecDeque<Channel> = free.into();
    let mut in_use: BinaryHeap<Reverse<(u64, Channel)>> = BinaryHeap::new();
    let mut order: Vec<usize> = notes
        .iter()
        .enumerate()
        .filter(|(_, note)| note.pitch.is_some() || note.damped || note.fade.is_some())
        .map(|(index, _)| index)
        .collect();
    order.sort_by_key(|&index| {
        notes
            .get(index)
            .map_or((0, 0), |note| (note.start, note.end))
    });

    let mut degraded = 0_usize;
    for index in order {
        let Some(note) = notes.get(index) else {
            continue;
        };
        let start = note.start;
        while let Some(&Reverse((end, channel))) = in_use.peek() {
            if end <= start {
                let _ = in_use.pop();
                if !free.iter().any(|&free| free == channel) {
                    free.push_back(channel);
                }
            } else {
                break;
            }
        }
        let Some(channel) = free.pop_front() else {
            // A damped note without a channel only keeps its release; a pitch shape is
            // lost, which is reported.
            if note.pitch.is_some() {
                degraded = degraded.saturating_add(1);
            }
            continue;
        };
        if let Some(slot) = channels.get_mut(index) {
            *slot = channel;
            // A bent note's release rings on after its end: the channel stays its own
            // until that has faded (a damped note is cut at once).
            let release = if note.damped { 0 } else { RELEASE_TICKS };
            in_use.push(Reverse((
                note.end
                    .max(note.start.saturating_add(1))
                    .saturating_add(release),
                channel,
            )));
        }
    }
    degraded
}

/// The MIDI events of `notes`, each on its channel in `channels` (`base` being its track's
/// own): a borrowed channel's setup and pitch movement, the note-on and note-off (and the
/// cut of a damped note).
fn emit(
    notes: &[PlayedNote],
    base: &[Channel],
    channels: &[Channel],
    tracks: &[SequenceTrack],
    events: &mut Vec<TimedEvent>,
) {
    for ((note, &own), &channel) in notes.iter().zip(base).zip(channels) {
        let borrowed = channel != own;
        let mut push = |tick: u64, event: MidiEvent| {
            events.push(TimedEvent {
                tick,
                track: note.track,
                event,
            });
        };
        if borrowed && let Some(track) = tracks.get(note.track) {
            for event in sound_events(channel, track.sound) {
                push(note.start, event);
            }
            let range = note
                .pitch
                .map_or(pitch::BEND_RANGE_SEMITONES, |shape| shape.range());
            for event in pitch::bend_range_setup(channel, range) {
                push(note.start, event);
            }
        }
        if borrowed && let Some(fade) = note.fade {
            for (tick, event) in strokes::fade_events(fade, note.start, note.end, channel) {
                push(tick, event);
            }
        }
        if borrowed && let Some(shape) = note.pitch {
            for (tick, event) in shape.events(channel, note.start) {
                push(tick, event);
            }
        }
        push(
            note.start,
            MidiEvent::NoteOn {
                channel,
                key: note.key,
                velocity: note.velocity,
            },
        );
        let end = note.end.max(note.start.saturating_add(1));
        push(
            end,
            MidiEvent::NoteOff {
                channel,
                key: note.key,
            },
        );
        if borrowed && note.damped {
            // Cut the release: the note stops at its end.
            push(
                end,
                MidiEvent::Control {
                    channel,
                    controller: controller::EXPRESSION,
                    value: 0,
                },
            );
        }
        // No pitch reset at the note's end: its release still sounds and would be pulled
        // back. The channel is set again when it is next borrowed (see
        // `assign_pitch_channels`, which waits for the release first).
    }
}
