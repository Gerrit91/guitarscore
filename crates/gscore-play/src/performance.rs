//! The engine-neutral performance of a song: every note as a guitarist plays it, with its
//! string, fret, timing and techniques, and no MIDI channels. Every playback engine works
//! from it: the MIDI and SoundFont path derives its [`Sequence`](crate::Sequence) from it
//! (`midi`), and a modelled guitar can play the same notes string by string (M22).

use gscore_model::{
    Fade, Fret, HarmonicKind, InstrumentKind, ModelError, Song, StringNumber, StrokeDirection,
    TrackIndex, TrackSound,
};

use crate::build;
use crate::pitch::PitchShape;
use crate::sequence::MeasureSpan;
use crate::tempo::TempoMap;

/// Which engine plays a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackEngine {
    /// The General MIDI path: a SoundFont or a MIDI device.
    #[default]
    SoundFont,
    /// Modelled strings of this kind (M22). MIDI output still plays the track's MIDI.
    Modelled(StringKind),
}

/// The kinds of modelled string instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringKind {
    /// Classical guitar.
    Nylon,
    /// Acoustic guitar with steel strings.
    Steel,
    /// Electric guitar.
    Electric,
    /// Electric bass.
    Bass,
}

impl StringKind {
    /// The modelled instrument for General MIDI `program` (0-based), if it is a guitar
    /// (24-31) or a bass (32-39).
    pub fn for_program(program: u8) -> Option<StringKind> {
        match program {
            24 => Some(StringKind::Nylon),
            25 => Some(StringKind::Steel),
            26..=31 => Some(StringKind::Electric),
            32..=39 => Some(StringKind::Bass),
            _ => None,
        }
    }
}

/// A track as it is performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerformanceTrack {
    /// The track in the song.
    pub track: TrackIndex,
    /// Its name.
    pub name: String,
    /// Its General MIDI sound (program, volume, pan, sends), also the mixer settings for
    /// any other engine.
    pub sound: TrackSound,
    /// String instrument, drums or other.
    pub kind: InstrumentKind,
    /// The sounding pitch (MIDI note number, capo included) of each open string, string 1
    /// (the highest) first. Empty for drums.
    pub strings: Vec<u8>,
    /// The engine that plays it.
    pub engine: TrackEngine,
}

/// How a note is set going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Attack {
    /// Picked (or plucked): a fresh strike of the string.
    #[default]
    Picked,
    /// Reached without a strike: a hammer-on, pull-off, legato, a trill's alternation or a
    /// grace note's hammer. The string keeps vibrating and only its length changes.
    Legato,
    /// Tapped with the fretting hand.
    LeftHandTap,
    /// Tapped with the picking hand.
    Tapped,
    /// Slapped with the thumb.
    Slapped,
    /// Popped (pulled and released).
    Popped,
    /// One step of a pick slide: the pick scraping along the string.
    Scrape,
}

/// One note of the performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedNote {
    /// Which entry of [`Performance::tracks`] it belongs to.
    pub track: usize,
    /// The string (1 is the highest); 0 when the note has no valid string.
    pub string: u8,
    /// The fret on that string, if known (not for drums, a scrape step or a note written
    /// below its stand-in string).
    pub fret: Option<u8>,
    /// The sounding pitch as a MIDI note number before any pitch movement; on a drum track
    /// the General MIDI drum sound.
    pub key: u8,
    /// How hard it is played, 1-127, with its dynamics and techniques applied (ghost notes
    /// and legato softer, accents and slaps louder).
    pub velocity: u8,
    /// When it starts and ends, in ticks. A damped note stops at its end; otherwise its
    /// end is where the string is released or struck again.
    pub start: u64,
    pub end: u64,
    /// Where its beat starts (a stroke or a grace note moves the note itself).
    pub onset: u64,
    /// How it is set going.
    pub attack: Attack,
    /// The direction of the stroke that strikes it, if its beat has one.
    pub stroke: Option<StrokeDirection>,
    /// Palm muted.
    pub palm_mute: bool,
    /// A dead (muted) note: no pitch, only the strike.
    pub dead: bool,
    /// A harmonic; the key is already the harmonic's pitch.
    pub harmonic: Option<HarmonicKind>,
    /// Let ring: it rings on until its string is struck again.
    pub let_ring: bool,
    /// It stops dead at its end (staccato, palm mute, dead notes, scrapes).
    pub damped: bool,
    /// A grace note.
    pub grace: bool,
    /// How its pitch moves (bends, slides, vibrato, whammy), if it does.
    pub pitch: Option<PitchShape>,
    /// A fade in, out or swell over the note.
    pub fade: Option<Fade>,
}

