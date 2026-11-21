//! MIDI input in the app (M23 steps 3 and 4): the port chosen in Settings › Audio I/O is
//! connected while it is there and connected again when it is plugged back in; its notes
//! are heard at once through the cursor's track (monitoring) and, in step entry, entered
//! at the cursor: the notes that start within a short window make one chord. A MIDI
//! guitar in mono mode (one channel per string) gives each note's string, so its fret is
//! known and the fingering is skipped.
//!
//! Every message is hostile input: it was read by `MidiInput::parse` on the MIDI thread,
//! and a note the track cannot play is a notice, never a panic.

use std::time::Duration;

use crate::Instant;

use gscore_audio::{MidiIn, midi_input_ports};
use gscore_edit::{Neck, Place, StepInput};
use gscore_model::{Fret, StringNumber, TrackIndex};
use gscore_play::{ChordGatherer, ChordNote, MidiInput};

use super::GuitarScoreApp;

/// How often the ports are listed again, to notice one plugged in or out.
const SCAN_EVERY: Duration = Duration::from_secs(2);
/// Most notes monitored at once (a held chord with some to spare).
const MAX_HELD: usize = 32;

/// A monitored note, sounding until its key is released.
#[derive(Debug, Clone, Copy)]
struct Held {
    channel: u8,
    key: u8,
    track: TrackIndex,
    string: u8,
}

/// The MIDI input's connection and what it is in the middle of.
#[derive(Debug, Default)]
pub(super) struct MidiInputs {
    connection: Option<MidiIn>,
    /// The input ports, as last listed.
    ports: Vec<String>,
    scanned: Option<Instant>,
    /// The port a connection failed for, so it is reported once.
    failed: Option<String>,
    chords: ChordGatherer,
    held: Vec<Held>,
}

impl MidiInputs {
    /// The input ports, as last listed.
    pub(super) fn ports(&self) -> &[String] {
        &self.ports
    }

    /// The port it is connected to, if any.
    pub(super) fn connected(&self) -> Option<&str> {
        self.connection.as_ref().map(MidiIn::port)
    }

    /// Lists the ports again at the next frame.
    pub(super) fn rescan(&mut self) {
        self.scanned = None;
    }
}

impl GuitarScoreApp {
    /// Each frame: keeps the chosen input connected, reads what it sent, monitors and
    /// enters it.
    pub(super) fn poll_midi_input(&mut self, ctx: &egui::Context) {
        let Some(port) = self.settings.midi_input_port.clone() else {
            if self.midi_inputs.connection.take().is_some() {
                self.release_held();
            }
            self.midi_inputs.chords.clear();
            return;
        };
        if self
            .midi_inputs
            .connected()
            .is_some_and(|connected| connected != port)
        {
            self.midi_inputs.connection = None;
            self.release_held();
        }
        self.keep_connected(&port, ctx);
        let now_ms = ctx.input(|i| (i.time.max(0.0) * 1000.0) as u64);
        let messages: Vec<MidiInput> = self
            .midi_inputs
            .connection
            .as_ref()
            .map(|connection| connection.messages().collect())
            .unwrap_or_default();
        for message in messages {
            self.midi_message(message, now_ms, ctx);
        }
        if let Some(chord) = self.midi_inputs.chords.poll(now_ms) {
            self.enter_midi_chord(&chord, ctx);
        }
        if let Some(deadline) = self.midi_inputs.chords.deadline_ms() {
            ctx.request_repaint_after(Duration::from_millis(
                deadline.saturating_sub(now_ms).max(1),
            ));
        }
    }

