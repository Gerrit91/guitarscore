//! A MIDI input port (M23 step 3), through `midir`: a keyboard or a MIDI guitar playing
//! notes into step entry. `midir` calls back on a thread of its own with each message; the
//! callback reads it ([`MidiInput::parse`], which never panics) and hands note messages
//! over through a bounded queue without waiting. When the queue is full (the app has
//! stopped reading), messages are dropped, never blocking the MIDI thread.

use std::sync::mpsc::{self, Receiver, TrySendError};

use gscore_play::{MidiInput, PlayError};
use midir::{Ignore, MidiInputConnection};

/// Client name shown to other MIDI programs.
const CLIENT: &str = "GuitarScore";
/// Note messages queued until the app reads them (a frame's worth is a handful).
const QUEUE: usize = 256;

/// The names of the MIDI input ports, in system order.
pub fn midi_input_ports() -> Result<Vec<String>, PlayError> {
    let input = midir::MidiInput::new(CLIENT).map_err(|e| PlayError::Device(e.to_string()))?;
    Ok(input
        .ports()
        .iter()
        .filter_map(|port| input.port_name(port).ok())
        .collect())
}

/// A connection to one MIDI input port. Dropping it disconnects.
pub struct MidiIn {
    port: String,
    /// Keeps the port open; `midir` closes it when this is dropped.
    _connection: MidiInputConnection<()>,
    messages: Receiver<MidiInput>,
}

impl std::fmt::Debug for MidiIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiIn").field("port", &self.port).finish()
    }
}

impl MidiIn {
    /// Connects to the input port called exactly `port`. `wake` is called (on the MIDI
    /// thread) after each note message, so the app can read it at once.
    pub fn connect(port: &str, wake: impl Fn() + Send + 'static) -> Result<Self, PlayError> {
        let mut input =
            midir::MidiInput::new(CLIENT).map_err(|e| PlayError::Device(e.to_string()))?;
        // System exclusive, clock and active sensing carry nothing step entry uses.
        input.ignore(Ignore::All);
        let ports = input.ports();
        let chosen = ports
            .iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(port))
            .ok_or_else(|| PlayError::Device(format!("the MIDI input \"{port}\" is not there")))?;
        let (sender, messages) = mpsc::sync_channel(QUEUE);
        let connection = input
            .connect(
                chosen,
                "GuitarScore in",
                move |_stamp, bytes, _| {
                    if let Some(message) = MidiInput::parse(bytes) {
                        // A full queue drops the note; a gone receiver ends nothing here.
                        match sender.try_send(message) {
                            Ok(()) => wake(),
                            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
                        }
                    }
                },
                (),
            )
            .map_err(|e| PlayError::Device(format!("cannot connect to {port}: {e}")))?;
        Ok(MidiIn {
            port: port.to_owned(),
            _connection: connection,
            messages,
        })
    }

    /// The port it is connected to.
    pub fn port(&self) -> &str {
        &self.port
    }

    /// The note messages that arrived since the last call, oldest first.
    pub fn messages(&self) -> impl Iterator<Item = MidiInput> + '_ {
        self.messages.try_iter()
    }
}
