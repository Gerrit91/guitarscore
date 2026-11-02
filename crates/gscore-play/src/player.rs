//! The `Player` trait output backends implement, and the [`Scheduler`] that decides which
//! events are due. The scheduler is pure (the backend passes in the time), so it can be
//! tested without a clock or a device.

use std::sync::Arc;

use crate::event::MidiEvent;
use crate::sequence::{Sequence, TimedEvent};

/// Why playback failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayError {
    /// The output device could not be used.
    Device(String),
    /// The player is gone (its thread stopped).
    Disconnected,
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayError::Device(e) => write!(f, "playback: {e}"),
            PlayError::Disconnected => f.write_str("the player stopped unexpectedly"),
        }
    }
}

impl std::error::Error for PlayError {}

/// What a player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    /// Not playing; the position is at the start.
    Stopped,
    /// Playing.
    Playing,
    /// Not playing; the position is kept.
    Paused,
}

/// An output that plays sequences (a MIDI port, or the internal synthesizer).
pub trait Player {
    /// Makes `sequence` the one to play, stopped at its start.
    fn load(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError>;
    /// Starts or resumes playing.
    fn play(&mut self) -> Result<(), PlayError>;
    /// Pauses, keeping the position.
    fn pause(&mut self) -> Result<(), PlayError>;
    /// Stops and goes back to the start.
    fn stop(&mut self) -> Result<(), PlayError>;
    /// Moves to `tick`.
    fn seek(&mut self, tick: u64) -> Result<(), PlayError>;
    /// Plays faster (above 1) or slower (below 1) than written.
    fn set_tempo_scale(&mut self, scale: f32) -> Result<(), PlayError>;
    /// Sends one MIDI message at once, without changing the sequence. Used for live mixer
    /// changes (volume and pan), which must not stop or rebuild playback.
    fn send_event(&mut self, event: MidiEvent) -> Result<(), PlayError>;
    /// Swaps in a rebuilt `sequence` at the current position without stopping or silencing
    /// (see [`Scheduler::replace`]); like [`Player::load`] if nothing is loaded.
    fn replace(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError>;
    /// The current position in ticks.
    fn position(&self) -> u64;
    /// Playing, paused or stopped.
    fn state(&self) -> PlayState;
}

/// Allowed tempo scale.
pub const TEMPO_SCALE_RANGE: (f32, f32) = (0.1, 4.0);

/// Walks through a sequence as time passes: give it the elapsed time, get the due events.
#[derive(Debug, Clone)]
pub struct Scheduler {
    sequence: Arc<Sequence>,
    /// Index of the next event to send.
    next: usize,
    /// Where playback (re)started, in ticks, and that tick's time in microseconds.
    anchor_tick: u64,
    anchor_micros: u64,
    tempo_scale: f32,
    position: u64,
}

impl Scheduler {
    /// A scheduler at the start of `sequence`.
    pub fn new(sequence: Arc<Sequence>) -> Self {
        Scheduler {
            sequence,
            next: 0,
            anchor_tick: 0,
            anchor_micros: 0,
            tempo_scale: 1.0,
            position: 0,
        }
    }

    /// The sequence.
    pub fn sequence(&self) -> &Arc<Sequence> {
        &self.sequence
    }

    /// The current position in ticks.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether the end has been reached.
    pub fn finished(&self) -> bool {
        self.position >= self.sequence.end
    }

    /// Moves to `tick`; `elapsed_micros` is the backend's clock now. Events before `tick`
    /// are skipped, except setup (program, volume, pan), which must still reach the synth:
    /// they are returned so the backend sends them.
    pub fn seek(&mut self, tick: u64, elapsed_micros: u64) -> Vec<TimedEvent> {
        self.jump(tick, elapsed_micros);
        if self.position == 0 {
            // From the start, the setup is sent with the other events anyway.
            return Vec::new();
        }
        self.setup().copied().collect()
    }

    /// Moves to `tick` like [`Scheduler::seek`], without collecting the setup: for callers
    /// that must not allocate (an audio callback). Send [`Scheduler::setup`] afterwards
    /// unless `tick` is 0.
    pub fn jump(&mut self, tick: u64, elapsed_micros: u64) {
        let tick = tick.min(self.sequence.end);
        self.next = self.sequence.events.partition_point(|e| e.tick < tick);
        self.anchor(tick, elapsed_micros);
    }

    /// The setup events (program, volume, pan at tick 0), which must reach the synth before
    /// playing from anywhere but the start.
    pub fn setup(&self) -> impl Iterator<Item = &TimedEvent> {
        self.sequence
            .events
            .iter()
            .take_while(|e| e.tick == 0)
            .filter(|e| !e.event.is_note())
    }

    /// Swaps in `sequence` where this one is, keeping the position, clock and tempo scale:
    /// for a rebuilt sequence whose events differ only in their values (a mixer change), so
    /// nothing is sent twice or skipped. If the number of events differs, the next event is
    /// looked up by tick instead. Returns the sequence it replaced.
    pub fn replace(&mut self, sequence: Arc<Sequence>) -> Arc<Sequence> {
        if sequence.events.len() != self.sequence.events.len() {
            self.next = sequence.events.partition_point(|e| e.tick < self.position);
        }
        std::mem::replace(&mut self.sequence, sequence)
    }

    /// Changes the tempo scale (clamped to [`TEMPO_SCALE_RANGE`]) from now on.
    pub fn set_tempo_scale(&mut self, scale: f32, elapsed_micros: u64) {
        let scale = if scale.is_finite() {
            scale.clamp(TEMPO_SCALE_RANGE.0, TEMPO_SCALE_RANGE.1)
        } else {
            1.0
        };
        self.anchor(self.position, elapsed_micros);
        self.tempo_scale = scale;
    }

    /// Restarts the clock at the current position (after a pause), `elapsed_micros` being
    /// the backend's clock now.
    pub fn resume(&mut self, elapsed_micros: u64) {
        self.anchor(self.position, elapsed_micros);
    }

    /// The events due at `elapsed_micros` on the backend's clock, in order; the position
    /// moves there.
    pub fn advance(&mut self, elapsed_micros: u64) -> &[TimedEvent] {
        let played = elapsed_micros.saturating_sub(self.anchor_micros);
        // Scaled time: at scale 2, one real microsecond is two microseconds of the piece.
        let piece_micros = (played as f64 * f64::from(self.tempo_scale)) as u64;
        let tempo = &self.sequence.tempo;
        let start = tempo.micros_at(self.anchor_tick);
        let tick = tempo
            .tick_at(start.saturating_add(piece_micros))
            .min(self.sequence.end);
        self.position = self.position.max(tick);
        let first = self.next;
        let events = &self.sequence.events;
        let due = events
            .get(first..)
            .map_or(0, |rest| rest.partition_point(|e| e.tick <= self.position));
        self.next = first.saturating_add(due);
        events.get(first..self.next).unwrap_or_default()
    }

    fn anchor(&mut self, tick: u64, elapsed_micros: u64) {
        self.anchor_tick = tick;
        self.anchor_micros = elapsed_micros;
        self.position = tick;
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;
    use crate::event::{Channel, MidiEvent};
    use crate::tempo::{PPQ, TempoMap};

    /// One note per quarter for a 4/4 bar at 120 BPM (a quarter is 500 ms).
    fn quarters() -> Arc<Sequence> {
        let channel = Channel::new(0).unwrap();
        let events = (0..4_u64)
            .map(|i| TimedEvent {
                tick: i * u64::from(PPQ),
                track: 0,
                event: MidiEvent::NoteOn {
                    channel,
                    key: 60,
                    velocity: 90,
                },
            })
            .collect();
        Arc::new(Sequence {
            tracks: Vec::new(),
            events,
            tempo: TempoMap::new(120),
            measures: Vec::new(),
            end: 4 * u64::from(PPQ),
            skipped_notes: 0,
            degraded_pitch_bends: 0,
        })
    }

    #[test]
    fn events_come_due_as_time_passes() {
        let mut scheduler = Scheduler::new(quarters());
        assert_eq!(scheduler.advance(0).len(), 1);
        assert_eq!(scheduler.advance(400_000).len(), 0);
        assert_eq!(scheduler.advance(1_000_000).len(), 2);
        assert_eq!(scheduler.advance(10_000_000).len(), 1);
        assert!(scheduler.finished());
    }

    #[test]
    fn setup_is_sent_once_whether_starting_or_seeking() {
        let channel = Channel::new(0).unwrap();
        let mut sequence = (*quarters()).clone();
        sequence.events.insert(
            0,
            TimedEvent {
                tick: 0,
                track: 0,
                event: MidiEvent::ProgramChange {
                    channel,
                    program: 25,
                },
            },
        );
        let sequence = Arc::new(sequence);
        let mut scheduler = Scheduler::new(Arc::clone(&sequence));
        assert!(scheduler.seek(0, 0).is_empty());
        let first = scheduler.advance(0);
        assert_eq!(first.iter().filter(|e| !e.event.is_note()).count(), 1);
        // Seeking into the piece skips tick 0, so the setup comes from `seek`.
        let mut later = Scheduler::new(Arc::clone(&sequence));
        assert_eq!(later.seek(u64::from(PPQ), 0).len(), 1);
        // `jump` moves the same way and leaves the setup to the caller.
        let mut jumped = Scheduler::new(sequence);
        jumped.jump(u64::from(PPQ), 0);
        assert_eq!(jumped.position(), later.position());
        assert_eq!(jumped.setup().count(), 1);
        assert_eq!(jumped.advance(0).len(), later.advance(0).len());
    }

    #[test]
    fn a_replaced_sequence_continues_where_playback_is() {
        let mut scheduler = Scheduler::new(quarters());
        assert_eq!(scheduler.advance(0).len(), 1);
        assert_eq!(scheduler.advance(600_000).len(), 1);
        // The same events with other values (a mixer change): nothing is sent again.
        let mut rebuilt = (*quarters()).clone();
        for event in &mut rebuilt.events {
            if let MidiEvent::NoteOn { velocity, .. } = &mut event.event {
                *velocity = 30;
            }
        }
        let old = scheduler.replace(Arc::new(rebuilt));
        assert_eq!(old.events.len(), 4);
        assert_eq!(scheduler.position(), u64::from(PPQ) * 600 / 500);
        let next = scheduler.advance(1_000_000);
        assert_eq!(next.len(), 1, "the third note, from the new sequence");
        assert!(matches!(
            next[0].event,
            MidiEvent::NoteOn { velocity: 30, .. }
        ));
        // A sequence with other events: the next event is found by tick.
        let mut shorter = (*quarters()).clone();
        shorter.events.remove(0);
        let _ = scheduler.replace(Arc::new(shorter));
        assert!(!scheduler.advance(1_600_000).is_empty(), "playback goes on");
        assert!(scheduler.finished() || scheduler.advance(2_000_000).is_empty());
    }

    #[test]
    fn seeking_and_tempo_scale_move_the_clock() {
        let mut scheduler = Scheduler::new(quarters());
        let _ = scheduler.seek(2 * u64::from(PPQ), 5_000_000);
        assert_eq!(scheduler.position(), 2 * u64::from(PPQ));
        // Twice as fast: 250 ms of real time is one quarter.
        scheduler.set_tempo_scale(2.0, 5_000_000);
        assert_eq!(scheduler.advance(5_000_000).len(), 1);
        assert_eq!(scheduler.advance(5_250_000).len(), 1);
        assert!(!scheduler.finished());
        assert!(scheduler.advance(5_500_000).is_empty());
        assert!(scheduler.finished());
        scheduler.set_tempo_scale(f32::NAN, 0);
        let _ = scheduler.seek(u64::MAX, 0);
        assert!(
            scheduler.finished(),
            "seeking past the end stops at the end"
        );
    }
}