    /// Connects to `port` when it is there, notices it unplugged, and lists the ports now
    /// and then.
    fn keep_connected(&mut self, port: &str, ctx: &egui::Context) {
        let due = self
            .midi_inputs
            .scanned
            .is_none_or(|scanned| scanned.elapsed() >= SCAN_EVERY);
        // Woken again to look for the port once more.
        ctx.request_repaint_after(SCAN_EVERY);
        if !due {
            return;
        }
        self.midi_inputs.scanned = Some(Instant::now());
        match midi_input_ports() {
            Ok(ports) => self.midi_inputs.ports = ports,
            Err(error) => {
                if self.midi_inputs.failed.as_deref() != Some(port) {
                    self.notice = Some(format!("MIDI input: {error}"));
                    self.midi_inputs.failed = Some(port.to_owned());
                }
                return;
            }
        }
        let present = self.midi_inputs.ports.iter().any(|name| name == port);
        if self.midi_inputs.connection.is_some() {
            if !present {
                self.midi_inputs.connection = None;
                self.release_held();
                self.status = Some(format!(
                    "The MIDI input \u{201c}{port}\u{201d} was unplugged; it connects again \
                     when it is back."
                ));
            }
            return;
        }
        if !present {
            return;
        }
        let waker = ctx.clone();
        match MidiIn::connect(port, move || waker.request_repaint()) {
            Ok(connection) => {
                self.midi_inputs.connection = Some(connection);
                self.midi_inputs.failed = None;
                self.status = Some(format!("MIDI input \u{201c}{port}\u{201d} connected."));
            }
            Err(error) => {
                if self.midi_inputs.failed.as_deref() != Some(port) {
                    self.notice = Some(error.to_string());
                    self.midi_inputs.failed = Some(port.to_owned());
                }
            }
        }
    }

    /// One note message: heard, and gathered into the chord to enter.
    fn midi_message(&mut self, message: MidiInput, now_ms: u64, ctx: &egui::Context) {
        let guitar = self.settings.midi_guitar;
        let wanted = |channel: u8| {
            guitar
                || self
                    .settings
                    .midi_input_channel
                    .is_none_or(|only| only == channel.saturating_add(1))
        };
        match message {
            MidiInput::NoteOn {
                channel,
                key,
                velocity,
            } if wanted(channel) => {
                if self.settings.midi_monitor {
                    self.monitor_on(channel, key, velocity);
                }
                if let Some(chord) = self
                    .midi_inputs
                    .chords
                    .note_on(ChordNote { channel, key }, now_ms)
                {
                    self.enter_midi_chord(&chord, ctx);
                }
            }
            MidiInput::NoteOff { channel, key } => self.monitor_off(channel, key),
            MidiInput::NoteOn { .. } => {}
        }
    }

    /// Starts hearing a note on the cursor's track: on a MIDI guitar's string, else on
    /// the string the fingering would choose.
    fn monitor_on(&mut self, channel: u8, key: u8, velocity: u8) {
        // The tab by its field, so the player can be borrowed beside it.
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let song = document.song();
        let cursor = document.editor().cursor();
        let neck = Neck::of_staff(song, &cursor).ok();
        let string = if self.settings.midi_guitar {
            self.guitar_string(channel)
        } else {
            let taken: Vec<Place> = self
                .midi_inputs
                .held
                .iter()
                .filter_map(|held| {
                    Some(Place {
                        string: StringNumber::new(held.string).ok()?,
                        fret: Fret::OPEN,
                    })
                })
                .collect();
            neck.as_ref()
                .and_then(|neck| neck.place_note(key, None, &taken).ok())
                .map(|place| place.string.get())
        }
        .unwrap_or(1);
        if self.midi_inputs.held.len() >= MAX_HELD {
            return;
        }
        let track = cursor.track;
        // Monitoring is a convenience: an output that cannot play stays quiet.
        let _ = self
            .playback
            .monitor_note(song, track, string, key, velocity);
        self.midi_inputs.held.push(Held {
            channel,
            key,
            track,
            string,
        });
    }

