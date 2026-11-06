//! What every backend does with the player's commands: play, pause, stop, seek and change
//! the tempo of a [`Scheduler`], sending the events to a [`Sink`]. Shared by the MIDI output
//! thread and the internal synth, so both behave the same.
//!
//! Nothing here allocates, waits or does I/O (beyond what the sink does), so the synth can
//! run it in the audio callback.

use std::sync::Arc;

use gscore_guitar::{BLOCKS, BlockSettings, StringModel};

use crate::chains::ChainSet;
use gscore_play::{MidiEvent, PlayState, Scheduler, Sequence, StringKind, TimedEvent};

/// A command for a backend.
#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
// `Quit` ends the MIDI thread.
// The chain's knob values make one variant large; boxing them would free memory in the
// audio callback, which is worse than copying a few hundred bytes through the queue.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Command {
    Load(Arc<Sequence>),
    Play,
    Pause,
    Stop,
    Seek(u64),
    TempoScale(f32),
    /// Sends one MIDI message at once (a live mixer change).
    Send(MidiEvent),
    /// Swaps in a rebuilt sequence where playback is (a mixer change), without stopping.
    Replace(Arc<Sequence>),
    /// Ends the backend (the transport itself ignores it).
    Quit,
    /// Changes how the modelled strings of one kind sound (the synth engine carries it
    /// out; the transport and MIDI output ignore it).
    StringModel(StringKind, StringModel),
    /// Gives the strings of one kind a prepared effect chain (`None`: none); the synth
    /// engine carries it out.
    Chain(StringKind, Option<Box<ChainSet>>),
    /// New knob values for one kind's chain.
    ChainSettings(StringKind, [BlockSettings; BLOCKS]),
    /// Gives the modelled tracks a prepared reverb bus (`None`: none), which every track
    /// sends into with its mixer reverb amount.
    ReverbBus(Option<Box<gscore_guitar::Chain>>),
    /// New knob values, level and mute for the reverb bus.
    ReverbBusSettings([BlockSettings; BLOCKS], u8, bool),
}

/// Where a transport's events go.
pub(crate) trait Sink {
    /// Plays one event now.
    fn send(&mut self, event: MidiEvent);
    /// Plays one event of the sequence now (its track is known).
    fn send_timed(&mut self, event: &TimedEvent) {
        self.send(event.event);
    }
    /// Ends every sounding note and stale pitch bend of `sequence`'s channels, keeping
    /// program, volume and pan (a pause resumes with the same mix).
    fn silence(&mut self, sequence: &Sequence);
}

/// A sequence being played, paused or stopped.
#[derive(Debug)]
pub(crate) struct Transport {
    scheduler: Option<Scheduler>,
    state: PlayState,
}

impl Default for Transport {
    fn default() -> Self {
        Transport {
            scheduler: None,
            state: PlayState::Stopped,
        }
    }
}

impl Transport {
    /// Playing, paused or stopped.
    pub(crate) fn state(&self) -> PlayState {
        self.state
    }

    /// The sequence loaded, if any.
    pub(crate) fn sequence(&self) -> Option<&Arc<Sequence>> {
        self.scheduler.as_ref().map(Scheduler::sequence)
    }

    /// The position in ticks (0 without a sequence).
    pub(crate) fn position(&self) -> u64 {
        self.scheduler.as_ref().map_or(0, Scheduler::position)
    }

