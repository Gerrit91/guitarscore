//! The notes a sequence plays and the pitch shapes they need: bends and slides, and which
//! channel each note plays on.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use gscore_model::{Fade, SlideKind, StrokeDirection, TremoloSpeed, TrillSpeed, Vibrato};

use crate::curve::Curve;
use crate::event::Channel;
use crate::pitch::PitchShape;
use crate::tempo::PPQ;

/// An upward bend reaches its target within a quarter note (or the note's length): Guitar
/// Pro's own bends take about a beat.
pub(super) const BEND_RAMP_TICKS: u64 = PPQ as u64;
/// How long a borrowed channel stays reserved after its note ends, for the release that
/// still sounds (and would follow the next note's bend): a quarter note.
const RELEASE_TICKS: u64 = PPQ as u64;
/// A slide into a note reaches its pitch within an eighth note (or the note's length).
pub(super) const SLIDE_IN_RAMP_TICKS: u64 = PPQ as u64 / 2;
/// How far an "into" slide starts from its note when the previous note is unknown: a whole
/// step.
const UNKNOWN_SLIDE_IN_SEMITONES: i32 = 2;

/// A note while the sequence is built.
#[derive(Debug, Clone, Copy)]
pub(super) struct Sounding {
    /// The sequence track it belongs to.
    pub(super) track: usize,
    /// The track's own channel.
    pub(super) channel: Channel,
    /// The channel the note plays on: the track's, or a borrowed one.
    pub(super) play_channel: Channel,
    pub(super) key: u8,
    pub(super) velocity: u8,
    pub(super) start: u64,
    pub(super) end: u64,
    /// The string, to find the next note of a slide. 0 when the note has no valid string.
    pub(super) string: u8,
    /// The selected slide, if any.
    pub(super) slide: Option<SlideMove>,
    /// The bend in quarter tones and whether it releases, if any.
    pub(super) bend: Option<(i16, bool)>,
    /// The pitch shape, once slide targets and channels are known.
    pub(super) shape: Option<PitchShape>,
    /// The note's (or its beat's) vibrato, if any.
    pub(super) vibrato: Option<Vibrato>,
    /// Whether the note rings on until the next note on its string.
    pub(super) let_ring: bool,
    /// How fast the note is played as repeated picks, if it is tremolo picked.
    pub(super) tremolo: Option<TremoloSpeed>,
    /// The note's trill: the other key and how fast it alternates.
    pub(super) trill: Option<(u8, TrillSpeed)>,
    /// A pick slide along the string and its direction.
    pub(super) pick_slide: Option<StrokeDirection>,
    /// The beat is played as a rasgueado, repeated by [`crate::effects::rasgueados`].
    pub(super) rasgueado: bool,
    /// Where its beat starts (a brush stroke may delay the note itself).
    pub(super) onset: u64,
    /// Whether the note stops dead at its end (staccato, palm mute) instead of fading
    /// with the SoundFont's release: it plays on a borrowed channel whose expression drops
    /// to zero then.
    pub(super) damp: bool,
    /// The bend's drawn curve, and the beat's whammy curve, if any.
    pub(super) bend_curve: Option<Curve>,
    pub(super) whammy: Option<Curve>,
    /// A fade in, out or swell: it plays on a borrowed channel whose expression follows
    /// the fade over the note.
    pub(super) fade: Option<Fade>,
}

/// Which slide of a note the sequencer plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SlideMove {
    /// To the next note on the same string.
    To,
    /// In from a higher note.
    IntoAbove,
    /// In from a lower note.
    IntoBelow,
    /// Out upwards, to no particular note.
    OutUp,
    /// Out downwards, to no particular note.
    OutDown,
}

/// Picks the slide to play when a note has several (outgoing, then into, then out).
pub(super) fn choose_slide(slides: &[SlideKind]) -> Option<SlideMove> {
    let outgoing = || {
        slides.iter().find_map(|kind| match kind {
            SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo => Some(SlideMove::To),
            _ => None,
        })
    };
    let into = || {
        slides.iter().find_map(|kind| match kind {
            SlideKind::IntoFromAbove => Some(SlideMove::IntoAbove),
            SlideKind::IntoFromBelow => Some(SlideMove::IntoBelow),
            _ => None,
        })
    };
    let out = || {
        slides.iter().find_map(|kind| match kind {
            SlideKind::OutUpwards => Some(SlideMove::OutUp),
            SlideKind::OutDownwards => Some(SlideMove::OutDown),
            _ => None,
        })
    };
    outgoing().or_else(into).or_else(out)
}