    /// Ends a monitored note when its key is released.
    fn monitor_off(&mut self, channel: u8, key: u8) {
        let Some(index) = self
            .midi_inputs
            .held
            .iter()
            .position(|held| held.channel == channel && held.key == key)
        else {
            return;
        };
        let held = self.midi_inputs.held.swap_remove(index);
        if let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) {
            let _ = self
                .playback
                .monitor_note(document.song(), held.track, held.string, key, 0);
        }
    }

    /// Ends every monitored note (the input went away).
    fn release_held(&mut self) {
        for held in std::mem::take(&mut self.midi_inputs.held) {
            if let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) {
                let _ = self.playback.monitor_note(
                    document.song(),
                    held.track,
                    held.string,
                    held.key,
                    0,
                );
            }
        }
    }

    /// The string a MIDI guitar's `channel` (0-based) is: string 1 on the first channel
    /// set, the next strings on the channels after it.
    fn guitar_string(&self, channel: u8) -> Option<u8> {
        let first = self.settings.midi_guitar_first_channel.clamp(1, 16);
        channel.saturating_add(2).checked_sub(first)
    }

    /// Enters a gathered chord in step entry (a single note is a chord of one).
    fn enter_midi_chord(&mut self, chord: &[ChordNote], ctx: &egui::Context) {
        let Some(document) = self.document() else {
            return;
        };
        if !document.editor().step_entry() || chord.is_empty() {
            return;
        }
        let input = if self.settings.midi_guitar {
            let song = document.song();
            let cursor = document.editor().cursor();
            let neck = match Neck::of_staff(song, &cursor) {
                Ok(neck) => neck,
                Err(error) => {
                    self.notice = Some(error.to_string());
                    return;
                }
            };
            let mut places = Vec::with_capacity(chord.len());
            for note in chord {
                match self.guitar_place(&neck, *note) {
                    Some(place) => places.push(place),
                    None => {
                        self.status = Some(format!(
                            "The MIDI guitar's {} on channel {} is not on this track's \
                             strings.",
                            gscore_edit::pitch_name(note.key),
                            u16::from(note.channel).saturating_add(1)
                        ));
                    }
                }
            }
            if places.is_empty() {
                return;
            }
            StepInput::Places(places)
        } else {
            StepInput::Pitches(chord.iter().map(|note| note.key).collect())
        };
        // A monitored input is heard already; otherwise the entered beat plays.
        let play = !self.settings.midi_monitor;
        self.enter_step(input, false, play, ctx);
    }

    /// Where a MIDI guitar's note is: its channel's string, at the fret its key is above
    /// that string's open pitch.
    fn guitar_place(&self, neck: &Neck, note: ChordNote) -> Option<Place> {
        let string = StringNumber::new(self.guitar_string(note.channel)?).ok()?;
        let open = neck.pitch(Place {
            string,
            fret: Fret::OPEN,
        })?;
        let fret = note.key.checked_sub(open).filter(|f| *f <= neck.frets())?;
        Some(Place {
            string,
            fret: Fret::new(fret).ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_midi_guitar_note_is_its_channel_string_and_its_fret() {
        let ctx = egui::Context::default();
        let mut app = GuitarScoreApp::new(&ctx, None);
        app.settings.midi_guitar_first_channel = 1;
        assert_eq!(app.guitar_string(0), Some(1));
        assert_eq!(app.guitar_string(5), Some(6));
        app.settings.midi_guitar_first_channel = 11;
        assert_eq!(app.guitar_string(10), Some(1));
        assert_eq!(app.guitar_string(3), None);
        let neck = Neck::new(
            [64, 59, 55, 50, 45, 40]
                .into_iter()
                .zip(1..)
                .map(|(pitch, n)| (StringNumber::new(n).unwrap(), pitch))
                .collect(),
            24,
        );
        // Channel 16 is string 6 (E2): G2 is its 3rd fret; below E2 it is nowhere.
        let note = |channel, key| ChordNote { channel, key };
        assert_eq!(
            app.guitar_place(&neck, note(15, 43)),
            Some(Place {
                string: StringNumber::new(6).unwrap(),
                fret: Fret::new(3).unwrap()
            })
        );
        assert_eq!(app.guitar_place(&neck, note(15, 39)), None);
        // Past the last fret, or a channel without a string.
        assert_eq!(app.guitar_place(&neck, note(10, 64 + 25)), None);
        assert_eq!(app.guitar_place(&neck, note(9, 64)), None);
    }
}