/// A song as it is played: what every engine plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Performance {
    /// The tracks that play.
    pub tracks: Vec<PerformanceTrack>,
    /// The notes, track by track in the order they were gathered (not sorted by time).
    pub notes: Vec<PlayedNote>,
    /// Tempo changes.
    pub tempo: TempoMap,
    /// The played measures, in playing order.
    pub measures: Vec<MeasureSpan>,
    /// The tick where the piece ends.
    pub end: u64,
    /// Notes left out because their pitch could not be worked out (invalid string, fret or
    /// a pitch above MIDI's range). The model lists such notes as issues.
    pub skipped_notes: usize,
    /// The indices of [`Performance::notes`] by start (ties in their order), for an engine
    /// that walks through the notes as time passes. [`Performance::index_by_start`]
    /// builds it; whoever changes the notes calls it again.
    pub by_start: Vec<usize>,
}

impl Performance {
    /// The performance of `tracks` of `song` (all tracks if `None`). Muted tracks do not
    /// play; with any soloed track, only soloed tracks play.
    pub fn new(song: &Song, tracks: Option<&[TrackIndex]>) -> Result<Self, ModelError> {
        // When each played measure starts, by its time signature.
        let (measures, end) = crate::timeline::measure_spans(song)?;

        let mut tempo_changes: Vec<(u64, u32)> = vec![(0, song.tempo())];
        for span in &measures {
            if let Some(bpm) = song.tempo_change(span.measure) {
                tempo_changes.push((span.start, bpm));
            }
        }

        let selected: Vec<TrackIndex> = match tracks {
            Some(tracks) => tracks.to_vec(),
            None => (0..song.track_count()).map(TrackIndex::new).collect(),
        };
        let soloed = song.any_soloed();
        let selected: Vec<TrackIndex> = selected
            .into_iter()
            .filter(|&track| match song.track(track) {
                // A track that does not exist is reported by `collect`.
                None => true,
                Some(view) => !view.is_muted() && (!soloed || view.is_soloed()),
            })
            .collect();
        let collected = build::collect(song, &selected, &measures, None, None)?;
        tempo_changes.extend(collected.tempo_changes);

        tempo_changes.sort_by_key(|&(tick, _)| tick);
        let mut tempo = TempoMap::new(song.tempo());
        for (tick, bpm) in tempo_changes {
            tempo.set(tick, bpm);
        }
        for span in &measures {
            tempo.hold_ticks(span.end, span.pause);
        }

        let mut performance = Performance {
            tracks: collected.tracks,
            notes: collected.notes,
            tempo,
            measures,
            end,
            skipped_notes: collected.skipped_notes,
            by_start: Vec::new(),
        };
        performance.index_by_start();
        Ok(performance)
    }

    /// Builds [`Performance::by_start`] for the current notes.
    pub fn index_by_start(&mut self) {
        let mut order: Vec<usize> = (0..self.notes.len()).collect();
        order.sort_by_key(|&index| self.notes.get(index).map_or(0, |note| note.start));
        self.by_start = order;
    }

    /// Lets the modelled strings play every guitar and bass track (a string instrument
    /// whose General MIDI program is a guitar or a bass); the others stay on the SoundFont.
    pub fn model_guitars(&mut self) {
        self.model_guitars_where(StringKind::for_program);
    }

    /// The same, the kind of string each preset starts from decided by `kind_of` (the
    /// app's per-preset tones, or [`StringKind::for_program`]).
    pub fn model_guitars_where(&mut self, kind_of: impl Fn(u8) -> Option<StringKind>) {
        for track in &mut self.tracks {
            if track.kind == InstrumentKind::Strings
                && !track.sound.percussion
                && let Some(kind) = kind_of(track.sound.program)
            {
                track.engine = TrackEngine::Modelled(kind);
            }
        }
    }
}

impl PerformanceTrack {
    /// The performed form of `track` of `song`.
    pub(crate) fn of(song: &Song, track: TrackIndex) -> Result<Self, ModelError> {
        let (Some(view), Some(sound)) = (song.track(track), song.played_sound(track)) else {
            return Err(ModelError::Invalid(format!("the song has no {track}")));
        };
        let strings = if view.is_percussion() {
            Vec::new()
        } else {
            (1..=view.string_count())
                .map_while(|number| {
                    let string = StringNumber::new(u8::try_from(number).ok()?).ok()?;
                    Some(
                        view.pitch_at(string, Fret::OPEN)
                            .map_or(0, |pitch| pitch.get()),
                    )
                })
                .collect()
        };
        Ok(PerformanceTrack {
            track,
            name: view.name().to_owned(),
            sound,
            kind: view.instrument_kind(),
            strings,
            engine: TrackEngine::SoundFont,
        })
    }
}