/// Gives each slide its target pitch: the next note on the same string for an outgoing
/// slide, the previous note (or a whole step) for an incoming one.
pub(super) fn apply_slide_targets(notes: &mut [Sounding]) {
    let mut groups: HashMap<(usize, u8), Vec<usize>> = HashMap::new();
    for (index, note) in notes.iter().enumerate() {
        groups
            .entry((note.track, note.string))
            .or_default()
            .push(index);
    }
    for group in groups.values_mut() {
        group.sort_by_key(|&index| {
            notes
                .get(index)
                .map_or((0, 0), |note| (note.start, note.end))
        });
        for position in 0..group.len() {
            let Some(&index) = group.get(position) else {
                continue;
            };
            let Some(note) = notes.get(index) else {
                continue;
            };
            let Some(slide) = note.slide else {
                continue;
            };
            let length = note.end.saturating_sub(note.start);
            let previous = position
                .checked_sub(1)
                .and_then(|p| group.get(p))
                .and_then(|&i| notes.get(i));
            let next = group
                .get(position.saturating_add(1))
                .and_then(|&i| notes.get(i))
                .filter(|next| next.start >= note.end);
            let shape = match slide {
                SlideMove::To => next.map(|next| {
                    PitchShape::slide_to(
                        i32::from(next.key).saturating_sub(i32::from(note.key)),
                        length,
                    )
                }),
                SlideMove::IntoAbove | SlideMove::IntoBelow => {
                    let above = slide == SlideMove::IntoAbove;
                    let delta = previous
                        .map(|previous| i32::from(previous.key).saturating_sub(i32::from(note.key)))
                        // Keep the previous note only if it lies in the slide's direction.
                        .filter(|delta| if above { *delta > 0 } else { *delta < 0 })
                        .unwrap_or(if above {
                            UNKNOWN_SLIDE_IN_SEMITONES
                        } else {
                            UNKNOWN_SLIDE_IN_SEMITONES.saturating_neg()
                        });
                    Some(PitchShape::slide_in(delta, length, SLIDE_IN_RAMP_TICKS))
                }
                SlideMove::OutUp => Some(PitchShape::slide_out(true, length)),
                SlideMove::OutDown => Some(PitchShape::slide_out(false, length)),
            };
            if let Some(shape) = shape
                && let Some(note) = notes.get_mut(index)
            {
                note.shape = Some(shape);
            }
        }
    }
}

/// Notes without a slide keep their bend, if any (its drawn curve when it has one), and
/// every note of a beat with the whammy bar follows its curve.
pub(super) fn apply_bends(notes: &mut [Sounding]) {
    for note in notes {
        let length = note.end.saturating_sub(note.start);
        if note.whammy.is_some() {
            let base = note.shape.unwrap_or_else(PitchShape::steady);
            note.shape = Some(base.with_curves(None, note.whammy, length));
        }
        if note.slide.is_some() && note.shape.is_some_and(|shape| !shape.is_steady()) {
            continue;
        }
        if let Some(curve) = note.bend_curve {
            let base = note.shape.unwrap_or_else(PitchShape::steady);
            note.shape = Some(base.with_curves(Some(curve), None, length));
            continue;
        }
        if note.shape.is_some() {
            continue;
        }
        let Some((quarter_tones, release)) = note.bend else {
            continue;
        };
        let length = note.end.saturating_sub(note.start);
        note.shape = Some(if release {
            PitchShape::release(quarter_tones, length)
        } else {
            PitchShape::up(quarter_tones, length, BEND_RAMP_TICKS)
        });
    }
}

/// Gives every note with a pitch shape a borrowed channel, reusing channels as notes end.
/// Returns how many notes had to go without one.
pub(super) fn assign_pitch_channels(notes: &mut [Sounding], free: Vec<Channel>) -> usize {
    let mut free: VecDeque<Channel> = free.into();
    let mut in_use: BinaryHeap<Reverse<(u64, Channel)>> = BinaryHeap::new();
    let mut order: Vec<usize> = notes
        .iter()
        .enumerate()
        .filter(|(_, note)| note.shape.is_some() || note.damp || note.fade.is_some())
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
            if note.shape.is_some() {
                degraded = degraded.saturating_add(1);
            }
            continue;
        };
        if let Some(note) = notes.get_mut(index) {
            note.play_channel = channel;
            // A bent note's release rings on after its end: the channel stays its own
            // until that has faded (a damped note is cut at once).
            let release = if note.damp { 0 } else { RELEASE_TICKS };
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