    /// Carries out `command` at `now` (microseconds on the backend's clock). Returns the
    /// sequence a `Load` replaced, so the caller decides where it is freed (not in an audio
    /// callback).
    pub(crate) fn handle(
        &mut self,
        command: Command,
        now: u64,
        out: &mut impl Sink,
    ) -> Option<Arc<Sequence>> {
        match command {
            Command::Load(sequence) => {
                self.silence(out);
                let old = self.scheduler.replace(Scheduler::new(sequence));
                self.state = PlayState::Stopped;
                return old.map(|s| Arc::clone(s.sequence()));
            }
            Command::Play => {
                if let Some(s) = &mut self.scheduler
                    && self.state != PlayState::Playing
                {
                    if self.state == PlayState::Stopped || s.finished() {
                        // From the start: the setup (programs, volume, pan) comes with the
                        // first events.
                        s.jump(0, now);
                    } else {
                        s.resume(now);
                    }
                    self.state = PlayState::Playing;
                }
            }
            Command::Pause => {
                if self.state == PlayState::Playing {
                    self.silence(out);
                    self.state = PlayState::Paused;
                }
            }
            Command::Stop => {
                self.silence(out);
                if let Some(s) = &mut self.scheduler {
                    s.jump(0, now);
                }
                self.state = PlayState::Stopped;
            }
            Command::Send(event) => {
                out.send(event);
            }
            Command::Replace(sequence) => match &mut self.scheduler {
                Some(s) => return Some(s.replace(sequence)),
                None => self.scheduler = Some(Scheduler::new(sequence)),
            },
            Command::Seek(tick) => {
                self.silence(out);
                if let Some(s) = &mut self.scheduler {
                    s.jump(tick, now);
                    if s.position() > 0 {
                        for event in s.setup() {
                            out.send(event.event);
                        }
                    }
                    if self.state == PlayState::Stopped {
                        self.state = PlayState::Paused;
                    }
                }
            }
            Command::TempoScale(scale) => {
                if let Some(s) = &mut self.scheduler {
                    s.set_tempo_scale(scale, now);
                }
            }
            Command::Quit
            | Command::StringModel(..)
            | Command::Chain(..)
            | Command::ChainSettings(..)
            | Command::ReverbBus(..)
            | Command::ReverbBusSettings(..) => {}
        }
        None
    }

    /// Sends the events due at `now`; at the end, silences and stops.
    pub(crate) fn advance(&mut self, now: u64, out: &mut impl Sink) {
        if self.state != PlayState::Playing {
            return;
        }
        let Some(s) = &mut self.scheduler else {
            return;
        };
        for event in s.advance(now) {
            out.send_timed(event);
        }
        if s.finished() {
            self.fail(out);
        }
    }

    /// Silences and stops where it is (the end was reached or the output failed).
    pub(crate) fn fail(&mut self, out: &mut impl Sink) {
        self.silence(out);
        self.state = PlayState::Stopped;
    }

    fn silence(&self, out: &mut impl Sink) {
        if let Some(s) = &self.scheduler {
            out.silence(s.sequence());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]
mod tests {
    use gscore_play::{Channel, PPQ, TempoMap, TimedEvent};

    use super::*;

    /// Records what reaches it.
    #[derive(Default)]
    struct Recorder {
        events: Vec<MidiEvent>,
        silenced: usize,
    }

    impl Sink for Recorder {
        fn send(&mut self, event: MidiEvent) {
            self.events.push(event);
        }
        fn silence(&mut self, _: &Sequence) {
            self.silenced += 1;
        }
    }

    /// A program change and one note per quarter for one 4/4 bar at 120 BPM.
    fn sequence() -> Arc<Sequence> {
        let channel = Channel::new(0).unwrap();
        let mut events = vec![TimedEvent {
            tick: 0,
            track: 0,
            event: MidiEvent::ProgramChange {
                channel,
                program: 25,
            },
        }];
        events.extend((0..4_u64).map(|i| TimedEvent {
            tick: i * u64::from(PPQ),
            track: 0,
            event: MidiEvent::NoteOn {
                channel,
                key: 60,
                velocity: 90,
            },
        }));
        Arc::new(Sequence {
            tracks: Vec::new(),
            events,
            tempo: TempoMap::new(120),
            measures: Vec::new(),
            end: 4 * u64::from(PPQ),
            skipped_notes: 0,
            degraded_pitch_bends: 0,
            performance: Default::default(),
        })
    }

    #[test]
    fn play_pause_seek_and_the_end() {
        let mut out = Recorder::default();
        let mut t = Transport::default();
        // Without a sequence, commands do nothing.
        assert!(t.handle(Command::Play, 0, &mut out).is_none());
        assert_eq!(t.state(), PlayState::Stopped);

        assert!(t.handle(Command::Load(sequence()), 0, &mut out).is_none());
        let _ = t.handle(Command::Play, 1_000, &mut out);
        t.advance(1_000, &mut out);
        assert_eq!(out.events.len(), 2, "setup and the first note");
        t.advance(501_000, &mut out);
        assert_eq!(out.events.len(), 3);

        let _ = t.handle(Command::Pause, 600_000, &mut out);
        assert_eq!(t.state(), PlayState::Paused);
        t.advance(5_000_000, &mut out);
        assert_eq!(out.events.len(), 3, "nothing while paused");
        // Resuming continues where it paused, not where the clock is.
        let _ = t.handle(Command::Play, 5_000_000, &mut out);
        t.advance(5_500_000, &mut out);
        assert_eq!(out.events.len(), 4);

        // Seeking into the piece sends the setup again.
        let _ = t.handle(Command::Seek(3 * u64::from(PPQ)), 6_000_000, &mut out);
        assert_eq!(out.events.len(), 5);
        assert!(matches!(
            out.events.last(),
            Some(MidiEvent::ProgramChange { .. })
        ));
        t.advance(6_000_000, &mut out);
        assert_eq!(out.events.len(), 6);
        t.advance(7_000_000, &mut out);
        assert_eq!(t.state(), PlayState::Stopped, "the end stops playback");
        assert_eq!(t.position(), 4 * u64::from(PPQ));

        // Loading again hands back the old sequence.
        assert!(t.handle(Command::Load(sequence()), 0, &mut out).is_some());
        assert_eq!(t.position(), 0);
        assert!(out.silenced >= 4);
    }

    #[test]
    fn a_live_mix_event_reaches_the_output_without_touching_the_sequence() {
        let mut out = Recorder::default();
        let mut t = Transport::default();
        let channel = Channel::new(3).unwrap();
        let event = MidiEvent::Control {
            channel,
            controller: gscore_play::controller::VOLUME,
            value: 90,
        };
        assert!(t.handle(Command::Send(event), 0, &mut out).is_none());
        assert_eq!(out.events, [event]);
        assert_eq!(t.state(), PlayState::Stopped);
        assert_eq!(t.position(), 0);
    }

    #[test]
    fn a_replaced_sequence_plays_on_without_silence() {
        let mut out = Recorder::default();
        let mut t = Transport::default();
        // Without a sequence, a replacement is a load.
        assert!(
            t.handle(Command::Replace(sequence()), 0, &mut out)
                .is_none()
        );
        let _ = t.handle(Command::Play, 0, &mut out);
        t.advance(600_000, &mut out);
        let (sent, silenced, position) = (out.events.len(), out.silenced, t.position());
        let old = t.handle(Command::Replace(sequence()), 600_000, &mut out);
        assert!(
            old.is_some(),
            "the old sequence comes back to be freed elsewhere"
        );
        assert_eq!(t.state(), PlayState::Playing);
        assert_eq!(t.position(), position);
        assert_eq!(
            (out.events.len(), out.silenced),
            (sent, silenced),
            "nothing stopped"
        );
        t.advance(1_100_000, &mut out);
        assert_eq!(out.events.len(), sent + 1, "the next note, once");
    }

    #[test]
    fn seeking_while_stopped_pauses_there() {
        let mut out = Recorder::default();
        let mut t = Transport::default();
        let _ = t.handle(Command::Load(sequence()), 0, &mut out);
        let _ = t.handle(Command::Seek(u64::from(PPQ)), 0, &mut out);
        assert_eq!(t.state(), PlayState::Paused);
        let _ = t.handle(Command::Play, 0, &mut out);
        t.advance(0, &mut out);
        // Setup from the seek, then the second note (not the first).
        assert_eq!(out.events.len(), 2);
        let _ = t.handle(Command::Stop, 0, &mut out);
        assert_eq!((t.state(), t.position()), (PlayState::Stopped, 0));
    }
}
